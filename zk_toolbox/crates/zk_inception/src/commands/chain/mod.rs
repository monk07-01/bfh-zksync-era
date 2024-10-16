use ::common::forge::ForgeScriptArgs;
use args::build_transactions::BuildTransactionsArgs;
pub(crate) use args::create::ChainCreateArgsFinal;
use clap::Subcommand;
pub(crate) use create::create_chain_inner;
use migrate_from_gateway::MigrateToGatewayArgs as MigrateFromGatewayArgs;
use migrate_to_gateway::MigrateToGatewayArgs;
use xshell::Shell;

use crate::commands::chain::{
    args::{create::ChainCreateArgs, genesis::GenesisArgs, init::InitArgs},
    deploy_l2_contracts::Deploy2ContractsOption,
};

pub(crate) mod args;
mod build_transactions;
mod common;
mod convert_to_gateway;
mod create;
pub mod deploy_l2_contracts;
pub mod deploy_paymaster;
mod deploy_and_bridge_zk;
pub mod genesis;
pub(crate) mod init;
mod migrate_from_gateway;
mod migrate_to_gateway;
mod set_token_multiplier_setter;
mod setup_legacy_bridge;

#[derive(Subcommand, Debug)]
pub enum ChainCommands {
    /// Create a new chain, setting the necessary configurations for later initialization
    Create(ChainCreateArgs),
    /// Create unsigned transactions for chain deployment
    BuildTransactions(BuildTransactionsArgs),
    /// Initialize chain, deploying necessary contracts and performing on-chain operations
    Init(InitArgs),
    /// Run server genesis
    Genesis(GenesisArgs),
    /// Initialize bridges on l2
    #[command(alias = "bridge")]
    InitializeBridges(ForgeScriptArgs),
    /// Deploy all l2 contracts
    #[command(alias = "l2")]
    DeployL2Contracts(ForgeScriptArgs),
    /// Deploy L2 consensus registry
    #[command(alias = "consensus")]
    DeployConsensusRegistry(ForgeScriptArgs),
    /// Deploy Default Upgrader
    Upgrader(ForgeScriptArgs),
    /// Deploy paymaster smart contract
    #[command(alias = "paymaster")]
    DeployPaymaster(ForgeScriptArgs),
    /// Update Token Multiplier Setter address on L1
    UpdateTokenMultiplierSetter(ForgeScriptArgs),
    /// Prepare chain to be an eligible gateway
    ConvertToGateway(ForgeScriptArgs),
    /// Migrate chain to gateway
    MigrateToGateway(MigrateToGatewayArgs),
    /// Migrate chain from gateway
    MigrateFromGateway(MigrateFromGatewayArgs),
    /// Deploy ZK token on Era and bridge it to L1q
    DeployAndBridgeZK(ForgeScriptArgs),
}

pub(crate) async fn run(shell: &Shell, args: ChainCommands) -> anyhow::Result<()> {
    match args {
        ChainCommands::Create(args) => create::run(args, shell),
        ChainCommands::Init(args) => init::run(args, shell).await,
        ChainCommands::BuildTransactions(args) => build_transactions::run(args, shell).await,
        ChainCommands::Genesis(args) => genesis::run(args, shell).await,
        ChainCommands::DeployL2Contracts(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::All).await
        }
        ChainCommands::DeployConsensusRegistry(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::ConsensusRegistry).await
        }
        ChainCommands::Upgrader(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::Upgrader).await
        }
        ChainCommands::InitializeBridges(args) => {
            deploy_l2_contracts::run(args, shell, Deploy2ContractsOption::InitiailizeBridges).await
        }
        ChainCommands::DeployPaymaster(args) => deploy_paymaster::run(args, shell).await,
        ChainCommands::UpdateTokenMultiplierSetter(args) => {
            set_token_multiplier_setter::run(args, shell).await
        }
        ChainCommands::ConvertToGateway(args) => convert_to_gateway::run(args, shell).await,
        ChainCommands::MigrateToGateway(args) => migrate_to_gateway::run(args, shell).await,
        ChainCommands::MigrateFromGateway(args) => migrate_from_gateway::run(args, shell).await,
        ChainCommands::DeployAndBridgeZK(args) => deploy_and_bridge_zk::run(args, shell).await,
    }
}
