use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::substatus::{Substatus, TicketStatus};
use crate::events::{mark_run_interrupted, publish_ticket_updated};
use crate::services::comment_service::CommentService;
use crate::services::run_orchestrator::RunOrchestrator;
use crate::services::run_service::RunService;
use crate::services::ticket_service::TicketService;
use crate::{AppConfig, AppState};

/// Ticket comment left when a run is stopped because Coppice quit, including
/// a crash discovered on the next launch.
pub const RUN_STOPPED_BECAUSE_QUIT: &str = "This run was stopped because Coppice quit.";

/// Stored as `interrupted: Coppice quit` on a graceful SIGTERM/SIGINT shutdown.
pub const GRACEFUL_QUIT_REASON: &str = "Coppice quit";

#[derive(Default)]
pub struct ServeOptions {
    /// Built SPA directory served for non-API paths.
    pub static_web_dir: Option<PathBuf>,
    /// Called with the bound address once workers are running, before serving.
    pub on_ready: Option<Box<dyn FnOnce(SocketAddr) + Send>>,
    /// Config file the Connectors toggle and agent save patch in place.
    pub config_path: Option<PathBuf>,
}

async fn interrupt_orphaned_run(state: &AppState, run_id: uuid::Uuid, reason: &str) {
    match mark_run_interrupted(state, run_id, reason).await {
        Ok(interrupted) => {
            block_working_ticket(state, &interrupted).await;
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

/// In Progress, In Review, and In QA mean an agent is working. Reviewer,
/// tech-lead, and QC runs stay in those columns, and a person can drag a
/// ticket there while a run is still live. Once that run is interrupted,
/// Blocked is the existing column for work that cannot continue until a
/// person acts. Wait for Human Review, Done, Ready, Backlog, and an already
/// Blocked ticket stay put; the quit comment is still added.
fn quit_blocks_status(status: TicketStatus) -> bool {
    matches!(
        status,
        TicketStatus::InProgress | TicketStatus::InReview | TicketStatus::InQa
    )
}

async fn block_working_ticket(state: &AppState, run: &crate::domain::run::AgentRun) {
    let Some(pool) = state.db.as_ref() else {
        return;
    };
    let Some(ticket_id) = run.ticket_id else {
        return;
    };
    let ticket_svc = TicketService::new(pool);
    match ticket_svc.get(ticket_id).await {
        Ok(ticket) if quit_blocks_status(ticket.ticket.status) => {
            match ticket_svc
                .update_status(
                    ticket_id,
                    TicketStatus::Blocked,
                    Some(Some(Substatus::BlockedByError)),
                    Some(None),
                )
                .await
            {
                Ok(updated) => publish_ticket_updated(&state.event_bus, &updated),
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        %ticket_id,
                        run_id = %run.id,
                        "failed to move interrupted run's ticket to Blocked"
                    );
                }
            }
        }
        Ok(_) => {}
        Err(err) => {
            tracing::warn!(
                error = %err,
                %ticket_id,
                run_id = %run.id,
                "failed to load ticket for interrupted run"
            );
        }
    }
    if let Err(err) = CommentService::new(pool)
        .create(
            ticket_id,
            AuthorType::System,
            None,
            RUN_STOPPED_BECAUSE_QUIT,
            CommentIntent::SystemEvent,
            &[],
            &[],
        )
        .await
    {
        tracing::warn!(
            error = %err,
            %ticket_id,
            run_id = %run.id,
            "failed to comment that the run was stopped because Coppice quit"
        );
    }
}

/// Marks every run whose process tree this process is about to stop, then
/// stops those trees. Database updates happen first so a worker unblocked by
/// SIGTERM cannot finish the run and leave the ticket in a working column.
pub async fn shutdown_agent_sessions(state: &AppState) {
    let mut seen = std::collections::HashSet::new();
    for raw in crate::process_tree::tracked_run_ids() {
        let Ok(run_id) = uuid::Uuid::parse_str(&raw) else {
            continue;
        };
        if seen.insert(run_id) {
            interrupt_orphaned_run(state, run_id, GRACEFUL_QUIT_REASON).await;
        }
    }
    crate::process_tree::shutdown_running_agents().await;
}

pub async fn sweep_orphaned_runs(state: &AppState) {
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
        interrupt_orphaned_run(state, run.id, "server restarted during run").await;
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
    crate::process_tree::install_from_artifacts(&config.storage.artifacts_dir);
    crate::process_tree::reap_orphaned_agents().await;

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
    let connectors = AppState::connectors_runtime(&config, opencode_runs.clone());
    connectors.set_config_path(options.config_path.clone());
    let state = Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        connectors,
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
            state
                .connector_probes
                .refresh_all(&state.connectors.config())
                .await;
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

    let shutdown_state = state.clone();
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
            // Agent CLIs are in their own sessions, so they outlive this
            // process group. Mark their runs interrupted before signaling,
            // then stop the trees before the runtime drops the workers.
            shutdown_agent_sessions(&shutdown_state).await;
            opencode_runs.shutdown_all().await;
            plugin_mcp.shutdown_all().await;
        })
        .await?;
    Ok(())
}
