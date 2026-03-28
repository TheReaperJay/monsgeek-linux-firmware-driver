use anyhow::Result;
use clap::Parser;
use monsgeek_cli::{Cli, FirmwareCommands, run};

#[tokio::main]
async fn main() -> Result<()> {
    let _firmware_commands_marker: Option<FirmwareCommands> = None;
    run(Cli::parse()).await
}
