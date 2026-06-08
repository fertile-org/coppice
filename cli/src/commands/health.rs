use clap::Args;
use coppice_config::AppConfig;

#[derive(Args)]
pub struct HealthArgs {
    #[arg(long, help = "Override API base URL derived from config server.port")]
    pub server_url: Option<String>,
    #[arg(long, help = "Also verify database.url from config")]
    pub check_database: bool,
    #[arg(long, help = "Override database.url from config")]
    pub database_url: Option<String>,
}

pub async fn run(args: HealthArgs) -> anyhow::Result<()> {
    let config = AppConfig::load().map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;
    let server_url = args.server_url.unwrap_or_else(|| config.server_url());

    let url = format!("{}/health", server_url.trim_end_matches('/'));
    let response = reqwest::get(&url).await?;
    if !response.status().is_success() {
        anyhow::bail!("server health check failed: {}", response.status());
    }

    if args.check_database {
        let database_url = args
            .database_url
            .unwrap_or_else(|| config.database.url.clone());
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await?;
    }

    println!("OK");
    Ok(())
}
