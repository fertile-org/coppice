use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use coppice_server::events::mark_run_interrupted;
use coppice_server::services::run_orchestrator::RunOrchestrator;
use coppice_server::services::run_service::RunService;
use coppice_server::AppState;

async fn interrupt_orphaned_run(state: &AppState, run_id: uuid::Uuid) {
    match mark_run_interrupted(state, run_id, "server restarted during run").await {
        Ok(interrupted) => {
            if let Some(pool) = state.db.as_ref() {
                RunOrchestrator::new(pool, &state.config.workflow)
                    .handle_terminal_run(&interrupted)
                    .await;
            }
        }
        Err(err) => {
            tracing::warn!(error = %err, %run_id, "failed to mark orphaned run interrupted");
        }
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

    // No per-run `opencode serve` survives a restart, so OpenCode runs are
    // orphaned just like every other connector's.
    for run in runs {
        if state.run_streams.get(run.id).is_some() {
            continue;
        }
        interrupt_orphaned_run(state, run.id).await;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = coppice_server::AppConfig::load()
        .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;

    let opencode_runs = coppice_server::sessions::opencode_run_server::OpenCodeRunServers::new(
        config.agent.connectors.opencode.command.clone(),
        config.agent.connectors.opencode.serve_hostname.clone(),
    );

    let db = coppice_server::db::connect_and_migrate(&config.database.url).await?;
    coppice_server::services::auth_service::AuthService::new(&db, &config.auth)
        .maybe_auto_bootstrap(&config.auth)
        .await
        .map_err(|e| anyhow::anyhow!("auto-bootstrap failed: {e}"))?;
    let agent_templates = coppice_server::AppState::load_agent_templates();
    coppice_server::agent_templates::ensure_all_presets_have_templates(&db, &agent_templates)
        .await
        .map_err(|e| anyhow::anyhow!("agent template validation failed: {e}"))?;
    let skills = coppice_server::AppState::builtin_skills_from_config(&config)?;
    let state = Arc::new(coppice_server::AppState {
        attachments: coppice_server::AppState::attachment_store_from_config(&config),
        connector_registry: coppice_server::AppState::connector_registry_from_config(
            &config,
            opencode_runs.clone(),
        ),
        agent_health: Arc::new(coppice_server::services::agent_health::AgentHealthRegistry::new()),
        run_streams: Arc::new(coppice_server::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(coppice_server::events::bus::EventBus::new()),
        opencode_runs: opencode_runs.clone(),
        agent_templates,
        secret_store: coppice_server::crypto::SecretStore::from_master_key(&config.secrets.master_key),
        skills,
        config: config.clone(),
        db: Some(db),
    });
    sweep_orphaned_runs(&state).await;
    coppice_server::workers::job_worker::spawn_workers(state.clone());
    coppice_server::workers::health_worker::spawn_health_worker(state.clone());
    coppice_server::workers::knowledge_compaction_scheduler::spawn_knowledge_compaction_scheduler(
        state.clone(),
    );
    coppice_server::workers::run_watchdog::spawn_run_watchdog(state.clone());
    let app = coppice_server::app(state);
    let addr: SocketAddr = format!("0.0.0.0:{}", config.server.port).parse()?;
    tracing::info!(%addr, "listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
            opencode_runs.shutdown_all().await;
        })
        .await?;
    Ok(())
}
