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

/// rmcp logs plugin-controlled content (notifications, peer info) below warn, so it
/// stays at warn unless the operator's `RUST_LOG` names it.
fn log_directives(rust_log: Option<&str>) -> String {
    let base = rust_log.filter(|s| !s.trim().is_empty()).unwrap_or("info");
    if base.contains("rmcp") {
        base.to_string()
    } else {
        format!("{base},rmcp=warn")
    }
}

/// Desktop launchers often omit common bin dirs; probes and runs share this PATH.
fn augment_process_path() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let current = std::env::var_os("PATH").unwrap_or_default();
    let path = coppice_connectors::probe::augment_path(std::path::Path::new(&home), &current);
    std::env::set_var("PATH", path);
}

/// PATH is set before the tokio runtime exists, so no other thread can read
/// the environment concurrently.
fn main() -> anyhow::Result<()> {
    augment_process_path();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> anyhow::Result<()> {
    let rust_log = std::env::var(EnvFilter::DEFAULT_ENV).ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(log_directives(rust_log.as_deref()))
                .unwrap_or_else(|_| EnvFilter::new(log_directives(None))),
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
    coppice_server::AppState::init_plugins(&db, &config, &skills).await?;
    coppice_server::services::plugin_service::PluginService::new(&db)
        .fail_stale_installs()
        .await
        .map_err(|e| anyhow::anyhow!("failed to mark stale plugin installs: {e}"))?;
    let secret_store =
        coppice_server::crypto::SecretStore::from_master_key(&config.secrets.master_key);
    let plugin_mcp = coppice_server::AppState::plugin_mcp_from_config(&config);
    let tools = coppice_server::AppState::build_tool_registry(
        Some(&db),
        &secret_store,
        plugin_mcp.clone(),
        coppice_server::AppState::list_timeout_from_config(&config),
    );
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
        secret_store,
        skills,
        tools,
        plugin_mcp: plugin_mcp.clone(),
        connector_probes: Arc::new(
            coppice_server::services::connector_probe_service::ConnectorProbes::new(),
        ),
        config: config.clone(),
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
            plugin_mcp.shutdown_all().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::log_directives;

    #[test]
    fn rmcp_defaults_to_warn_unless_named() {
        assert_eq!(log_directives(None), "info,rmcp=warn");
        assert_eq!(log_directives(Some(" ")), "info,rmcp=warn");
        assert_eq!(log_directives(Some("debug")), "debug,rmcp=warn");
        assert_eq!(log_directives(Some("info,rmcp=debug")), "info,rmcp=debug");
    }
}
