mod doctor;
mod enable;
mod install;
mod list;
mod registry;
mod setup;

use clap::{Args, Subcommand};

pub use registry::ConnectorId;

#[derive(Args)]
pub struct ConnectorArgs {
    #[command(subcommand)]
    pub command: ConnectorCommands,
}

#[derive(Subcommand)]
pub enum ConnectorCommands {
    /// List known connectors and a short status summary
    List(list::ListArgs),
    /// Enable a connector in config.toml
    Enable(enable::EnableArgs),
    /// Diagnose binary, auth, and optional models probe
    Doctor(doctor::DoctorArgs),
    /// Run vendor login / setup for a connector
    Setup(setup::SetupArgs),
    /// Install a connector CLI into managed $HOME
    Install(install::InstallArgs),
}

pub async fn run(args: ConnectorArgs) -> anyhow::Result<()> {
    match args.command {
        ConnectorCommands::List(a) => list::run(a),
        ConnectorCommands::Enable(a) => enable::run(a),
        ConnectorCommands::Doctor(a) => doctor::run(a),
        ConnectorCommands::Setup(a) => setup::run(a),
        ConnectorCommands::Install(a) => install::run(a),
    }
}
