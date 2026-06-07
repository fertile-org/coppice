use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config_path = coppice_server::AppConfig::resolve_config_path();
    let config = coppice_server::AppConfig::load(config_path.as_deref())
        .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;

    let db = coppice_server::db::connect_and_migrate(&config.database.url).await?;
    let state = Arc::new(coppice_server::AppState {
        attachments: coppice_server::AppState::attachment_store_from_config(&config),
        config: config.clone(),
        db: Some(db),
    });
    let app = coppice_server::app(state);
    let addr: SocketAddr = format!("0.0.0.0:{}", config.server.port).parse()?;
    tracing::info!(%addr, "listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
