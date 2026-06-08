use clap::Args;
use coppice_config::AppConfig;
use sqlx::migrate::Migrator;
use std::path::PathBuf;

#[derive(Args)]
pub struct MigrateArgs {
    #[arg(long, help = "Override database.url from config")]
    pub database_url: Option<String>,
}

pub async fn run(args: MigrateArgs) -> anyhow::Result<()> {
    let config = AppConfig::load().map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;
    let database_url = args
        .database_url
        .unwrap_or_else(|| config.database.url.clone());

    let migrations = migrations_path()?;
    let migrator = Migrator::new(migrations).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    migrator.run(&pool).await?;
    println!("migrations applied");
    Ok(())
}

fn migrations_path() -> anyhow::Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../server/migrations");
    if path.exists() {
        return Ok(path);
    }
    anyhow::bail!("migrations path not found at {}", path.display())
}
