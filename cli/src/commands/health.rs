use clap::Args;

#[derive(Args)]
pub struct HealthArgs {
    #[arg(long, env = "COPPICE_SERVER_URL", default_value = "http://localhost:8080")]
    pub server_url: String,
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: Option<String>,
}

pub async fn run(args: HealthArgs) -> anyhow::Result<()> {
    let url = format!("{}/health", args.server_url.trim_end_matches('/'));
    let response = reqwest::get(&url).await?;
    if !response.status().is_success() {
        anyhow::bail!("server health check failed: {}", response.status());
    }

    if let Some(database_url) = args.database_url {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await?;
    }

    println!("OK");
    Ok(())
}
