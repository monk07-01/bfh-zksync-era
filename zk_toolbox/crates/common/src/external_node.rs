use anyhow::Context;
use xshell::{cmd, Shell};

use crate::cmd::Cmd;

pub fn run(
    shell: &Shell,
    config_path: &str,
    secrets_path: &str,
    en_config_path: &str,
    additional_args: Vec<String>,
) -> anyhow::Result<()> {
    let cmd = Cmd::new(
        cmd!(
            shell,
            "cargo run --release --bin zksync_external_node --
            --config-path {config_path}
            --secrets-path {secrets_path}
            --external-node-config-path {en_config_path}
            "
        )
        .args(additional_args)
        .env_remove("RUSTUP_TOOLCHAIN"),
    )
    .with_force_run();

    cmd.run().context("Failed to run external node")
}
