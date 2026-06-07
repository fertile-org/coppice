use clap::Args;
use serde::Serialize;

#[derive(Args)]
pub struct BootstrapArgs {
    #[arg(long)]
    pub email: String,
    #[arg(long)]
    pub password: String,
    #[arg(long, env = "COPPICE_SERVER_URL", default_value = "http://localhost:8080")]
    pub server_url: String,
    #[arg(long, env = "COPPICE_BOOTSTRAP_PASSWORD", default_value = "changeme")]
    pub bootstrap_password: String,
}

#[derive(Serialize)]
struct BootstrapBody<'a> {
    email: &'a str,
    password: &'a str,
}

pub async fn run(args: BootstrapArgs) -> anyhow::Result<()> {
    let url = format!(
        "{}/api/auth/bootstrap",
        args.server_url.trim_end_matches('/')
    );
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("content-type", "application/json")
        .header("x-bootstrap-password", &args.bootstrap_password)
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
