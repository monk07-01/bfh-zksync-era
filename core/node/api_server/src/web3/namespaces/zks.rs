use std::collections::HashMap;

use anyhow::Context as _;
use zksync_crypto_primitives::hasher::{keccak::KeccakHasher, Hasher};
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_metadata_calculator::api_server::TreeApiError;
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_multivm::interface::VmEvent;
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    address_to_h256,
    api::{
        self, state_override::StateOverride, BlockDetails, BridgeAddresses, GetLogsFilter,
        L1BatchDetails, L2ToL1LogProof, Proof, ProtocolVersion, StorageProof,
        TransactionDetailedResult, TransactionDetails,
    },
    fee::Fee,
    fee_model::{FeeParams, PubdataIndependentBatchFeeModelInput},
    h256_to_u256,
    l1::L1Tx,
    l2::L2Tx,
    l2_to_l1_log::{l2_to_l1_logs_tree_size, L2ToL1Log, LOG_PROOF_SUPPORTED_METADATA_VERSION},
    tokens::ETHEREUM_ADDRESS,
    transaction_request::CallRequest,
    utils::storage_key_for_standard_token_balance,
    web3,
    web3::Bytes,
    AccountTreeId, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey, Transaction,
    L1_MESSENGER_ADDRESS, L2_BASE_TOKEN_ADDRESS, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256, U64,
};
use zksync_web3_decl::{
    error::{ClientRpcContext, Web3Error},
    namespaces::ZksNamespaceClient,
    types::{Address, Token, H256},
};

use crate::{
    execution_sandbox::BlockArgs,
    tx_sender::BinarySearchKind,
    utils::open_readonly_transaction,
    web3::{backend_jsonrpsee::MethodTracer, metrics::API_METRICS, RpcState},
};

#[derive(Debug)]
pub(crate) struct ZksNamespace {
    state: RpcState,
}

