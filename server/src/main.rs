use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = coppice_server::AppConfig::load()
        .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;

    let opencode_serve =
        if config.agent.providers.opencode.enabled || config.agent.default_provider == "opencode" {
            Some(
                coppice_server::sessions::opencode_serve::OpenCodeServeManager::start(
                    &config.agent.providers.opencode,
                )
                .await?,
            )
        } else {
            None
        };

    let db = coppice_server::db::connect_and_migrate(&config.database.url).await?;
    let state = Arc::new(coppice_server::AppState {
        attachments: coppice_server::AppState::attachment_store_from_config(&config),
        provider_registry: coppice_server::AppState::provider_registry_from_config(
            &config,
            opencode_serve.clone(),
        ),
        agent_health: Arc::new(coppice_server::services::agent_health::AgentHealthRegistry::new()),
        run_streams: Arc::new(coppice_server::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(coppice_server::events::bus::EventBus::new()),
        opencode_serve: opencode_serve.clone(),
        config: config.clone(),
        db: Some(db),
    });
    coppice_server::workers::job_worker::spawn_workers(state.clone());
    let app = coppice_server::app(state);
    let addr: SocketAddr = format!("0.0.0.0:{}", config.server.port).parse()?;
    tracing::info!(%addr, "listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
            if let Some(serve) = opencode_serve {
                serve.shutdown().await;
            }
        })
        .await?;
    Ok(())
}
