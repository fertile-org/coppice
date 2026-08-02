use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use coppice_server::events::mark_run_interrupted;
use coppice_server::services::run_service::RunService;
use coppice_server::sessions::opencode_client::OpenCodeClient;
use coppice_server::AppState;

async fn interrupt_orphaned_run(state: &AppState, run_id: uuid::Uuid) {
    if let Err(err) = mark_run_interrupted(state, run_id, "server restarted during run").await {
        tracing::warn!(error = %err, %run_id, "failed to mark orphaned run interrupted");
    }
}

async fn sweep_orphaned_runs(state: &AppState) {
    let Some(pool) = state.db.as_ref() else {
        return;
    };
    let run_svc = RunService::new(pool);
    let Ok(runs) = run_svc.list_active_runs().await else {
        return;
    };

    for run in runs {
        if state.run_streams.get(run.id).is_some() {
            continue;
        }
        if let (Some(session_id), Some(worktree)) = (&run.session_id, &run.worktree_path) {
            let connector = run_svc
                .agent_connector_for_run(run.agent_id)
                .await
                .ok()
                .flatten();
            if connector.as_deref() == Some("opencode") {
                if let Some(serve) = state.opencode_serve.as_ref() {
                    let client = OpenCodeClient::new(serve.base_url());
                    let alive = client
                        .session_status(std::path::Path::new(worktree), session_id)
                        .await
                        .ok()
                        .flatten()
                        .is_some();
                    if !alive {
                        interrupt_orphaned_run(state, run.id).await;
                    }
                } else {
                    interrupt_orphaned_run(state, run.id).await;
                }
                continue;
            }
        }
        interrupt_orphaned_run(state, run.id).await;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = coppice_server::AppConfig::load()
        .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;

    let opencode_serve =
        if config.agent.connectors.opencode.enabled || config.agent.default_connector == "opencode" {
            Some(
                coppice_server::sessions::opencode_serve::OpenCodeServeManager::start(
                    &config.agent.connectors.opencode,
                )
                .await?,
            )
        } else {
            None
        };

    let db = coppice_server::db::connect_and_migrate(&config.database.url).await?;
    let agent_templates = coppice_server::AppState::load_agent_templates();
    coppice_server::agent_templates::ensure_all_presets_have_templates(&db, &agent_templates)
        .await
        .map_err(|e| anyhow::anyhow!("agent template validation failed: {e}"))?;
    let state = Arc::new(coppice_server::AppState {
        attachments: coppice_server::AppState::attachment_store_from_config(&config),
        connector_registry: coppice_server::AppState::connector_registry_from_config(
            &config,
            opencode_serve.clone(),
        ),
        agent_health: Arc::new(coppice_server::services::agent_health::AgentHealthRegistry::new()),
        run_streams: Arc::new(coppice_server::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(coppice_server::events::bus::EventBus::new()),
        opencode_serve: opencode_serve.clone(),
        agent_templates,
        config: config.clone(),
        db: Some(db),
    });
    sweep_orphaned_runs(&state).await;
    coppice_server::workers::job_worker::spawn_workers(state.clone());
    coppice_server::workers::health_worker::spawn_health_worker(state.clone());
    coppice_server::workers::run_watchdog::spawn_run_watchdog(state.clone());
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
