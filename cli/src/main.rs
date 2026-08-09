mod commands;

use clap::{Parser, Subcommand};
use commands::{bootstrap, connector, health, migrate, server, web};

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
    Server {
        #[command(subcommand)]
        command: ServerCommands,
    },
    Web {
        #[command(subcommand)]
        command: WebCommands,
    },
    /// Manage agent connectors (enable, install, setup, doctor)
    Connector(connector::ConnectorArgs),
}

#[derive(Subcommand)]
enum BootstrapCommands {
    Admin(bootstrap::BootstrapArgs),
}

#[derive(Subcommand)]
enum ServerCommands {
    Start(server::ServerStartArgs),
}

#[derive(Subcommand)]
enum WebCommands {
    Start(web::WebStartArgs),
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
        Commands::Server {
            command: ServerCommands::Start(args),
        } => server::run(args),
        Commands::Web {
            command: WebCommands::Start(args),
        } => web::run(args).await,
        Commands::Connector(args) => connector::run(args).await,
    }
}
