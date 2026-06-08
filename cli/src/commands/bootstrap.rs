use clap::Args;
use coppice_config::AppConfig;
use serde::Serialize;

#[derive(Args)]
pub struct BootstrapArgs {
    #[arg(long)]
    pub email: String,
    #[arg(long)]
    pub password: String,
    #[arg(long, help = "Override API base URL derived from config server.port")]
    pub server_url: Option<String>,
    #[arg(long, help = "Override auth.bootstrap_password from config")]
    pub bootstrap_password: Option<String>,
}

#[derive(Serialize)]
struct BootstrapBody<'a> {
    email: &'a str,
    password: &'a str,
}

pub async fn run(args: BootstrapArgs) -> anyhow::Result<()> {
    let config = AppConfig::load().map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;
    let server_url = args.server_url.unwrap_or_else(|| config.server_url());
    let bootstrap_password = args
        .bootstrap_password
        .unwrap_or_else(|| config.auth.bootstrap_password.clone());

    let url = format!("{}/api/auth/bootstrap", server_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("content-type", "application/json")
        .header("x-bootstrap-password", &bootstrap_password)
        .json(&BootstrapBody {
            email: &args.email,
            password: &args.password,
        })
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("bootstrap failed: {}", response.status());
    }

    println!("admin bootstrapped: {}", args.email);
    Ok(())
}
