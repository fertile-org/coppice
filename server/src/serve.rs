use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use crate::events::mark_run_interrupted;
use crate::services::run_orchestrator::RunOrchestrator;
use crate::services::run_service::RunService;
use crate::{AppConfig, AppState};

#[derive(Default)]
pub struct ServeOptions {
    /// Built SPA directory served for non-API paths.
    pub static_web_dir: Option<PathBuf>,
    /// Called with the bound address once workers are running, before serving.
    pub on_ready: Option<Box<dyn FnOnce(SocketAddr) + Send>>,
}

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

/// Connects and migrates the database, bootstraps, starts workers and serves
/// on `listener` until `shutdown` resolves.
pub async fn serve(
    config: AppConfig,
    listener: tokio::net::TcpListener,
    options: ServeOptions,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let opencode_runs = crate::sessions::opencode_run_server::OpenCodeRunServers::new(
        config.agent.connectors.opencode.command.clone(),
        config.agent.connectors.opencode.serve_hostname.clone(),
    );

    let db = crate::db::connect_and_migrate(&config.database.url).await?;
    crate::services::auth_service::AuthService::new(&db, &config.auth)
        .maybe_auto_bootstrap(&config.auth)
        .await
        .map_err(|e| anyhow::anyhow!("auto-bootstrap failed: {e}"))?;
    let agent_templates = AppState::load_agent_templates();
    crate::agent_templates::ensure_all_presets_have_templates(&db, &agent_templates)
        .await
        .map_err(|e| anyhow::anyhow!("agent template validation failed: {e}"))?;
    let skills = AppState::builtin_skills_from_config(&config)?;
    AppState::init_plugins(&db, &config, &skills).await?;
    crate::services::plugin_service::PluginService::new(&db)
        .fail_stale_installs()
        .await
        .map_err(|e| anyhow::anyhow!("failed to mark stale plugin installs: {e}"))?;
    let secret_store = crate::crypto::SecretStore::from_master_key(&config.secrets.master_key);
    let plugin_mcp = AppState::plugin_mcp_from_config(&config);
    let tools = AppState::build_tool_registry(
        Some(&db),
        &secret_store,
        plugin_mcp.clone(),
        AppState::list_timeout_from_config(&config),
    );
    let state = Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        connector_registry: AppState::connector_registry_from_config(
            &config,
            opencode_runs.clone(),
        ),
        agent_health: Arc::new(crate::services::agent_health::AgentHealthRegistry::new()),
        run_streams: Arc::new(crate::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(crate::events::bus::EventBus::new()),
        opencode_runs: opencode_runs.clone(),
        agent_templates,
        secret_store,
        skills,
        tools,
        plugin_mcp: plugin_mcp.clone(),
        connector_probes: Arc::new(
            crate::services::connector_probe_service::ConnectorProbes::new(),
        ),
        config,
        db: Some(db),
    });
    {
        let state = state.clone();
        tokio::spawn(async move {
            state.connector_probes.refresh_all(&state.config).await;
        });
    }
    plugin_mcp.spawn_reaper();
    sweep_orphaned_runs(&state).await;
    if let Some(pool) = state.db.as_ref() {
        match crate::services::connector_check_service::ConnectorCheckService::new(pool)
            .fail_stale()
            .await
        {
            Ok(0) => {}
            Ok(count) => tracing::info!(count, "marked stale connector checks failed"),
            Err(err) => tracing::warn!(error = %err, "failed to mark stale connector checks"),
        }
    }
    crate::workers::job_worker::spawn_workers(state.clone());
    crate::workers::health_worker::spawn_health_worker(state.clone());
    crate::workers::knowledge_compaction_scheduler::spawn_knowledge_compaction_scheduler(
        state.clone(),
    );
    crate::workers::run_watchdog::spawn_run_watchdog(state.clone());

    let mut app = crate::app(state);
    if let Some(dir) = options.static_web_dir.as_deref() {
        app = crate::static_web::with_static_web(app, dir);
    }
    let addr = listener.local_addr()?;
    tracing::info!(%addr, "listening");
    if let Some(on_ready) = options.on_ready {
        on_ready(addr);
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.await;
            opencode_runs.shutdown_all().await;
            plugin_mcp.shutdown_all().await;
        })
        .await?;
    Ok(())
}
