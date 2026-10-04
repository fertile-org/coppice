use clap::Args;
use coppice_config::AppConfig;

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

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    coppice_migrations::run_migrations(&pool).await?;
    println!("migrations applied");
    Ok(())
}
