use zksync_basic_types::{web3::Bytes, Address, SLChainId};

use super::chain::AllContractsConfig;

/// Config that is only stored for the gateway chain.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct GatewayConfig {
    pub state_transition_proxy_addr: Address,
    pub state_transition_implementation_addr: Address,
    pub verifier_addr: Address,
    pub validator_timelock_addr: Address,
    pub admin_facet_addr: Address,
    pub mailbox_facet_addr: Address,
    pub executor_facet_addr: Address,
    pub getters_facet_addr: Address,
    pub diamond_init_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub default_upgrade_addr: Address,
    pub multicall3_addr: Address,
    pub relayed_sl_da_validator: Address,
    pub validium_da_validator: Address,
    pub diamond_cut_data: Bytes,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct GatewayChainConfig {
    pub state_transition_proxy_addr: Option<Address>,
    pub validator_timelock_addr: Option<Address>,
    pub multicall3_addr: Address,
    pub diamond_proxy_addr: Address,
    pub chain_admin_addr: Address,
    pub gateway_chain_id: SLChainId,
    pub server_notifier: Option<Address>,
}

impl GatewayChainConfig {
    pub fn from_gateway_and_chain_data(
        gateway_config: &GatewayConfig,
        diamond_proxy_addr: Address,
        l2_chain_admin_addr: Address,
        gateway_chain_id: SLChainId,
    ) -> Self {
        Self {
            state_transition_proxy_addr: Some(gateway_config.state_transition_proxy_addr),
            validator_timelock_addr: Some(gateway_config.validator_timelock_addr),
            multicall3_addr: gateway_config.multicall3_addr,
            diamond_proxy_addr,
            chain_admin_addr: l2_chain_admin_addr,
            gateway_chain_id,
            server_notifier: None,
        }
    }

    pub fn from_contracts_and_chain_id(
        contracts: AllContractsConfig,
        gateway_chain_id: SLChainId,
    ) -> Self {
        Self {
            state_transition_proxy_addr: contracts
                .ecosystem_contracts
                .map(|a| a.state_transition_proxy_addr)
                .flatten(),
            validator_timelock_addr: Some(contracts.validator_timelock_addr),
            multicall3_addr: contracts.l1_multicall3_addr,
            diamond_proxy_addr: contracts.diamond_proxy_addr,
            chain_admin_addr: contracts.chain_admin_addr,
            gateway_chain_id,
            server_notifier: None,
        }
    }
}
