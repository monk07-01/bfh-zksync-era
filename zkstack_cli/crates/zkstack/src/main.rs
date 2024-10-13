use clap::{command, Parser, Subcommand};
use commands::{
    args::{ContainersArgs, UpdateArgs},
    dev::DevCommands,
};
use common::{
    check_general_prerequisites,
    config::{global_config, init_global_config, GlobalConfig},
    error::log_error,
    init_prompt_theme, logger,
    version::version_message,
};
use config::EcosystemConfig;
use xshell::Shell;

use crate::commands::{
    chain::ChainCommands, ecosystem::EcosystemCommands, explorer::ExplorerCommands,
    external_node::ExternalNodeCommands, prover::ProverCommands,
};

pub mod accept_ownership;
mod commands;
mod consts;
mod defaults;
pub mod external_node;
mod messages;
mod utils;

#[derive(Parser, Debug)]
#[command(
    version = version_message(env!("CARGO_PKG_VERSION")),
    about
)]
struct Inception {
    #[command(subcommand)]
    command: InceptionSubcommands,
    #[clap(flatten)]
    global: InceptionGlobalArgs,
}

#[derive(Subcommand, Debug)]
pub enum InceptionSubcommands {
    /// Ecosystem related commands
    #[command(subcommand, alias = "e")]
    Ecosystem(Box<EcosystemCommands>),
    /// Chain related commands
    #[command(subcommand, alias = "c")]
    Chain(Box<ChainCommands>),
    /// Chain related commands
    #[command(subcommand)]
    Dev(DevCommands),
    /// Prover related commands
    #[command(subcommand, alias = "p")]
    Prover(ProverCommands),
    ///  External Node related commands
    #[command(subcommand, alias = "en")]
    ExternalNode(ExternalNodeCommands),
    /// Run containers for local development
    #[command(alias = "up")]
    Containers(ContainersArgs),
    /// Run dapp-portal
    Portal,
    /// Run block-explorer
    #[command(subcommand)]
    Explorer(ExplorerCommands),
    /// Update ZKsync
    #[command(alias = "u")]
    Update(UpdateArgs),
    #[command(hide = true)]
    Markdown,
}

#[derive(Parser, Debug)]
#[clap(next_help_heading = "Global options")]
struct InceptionGlobalArgs {
    /// Verbose mode
    #[clap(short, long, global = true)]
    verbose: bool,
    /// Chain to use
    #[clap(long, global = true)]
    chain: Option<String>,
    /// Ignores prerequisites checks
    #[clap(long, global = true)]
    ignore_prerequisites: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    human_panic::setup_panic!();

    // We must parse arguments before printing the intro, because some autogenerated
    // Clap commands (like `--version` would look odd otherwise).
    let inception_args = Inception::parse();

    init_prompt_theme();

    logger::new_empty_line();
    logger::intro();

    let shell = Shell::new().unwrap();

    init_global_config_inner(&shell, &inception_args.global)?;

    if !global_config().ignore_prerequisites {
        check_general_prerequisites(&shell);
    }

    match run_subcommand(inception_args, &shell).await {
        Ok(_) => {}
        Err(error) => {
            log_error(error);
            std::process::exit(1);
        }
    }
    Ok(())
}

async fn run_subcommand(inception_args: Inception, shell: &Shell) -> anyhow::Result<()> {
    match inception_args.command {
        InceptionSubcommands::Ecosystem(args) => commands::ecosystem::run(shell, *args).await?,
        InceptionSubcommands::Chain(args) => commands::chain::run(shell, *args).await?,
        InceptionSubcommands::Dev(args) => commands::dev::run(shell, args).await?,
        InceptionSubcommands::Prover(args) => commands::prover::run(shell, args).await?,
        InceptionSubcommands::Containers(args) => commands::containers::run(shell, args)?,
        InceptionSubcommands::ExternalNode(args) => {
            commands::external_node::run(shell, args).await?
        }
        InceptionSubcommands::Explorer(args) => commands::explorer::run(shell, args).await?,
        InceptionSubcommands::Portal => commands::portal::run(shell).await?,
        InceptionSubcommands::Update(args) => commands::update::run(shell, args).await?,
        InceptionSubcommands::Markdown => {
            clap_markdown::print_help_markdown::<Inception>();
        }
    }
    Ok(())
}

fn init_global_config_inner(
    shell: &Shell,
    inception_args: &InceptionGlobalArgs,
) -> anyhow::Result<()> {
    if let Some(name) = &inception_args.chain {
        if let Ok(config) = EcosystemConfig::from_file(shell) {
            let chains = config.list_of_chains();
            if !chains.contains(name) {
                anyhow::bail!(
                    "Chain with name {} doesnt exist, please choose one of {:?}",
                    name,
                    &chains
                );
            }
        }
    }
    init_global_config(GlobalConfig {
        verbose: inception_args.verbose,
        chain_name: inception_args.chain.clone(),
        ignore_prerequisites: inception_args.ignore_prerequisites,
    });
    Ok(())
}
