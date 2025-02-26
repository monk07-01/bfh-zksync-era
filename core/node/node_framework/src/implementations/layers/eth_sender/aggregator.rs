use anyhow::Context;
use zksync_circuit_breaker::l1_txs::FailedL1TransactionChecker;
use zksync_config::configs::{eth_sender::EthConfig, gateway::GatewayChainConfig, ContractsConfig};
use zksync_eth_client::BoundEthInterface;
use zksync_eth_sender::{Aggregator, EthTxAggregator};
use zksync_types::{commitment::L1BatchCommitmentMode, settlement::SettlementMode, L2ChainId};

use crate::{
    implementations::resources::{
        circuit_breakers::CircuitBreakersResource,
        eth_interface::{
            BoundEthInterfaceForBlobsResource, BoundEthInterfaceForL2Resource,
            BoundEthInterfaceResource,
        },
        healthcheck::AppHealthCheckResource,
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource, ReplicaPool},
        settlement_layer::SettlementModeResource,
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for aggregating l1 batches into `eth_txs`
///
/// Responsible for initialization and running of [`EthTxAggregator`], that aggregates L1 batches
/// into `eth_txs`(such as `CommitBlocks`, `PublishProofBlocksOnchain` or `ExecuteBlock`).
/// These `eth_txs` will be used as a queue for generating signed txs and will be sent later on L1.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `PoolResource<ReplicaPool>`
/// - `BoundEthInterfaceResource`
/// - `BoundEthInterfaceForBlobsResource` (optional)
/// - `ObjectStoreResource`
/// - `CircuitBreakersResource` (adds a circuit breaker)
///
/// ## Adds tasks
///
/// - `EthTxAggregator`
#[derive(Debug)]
pub struct EthTxAggregatorLayer {
    eth_sender_config: EthConfig,
    contracts_config: ContractsConfig,
    gateway_chain_config: Option<GatewayChainConfig>,
    zksync_network_id: L2ChainId,
    l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub replica_pool: PoolResource<ReplicaPool>,
    pub eth_client: Option<BoundEthInterfaceResource>,
    pub eth_client_blobs: Option<BoundEthInterfaceForBlobsResource>,
    pub eth_client_gateway: Option<BoundEthInterfaceForL2Resource>,
    pub object_store: ObjectStoreResource,
    pub settlement_mode: SettlementModeResource,
    #[context(default)]
    pub circuit_breakers: CircuitBreakersResource,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub eth_tx_aggregator: EthTxAggregator,
}

impl EthTxAggregatorLayer {
    pub fn new(
        eth_sender_config: EthConfig,
        contracts_config: ContractsConfig,
        gateway_chain_config: Option<GatewayChainConfig>,
        zksync_network_id: L2ChainId,
        l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            eth_sender_config,
            contracts_config,
            gateway_chain_config,
            zksync_network_id,
            l1_batch_commit_data_generator_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for EthTxAggregatorLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "eth_tx_aggregator_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        tracing::info!(
            "Wiring tx_aggregator in {:?} mode which is {}",
            input.settlement_mode.0,
            input.settlement_mode.0.is_gateway()
        );
        tracing::info!("Contracts: {:?}", self.contracts_config);
        tracing::info!("Gateway contracts: {:?}", self.gateway_chain_config);
        // Get resources.

        let (
            validator_timelock_addr,
            multicall3_addr,
            diamond_proxy_addr,
            state_transition_manager_address,
        ) = if input.settlement_mode.0.is_gateway() {
            let gateway_chain_config = self
                .gateway_chain_config
                .as_ref()
                .context("gateway_chain_config")?;
            (
                gateway_chain_config.validator_timelock_addr,
                gateway_chain_config.multicall3_addr,
                gateway_chain_config.diamond_proxy_addr,
                gateway_chain_config.state_transition_proxy_addr,
            )
        } else {
            (
                self.contracts_config.validator_timelock_addr,
                self.contracts_config.l1_multicall3_addr,
                self.contracts_config.diamond_proxy_addr,
                self.contracts_config
                    .ecosystem_contracts
                    .context("Missing ecosystem contracts")?
                    .state_transition_proxy_addr,
            )
        };

        let eth_client = if input.settlement_mode.0.is_gateway() {
            input
                .eth_client_gateway
                .context("eth_client_gateway missing")?
                .0
        } else {
            input.eth_client.context("eth_client missing")?.0
        };
        let master_pool = input.master_pool.get().await.unwrap();
        let replica_pool = input.replica_pool.get().await.unwrap();

        let eth_client_blobs = input.eth_client_blobs.map(|c| c.0);
        let object_store = input.object_store.0;

        // Create and add tasks.
        let eth_client_blobs_addr = eth_client_blobs
            .as_deref()
            .map(BoundEthInterface::sender_account);

        let config = self.eth_sender_config.sender.context("sender")?;
        let aggregator = Aggregator::new(
            config.clone(),
            object_store,
            eth_client_blobs_addr,
            self.l1_batch_commit_data_generator_mode,
            replica_pool.clone(),
            input.settlement_mode.0,
        )
        .await?;

        let eth_tx_aggregator = EthTxAggregator::new(
            master_pool.clone(),
            config.clone(),
            aggregator,
            eth_client,
            validator_timelock_addr,
            state_transition_manager_address,
            multicall3_addr,
            diamond_proxy_addr,
            self.zksync_network_id,
            eth_client_blobs_addr,
            input.settlement_mode.0,
        )
        .await;

        // Insert circuit breaker.
        input
            .circuit_breakers
            .breakers
            .insert(Box::new(FailedL1TransactionChecker { pool: replica_pool }))
            .await;

        input
            .app_health
            .0
            .insert_component(eth_tx_aggregator.health_check())
            .map_err(WiringError::internal)?;

        Ok(Output { eth_tx_aggregator })
    }
}

#[async_trait::async_trait]
impl Task for EthTxAggregator {
    fn id(&self) -> TaskId {
        "eth_tx_aggregator".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