impl ZksNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    pub async fn estimate_fee_impl(
        &self,
        request: CallRequest,
        state_override: Option<StateOverride>,
    ) -> Result<Fee, Web3Error> {
        let mut request_with_gas_per_pubdata_overridden = request;
        self.state
            .set_nonce_for_call_request(&mut request_with_gas_per_pubdata_overridden)
            .await?;

        if let Some(ref mut eip712_meta) = request_with_gas_per_pubdata_overridden.eip712_meta {
            eip712_meta.gas_per_pubdata = U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE);
        }

        let mut connection = self.state.acquire_connection().await?;
        let block_args = BlockArgs::pending(&mut connection).await?;
        drop(connection);
        let mut tx = L2Tx::from_request(
            request_with_gas_per_pubdata_overridden.into(),
            self.state.api_config.max_tx_size,
            block_args.use_evm_emulator(),
        )?;

        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        tx.common_data.fee.max_priority_fee_per_gas = 0u64.into();
        tx.common_data.fee.gas_per_pubdata_limit = U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE);
        self.estimate_fee(tx.into(), block_args, state_override)
            .await
    }

    pub async fn estimate_l1_to_l2_gas_impl(
        &self,
        request: CallRequest,
        state_override: Option<StateOverride>,
    ) -> Result<U256, Web3Error> {
        let mut request_with_gas_per_pubdata_overridden = request;
        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        if let Some(ref mut eip712_meta) = request_with_gas_per_pubdata_overridden.eip712_meta {
            if eip712_meta.gas_per_pubdata == U256::zero() {
                eip712_meta.gas_per_pubdata = REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into();
            }
        }

        let mut connection = self.state.acquire_connection().await?;
        let block_args = BlockArgs::pending(&mut connection).await?;
        drop(connection);
        let tx = L1Tx::from_request(
            request_with_gas_per_pubdata_overridden,
            block_args.use_evm_emulator(),
        )
        .map_err(Web3Error::SerializationError)?;

        let fee = self
            .estimate_fee(tx.into(), block_args, state_override)
            .await?;
        Ok(fee.gas_limit)
    }

    async fn estimate_fee(
        &self,
        tx: Transaction,
        block_args: BlockArgs,
        state_override: Option<StateOverride>,
    ) -> Result<Fee, Web3Error> {
        let scale_factor = self.state.api_config.estimate_gas_scale_factor;
        let acceptable_overestimation =
            self.state.api_config.estimate_gas_acceptable_overestimation;
        let search_kind = BinarySearchKind::new(self.state.api_config.estimate_gas_optimize_search);

        Ok(self
            .state
            .tx_sender
            .get_txs_fee_in_wei(
                tx,
                block_args,
                scale_factor,
                acceptable_overestimation as u64,
                state_override,
                search_kind,
            )
            .await?)
    }

    pub fn get_bridgehub_contract_impl(&self) -> Option<Address> {
        self.state.api_config.l1_bridgehub_proxy_addr
    }

    pub fn get_main_contract_impl(&self) -> Address {
        self.state.api_config.l1_diamond_proxy_addr
    }

    pub fn get_testnet_paymaster_impl(&self) -> Option<Address> {
        self.state.api_config.l2_testnet_paymaster_addr
    }

    pub async fn get_bridge_contracts_impl(&self) -> BridgeAddresses {
        self.state.bridge_addresses_handle.read().await
    }

    pub fn get_timestamp_asserter_impl(&self) -> Option<Address> {
        self.state.api_config.timestamp_asserter_address
    }

    pub fn l1_chain_id_impl(&self) -> U64 {
        U64::from(*self.state.api_config.l1_chain_id)
    }

    pub async fn get_confirmed_tokens_impl(
        &self,
        from: u32,
        limit: u8,
    ) -> Result<Vec<Token>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let tokens = storage
            .tokens_web3_dal()
            .get_well_known_tokens()
            .await
            .map_err(DalError::generalize)?;

        let tokens = tokens
            .into_iter()
            .skip(from as usize)
            .take(limit.into())
            .map(|token_info| Token {
                l1_address: token_info.l1_address,
                l2_address: token_info.l2_address,
                name: token_info.metadata.name,
                symbol: token_info.metadata.symbol,
                decimals: token_info.metadata.decimals,
            })
            .collect();
        Ok(tokens)
    }

    pub async fn get_all_account_balances_impl(
        &self,
        address: Address,
    ) -> Result<HashMap<Address, U256>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let tokens = storage
            .tokens_dal()
            .get_all_l2_token_addresses()
            .await
            .map_err(DalError::generalize)?;
        let hashed_balance_keys = tokens.iter().map(|&token_address| {
            let token_account = AccountTreeId::new(if token_address == ETHEREUM_ADDRESS {
                L2_BASE_TOKEN_ADDRESS
            } else {
                token_address
            });
            let hashed_key =
                storage_key_for_standard_token_balance(token_account, &address).hashed_key();
            (hashed_key, (hashed_key, token_address))
        });
        let (hashed_balance_keys, hashed_key_to_token_address): (Vec<_>, HashMap<_, _>) =
            hashed_balance_keys.unzip();

        let balance_values = storage
            .storage_web3_dal()
            .get_values(&hashed_balance_keys)
            .await
            .map_err(DalError::generalize)?;

        let balances = balance_values
            .into_iter()
            .filter_map(|(hashed_key, balance)| {
                let balance = h256_to_u256(balance);
                if balance.is_zero() {
                    return None;
                }
                Some((hashed_key_to_token_address[&hashed_key], balance))
            })
            .collect();
        Ok(balances)
    }

    pub async fn get_l2_to_l1_msg_proof_impl(
        &self,
        block_number: L2BlockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        if let Some(handler) = &self.state.l2_l1_log_proof_handler {
            return handler
                .get_l2_to_l1_msg_proof(block_number, sender, msg, l2_log_position)
                .rpc_context("get_l2_to_l1_msg_proof")
                .await
                .map_err(Into::into);
        }

        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        let Some(l1_batch_number) = storage
            .blocks_web3_dal()
            .get_l1_batch_number_of_l2_block(block_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };
        let (first_l2_block_of_l1_batch, _) = storage
            .blocks_web3_dal()
            .get_l2_block_range_of_l1_batch(l1_batch_number)
            .await
            .map_err(DalError::generalize)?
            .context("L1 batch should contain at least one L2 block")?;

        // Position of l1 log in L1 batch relative to logs with identical data
        let l1_log_relative_position = if let Some(l2_log_position) = l2_log_position {
            let logs = storage
                .events_web3_dal()
                .get_logs(
                    GetLogsFilter {
                        from_block: first_l2_block_of_l1_batch,
                        to_block: block_number,
                        addresses: vec![L1_MESSENGER_ADDRESS],
                        topics: vec![(2, vec![address_to_h256(&sender)]), (3, vec![msg])],
                    },
                    self.state.api_config.req_entities_limit,
                )
                .await
                .map_err(DalError::generalize)?;
            let maybe_pos = logs.iter().position(|event| {
                event.block_number == Some(block_number.0.into())
                    && event.log_index == Some(l2_log_position.into())
            });
            match maybe_pos {
                Some(pos) => pos,
                None => return Ok(None),
            }
        } else {
            0
        };

        let log_proof = self
            .get_l2_to_l1_log_proof_inner(
                &mut storage,
                l1_batch_number,
                l1_log_relative_position,
                |log| {
                    log.sender == L1_MESSENGER_ADDRESS
                        && log.key == address_to_h256(&sender)
                        && log.value == msg
                },
                None,
                None,
            )
            .await?;
        Ok(log_proof)
    }

    // pub async fn get_l2_to_global_message_root_proof_impl(
    //     &self,
    //     block_number: L2BlockNumber,
    //     sender: Address,
    //     msg: H256,
    // ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
    //     todo!() // kl todo
    // }

    async fn get_l2_to_l1_log_proof_inner(
        &self,
        storage: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
        index_in_filtered_logs: usize,
        log_filter: impl Fn(&L2ToL1Log) -> bool,
        proof_until_chain_id: Option<U64>,
        precommit_log_index: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let all_l1_logs_in_batch = storage
            .blocks_web3_dal()
            .get_l2_to_l1_logs(l1_batch_number)
            .await
            .map_err(DalError::generalize)?;

        let Some((l1_log_index, _)) = all_l1_logs_in_batch
            .iter()
            .enumerate()
            .filter(|(_, log)| log_filter(log))
            .nth(index_in_filtered_logs)
        else {
            return Ok(None);
        };
        if let Some(precommit_log_index) = precommit_log_index {
            if l1_log_index > precommit_log_index {
                return Ok(None);
            }
        }

        let Some(batch_with_metadata) = storage
            .blocks_dal()
            .get_l1_batch_metadata(l1_batch_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        let merkle_tree_leaves = all_l1_logs_in_batch.iter().map(L2ToL1Log::to_bytes);
        // let merkle_tree_leaves = all_l1_logs_in_batch[..precommit_log_index.unwrap_or(all_l1_logs_in_batch.len())]
        // .iter()
        // .map(L2ToL1Log::to_bytes);

        let protocol_version = batch_with_metadata
            .header
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);
        let tree_size = l2_to_l1_logs_tree_size(protocol_version);
        // println!("kl toodo merkle tree leaves: {:?}", merkle_tree_leaves);
        let (local_root, proof) = MiniMerkleTree::new(merkle_tree_leaves, None) //, Some(tree_size))
            .merkle_root_and_path(l1_log_index);

        if protocol_version.is_pre_gateway() {
            return Ok(Some(L2ToL1LogProof {
                proof,
                root: local_root,
                id: l1_log_index as u32,
            }));
        }

        let aggregated_root = batch_with_metadata
            .metadata
            .aggregation_root
            .expect("`aggregation_root` must be present for post-gateway branch");
        let root = KeccakHasher.compress(&local_root, &aggregated_root);

        let mut log_leaf_proof = proof;
        log_leaf_proof.push(aggregated_root);

        let Some(sl_chain_id) = storage
            .eth_sender_dal()
            .get_batch_execute_chain_id(l1_batch_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        let (batch_proof_len, batch_chain_proof, is_final_node) =
            // if we provide the GW chain id, we don't want to extend to L1.
            if Some(U64::from(sl_chain_id.0)) == proof_until_chain_id {
                (0, Vec::new(), true)
            } else if sl_chain_id.0 != self.state.api_config.l1_chain_id.0 {
                let Some(batch_chain_proof) = storage
                    .blocks_dal()
                    .get_l1_batch_chain_merkle_path(l1_batch_number)
                    .await
                    .map_err(DalError::generalize)?
                else {
                    return Ok(None);
                };

                (
                    batch_chain_proof.batch_proof_len,
                    batch_chain_proof.proof,
                    false,
                )
            } else {
                (0, Vec::new(), true)
            };

        let proof = {
            let mut metadata = [0u8; 32];
            metadata[0] = LOG_PROOF_SUPPORTED_METADATA_VERSION;
            metadata[1] = log_leaf_proof.len() as u8;
            metadata[2] = batch_proof_len as u8;
            metadata[3] = if is_final_node { 1 } else { 0 };

            let mut result = vec![H256(metadata)];

            result.extend(log_leaf_proof);
            result.extend(batch_chain_proof);

            result
        };

        Ok(Some(L2ToL1LogProof {
            proof,
            root,
            id: l1_log_index as u32,
        }))
    }

    pub async fn get_l2_to_l1_log_proof_impl(
        &self,
        tx_hash: H256,
        index: Option<usize>,
        proof_until_chain_id: Option<U64>,
        precommit_log_index: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        if let Some(handler) = &self.state.l2_l1_log_proof_handler {
            if let Some(proof_until_chain_id) = proof_until_chain_id {
                return handler
                    .get_l2_to_l1_log_proof_until_chain_id(
                        tx_hash,
                        index,
                        Some(proof_until_chain_id),
                    )
                    .rpc_context("get_l2_to_l1_log_proof_until_chain_id")
                    .await
                    .map_err(Into::into);
            }
            return handler
                .get_l2_to_l1_log_proof_precommit(tx_hash, index, precommit_log_index)
                .rpc_context("get_l2_to_l1_log_proof_precommit")
                .await
                .map_err(Into::into);
        }

        let mut storage = self.state.acquire_connection().await?;
        // kl todo for precommit based, we need it based on blocks.
        // if precommit_log_index.is_none() {
        let Some((l1_batch_number, l1_batch_tx_index)) = storage
            .blocks_web3_dal()
            .get_l1_batch_info_for_tx(tx_hash)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        self.state
            .start_info
            .ensure_not_pruned(l1_batch_number, &mut storage)
            .await?;
        // }

        let log_proof = self
            .get_l2_to_l1_log_proof_inner(
                &mut storage,
                l1_batch_number,
                index.unwrap_or(0),
                |log| log.tx_number_in_block == l1_batch_tx_index,
                proof_until_chain_id,
                precommit_log_index,
            )
            .await?;
        Ok(log_proof)
    }

    pub async fn get_l1_batch_number_impl(&self) -> Result<U64, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .map_err(DalError::generalize)?
            .ok_or(Web3Error::NoBlock)?;
        Ok(l1_batch_number.0.into())
    }

    pub async fn get_l2_block_range_impl(
        &self,
        batch: L1BatchNumber,
    ) -> Result<Option<(U64, U64)>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(batch, &mut storage)
            .await?;
        let range = storage
            .blocks_web3_dal()
            .get_l2_block_range_of_l1_batch(batch)
            .await
            .map_err(DalError::generalize)?;
        Ok(range.map(|(min, max)| (U64::from(min.0), U64::from(max.0))))
    }

    pub async fn get_block_details_impl(
        &self,
        block_number: L2BlockNumber,
    ) -> Result<Option<BlockDetails>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        Ok(storage
            .blocks_web3_dal()
            .get_block_details(block_number)
            .await
            .map_err(DalError::generalize)?)
    }

    pub async fn get_raw_block_transactions_impl(
        &self,
        block_number: L2BlockNumber,
    ) -> Result<Vec<Transaction>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        Ok(storage
            .transactions_web3_dal()
            .get_raw_l2_block_transactions(block_number)
            .await
            .map_err(DalError::generalize)?)
    }

    pub async fn get_transaction_details_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionDetails>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        // Open a readonly transaction to have a consistent view of Postgres
        let mut storage = open_readonly_transaction(&mut storage).await?;
        let mut tx_details = storage
            .transactions_web3_dal()
            .get_transaction_details(hash)
            .await
            .map_err(DalError::generalize)?;

        if tx_details.is_none() {
            tx_details = self
                .state
                .tx_sink()
                .lookup_tx_details(&mut storage, hash)
                .await?;
        }
        Ok(tx_details)
    }

    pub async fn get_l1_batch_details_impl(
        &self,
        batch_number: L1BatchNumber,
    ) -> Result<Option<L1BatchDetails>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(batch_number, &mut storage)
            .await?;

        Ok(storage
            .blocks_web3_dal()
            .get_l1_batch_details(batch_number)
            .await
            .map_err(DalError::generalize)?)
    }

    pub async fn get_bytecode_by_hash_impl(
        &self,
        hash: H256,
    ) -> Result<Option<Vec<u8>>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        Ok(storage
            .factory_deps_dal()
            .get_sealed_factory_dep(hash)
            .await
            .map_err(DalError::generalize)?)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_fee_params_impl(&self) -> FeeParams {
        self.state
            .tx_sender
            .0
            .batch_fee_input_provider
            .get_fee_model_params()
    }

    pub async fn get_protocol_version_impl(
        &self,
        version_id: Option<u16>,
    ) -> Result<Option<ProtocolVersion>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let protocol_version = if let Some(id) = version_id {
            storage
                .protocol_versions_web3_dal()
                .get_protocol_version_by_id(id)
                .await
                .map_err(DalError::generalize)?
        } else {
            Some(
                storage
                    .protocol_versions_web3_dal()
                    .get_latest_protocol_version()
                    .await
                    .map_err(DalError::generalize)?,
            )
        };
        Ok(protocol_version)
    }

    pub async fn get_proofs_impl(
        &self,
        address: Address,
        keys: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<Proof>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(l1_batch_number, &mut storage)
            .await?;
        let hashed_keys = keys
            .iter()
            .map(|key| StorageKey::new(AccountTreeId::new(address), *key).hashed_key_u256())
            .collect();
        let tree_api = self
            .state
            .tree_api
            .as_deref()
            .ok_or(Web3Error::MethodNotImplemented)?;
        let proofs_result = tree_api.get_proofs(l1_batch_number, hashed_keys).await;
        let proofs = match proofs_result {
            Ok(proofs) => proofs,
            Err(TreeApiError::NotReady(_)) => return Err(Web3Error::TreeApiUnavailable),
            Err(TreeApiError::NoVersion(err)) => {
                return if err.missing_version > err.version_count {
                    Ok(None)
                } else {
                    Err(Web3Error::InternalError(anyhow::anyhow!(
                        "L1 batch #{l1_batch_number} is pruned in Merkle tree, but not in Postgres"
                    )))
                };
            }
            Err(TreeApiError::Internal(err)) => return Err(Web3Error::InternalError(err)),
            Err(_) => {
                // This branch is not expected to be executed, but has to be provided since the error is non-exhaustive.
                return Err(Web3Error::InternalError(anyhow::anyhow!(
                    "Unspecified tree API error"
                )));
            }
        };

        let storage_proof = proofs
            .into_iter()
            .zip(keys)
            .map(|(proof, key)| StorageProof {
                key,
                proof: proof.merkle_path,
                value: proof.value,
                index: proof.index,
            })
            .collect();

        Ok(Some(Proof {
            address,
            storage_proof,
        }))
    }

    pub fn get_base_token_l1_address_impl(&self) -> Result<Address, Web3Error> {
        self.state
            .api_config
            .base_token_address
            .ok_or(Web3Error::MethodNotImplemented)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_batch_fee_input_impl(
        &self,
    ) -> Result<PubdataIndependentBatchFeeModelInput, Web3Error> {
        Ok(self
            .state
            .tx_sender
            .scaled_batch_fee_input()
            .await?
            .into_pubdata_independent())
    }

    #[tracing::instrument(skip(self, tx_bytes))]
    pub async fn send_raw_transaction_with_detailed_output_impl(
        &self,
        tx_bytes: Bytes,
    ) -> Result<TransactionDetailedResult, Web3Error> {
        let mut connection = self.state.acquire_connection().await?;
        let block_args = BlockArgs::pending(&mut connection).await?;
        drop(connection);
        let (mut tx, tx_hash) = self
            .state
            .parse_transaction_bytes(&tx_bytes.0, &block_args)?;
        tx.set_input(tx_bytes.0, tx_hash);

        let submit_output = self
            .state
            .tx_sender
            .submit_tx(tx, block_args)
            .await
            .map_err(|err| {
                tracing::debug!("Send raw transaction error: {err}");
                API_METRICS.submit_tx_error[&err.prom_error_code()].inc();
                err
            })?;
        Ok(TransactionDetailedResult {
            transaction_hash: tx_hash,
            storage_logs: submit_output
                .write_logs
                .into_iter()
                .map(Into::into)
                .collect(),
            events: submit_output
                .events
                .into_iter()
                .map(|event| map_event(event, tx_hash))
                .collect(),
        })
    }
}

fn map_event(vm_event: VmEvent, tx_hash: H256) -> api::Log {
    api::Log {
        address: vm_event.address,
        topics: vm_event.indexed_topics,
        data: web3::Bytes::from(vm_event.value),
        block_hash: None,
        block_number: None,
        l1_batch_number: Some(U64::from(vm_event.location.0 .0)),
        transaction_hash: Some(tx_hash),
        transaction_index: Some(web3::Index::from(vm_event.location.1)),
        log_index: None,
        transaction_log_index: None,
        log_type: None,
        removed: Some(false),
        block_timestamp: None,
    }
}
