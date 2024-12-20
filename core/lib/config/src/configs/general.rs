use smart_config::{ConfigSchema, DescribeConfig, DeserializeConfig};

use crate::{
    configs::{
        base_token_adjuster::BaseTokenAdjusterConfig,
        chain::{
            CircuitBreakerConfig, MempoolConfig, OperationsManagerConfig, StateKeeperConfig,
            TimestampAsserterConfig,
        },
        consensus::ConsensusConfig,
        contracts::L1ContractsConfig,
        da_dispatcher::DADispatcherConfig,
        en_config::ENConfig,
        fri_prover_group::FriProverGroupConfig,
        house_keeper::HouseKeeperConfig,
        prover_job_monitor::ProverJobMonitorConfig,
        pruning::PruningConfig,
        snapshot_recovery::SnapshotRecoveryConfig,
        vm_runner::{BasicWitnessInputProducerConfig, ProtectiveReadsWriterConfig},
        wallets::Wallets,
        CommitmentGeneratorConfig, EcosystemContracts, ExperimentalVmConfig,
        ExternalPriceApiClientConfig, FriProofCompressorConfig, FriProverConfig,
        FriProverGatewayConfig, FriWitnessGeneratorConfig, FriWitnessVectorGeneratorConfig,
        ObservabilityConfig, PrometheusConfig, ProofDataHandlerConfig, Secrets,
    },
    ApiConfig, ContractVerifierConfig, ContractsConfig, DBConfig, EthConfig,
    ExternalProofIntegrationApiConfig, GenesisConfigWrapper, ObjectStoreConfig, PostgresConfig,
    SnapshotsCreatorConfig,
};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct GeneralConfig {
    #[config(nest, rename = "postgres", alias = "database")]
    pub postgres_config: PostgresConfig,
    #[config(nest, rename = "api")]
    pub api_config: Option<ApiConfig>,
    #[config(nest)]
    pub contract_verifier: ContractVerifierConfig,
    #[config(nest, rename = "circuit_breaker")]
    pub circuit_breaker_config: CircuitBreakerConfig,
    #[config(nest, rename = "mempool")]
    pub mempool_config: MempoolConfig,
    #[config(nest, rename = "operations_manager")]
    pub operations_manager_config: OperationsManagerConfig,
    #[config(nest, rename = "state_keeper")]
    pub state_keeper_config: Option<StateKeeperConfig>,
    #[config(nest, rename = "house_keeper")]
    pub house_keeper_config: HouseKeeperConfig,

    #[config(nest, rename = "proof_compressor", alias = "fri_proof_compressor")]
    pub proof_compressor_config: Option<FriProofCompressorConfig>,
    #[config(nest, rename = "prover", alias = "fri_prover")]
    pub prover_config: Option<FriProverConfig>,
    #[config(nest, alias = "fri_prover_gateway")]
    pub prover_gateway: Option<FriProverGatewayConfig>,
    #[config(nest, alias = "fri_witness_vector_generator")]
    pub witness_vector_generator: Option<FriWitnessVectorGeneratorConfig>,
    #[config(nest, rename = "prover_group", alias = "fri_prover_group")]
    pub prover_group_config: Option<FriProverGroupConfig>,
    #[config(nest, rename = "witness_generator", alias = "fri_witness")]
    pub witness_generator_config: Option<FriWitnessGeneratorConfig>,

    #[config(nest, rename = "prometheus")] // FIXME: also nested within API?
    pub prometheus_config: Option<PrometheusConfig>,
    #[config(nest, rename = "data_handler")]
    pub proof_data_handler_config: Option<ProofDataHandlerConfig>,
    #[config(nest, rename = "db", alias = "database")]
    pub db_config: DBConfig,
    #[config(nest)]
    pub eth: Option<EthConfig>,
    #[config(nest)]
    pub snapshot_creator: Option<SnapshotsCreatorConfig>,
    #[config(nest)]
    pub observability: ObservabilityConfig,
    //#[config(nest)]
    //pub da_client_config: Option<DAClientConfig>,
    #[config(nest, rename = "da_dispatcher")]
    pub da_dispatcher_config: Option<DADispatcherConfig>,
    #[config(nest, rename = "protective_reads_writer")]
    pub protective_reads_writer_config: Option<ProtectiveReadsWriterConfig>,
    #[config(nest, rename = "basic_witness_input_producer")]
    pub basic_witness_input_producer_config: Option<BasicWitnessInputProducerConfig>,
    #[config(nest)]
    pub commitment_generator: CommitmentGeneratorConfig,
    #[config(nest)]
    pub snapshot_recovery: SnapshotRecoveryConfig,
    #[config(nest)]
    pub pruning: PruningConfig,
    #[config(nest)]
    pub core_object_store: Option<ObjectStoreConfig>,
    #[config(nest)]
    pub base_token_adjuster: BaseTokenAdjusterConfig,
    #[config(nest, rename = "external_price_api_client")]
    pub external_price_api_client_config: ExternalPriceApiClientConfig,
    #[config(nest, rename = "consensus")]
    pub consensus_config: Option<ConsensusConfig>,
    #[config(nest, rename = "external_proof_integration_api")]
    pub external_proof_integration_api_config: Option<ExternalProofIntegrationApiConfig>,
    #[config(nest, rename = "experimental_vm")]
    pub experimental_vm_config: ExperimentalVmConfig,
    #[config(nest, rename = "prover_job_monitor")]
    pub prover_job_monitor_config: Option<ProverJobMonitorConfig>,
    #[config(nest, rename = "timestamp_asserter")]
    pub timestamp_asserter_config: TimestampAsserterConfig,
}

pub fn full_config_schema(for_en: bool) -> ConfigSchema {
    let mut schema = ConfigSchema::new(&GeneralConfig::DESCRIPTION, "");

    // Add global aliases for the snapshots object store.
    schema
        .get_mut(
            &ObjectStoreConfig::DESCRIPTION,
            "snapshot_creator.object_store",
        )
        .unwrap()
        .push_alias("snapshots.object_store")
        .unwrap();
    schema
        .get_mut(
            &ObjectStoreConfig::DESCRIPTION,
            "snapshot_recovery.object_store",
        )
        .unwrap()
        .push_alias("snapshots.object_store")
        .unwrap();
    // TODO: add aliases for prover object stores in the same way and other aliases from tests

    // Specialized configuration that were placed in separate files.
    schema.insert(&Secrets::DESCRIPTION, "").unwrap();

    if for_en {
        schema
            .insert(&ENConfig::DESCRIPTION, "external_node")
            .unwrap();
    } else {
        // Contracts, wallets and genesis configs are only read by the main node.
        schema
            .insert(&GenesisConfigWrapper::DESCRIPTION, "")
            .unwrap();
        schema.insert(&Wallets::DESCRIPTION, "wallets").unwrap();

        schema
            .insert(&ContractsConfig::DESCRIPTION, "contracts")
            .unwrap();
        schema
            .single_mut(&L1ContractsConfig::DESCRIPTION)
            .unwrap()
            .push_alias("contracts")
            .unwrap();
        schema
            .single_mut(&EcosystemContracts::DESCRIPTION)
            .unwrap()
            .push_alias("contracts")
            .unwrap();
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_schema_can_be_constructed_for_main_node() {
        full_config_schema(false);
    }

    #[test]
    fn config_schema_can_be_constructed_for_en() {
        full_config_schema(true);
    }
}
