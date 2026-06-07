mod commands;

use clap::{Parser, Subcommand};
use commands::{bootstrap, health, migrate};

#[derive(Parser)]
#[command(name = "coppice", about = "Coppice operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Health(health::HealthArgs),
    Migrate(migrate::MigrateArgs),
    Bootstrap {
        #[command(subcommand)]
        command: BootstrapCommands,
    },
}

#[derive(Subcommand)]
enum BootstrapCommands {
    Admin(bootstrap::BootstrapArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Health(args) => health::run(args).await,
        Commands::Migrate(args) => migrate::run(args).await,
        Commands::Bootstrap {
            command: BootstrapCommands::Admin(args),
        } => bootstrap::run(args).await,
    }
}
