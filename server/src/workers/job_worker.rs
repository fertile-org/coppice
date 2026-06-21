use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use sqlx::PgPool;
use sqlx::Row;
use tokio::sync::watch;

use crate::domain::comment::{author_type_to_str, intent_to_str, Comment, CommentIntent};
use crate::domain::substatus::TicketStatus;
use crate::domain::run::{run_status_to_str, RunStatus};
use crate::domain::slug::slugify;
use crate::domain::ticket::{status_to_str, substatus_to_str};
use crate::events::bus::AppEvent;
use crate::providers::{AgentRunInput, ProviderError};
use crate::services::agent_service::AgentService;
use crate::services::artifact_service::{ArtifactService, RunArtifactMeta, RunArtifactPaths};
use crate::services::comment_service::CommentService;
use crate::domain::context_profile::ContextProfile;
use crate::services::context_builder::{
    write_agent_context_files, write_context_file, ContextInput, HumanRequest,
};
use crate::services::job_service::JobService;
use crate::services::notification_service::NotificationService;
use crate::services::result_contract::{self, ACCEPTANCE_CRITERIA_HEADER};
use crate::services::run_orchestrator::{load_run_continuation_context, RunOrchestrator};
use crate::services::run_service::{AgentRunWithConnector, RunService};
use crate::services::ticket_service::{TicketService, TicketWithDisplay};
use crate::services::ticket_thread;
use crate::services::workflow_service::WorkflowService;
use crate::services::worktree_service::{
    compute_paths, finalize_worktree_git, format_git_comment_footer, sync_worktree_to_branch_tip,
    WorktreeService,
};
use crate::util::error_format::format_job_error;
use crate::util::truncate::truncate_with_ellipsis;
use crate::AppState;
use time::format_description::well_known::Rfc3339;

#[derive(Debug)]
struct JobCancelled;

impl std::fmt::Display for JobCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "run cancelled")
    }
}

impl std::error::Error for JobCancelled {}

pub fn spawn_workers(state: Arc<AppState>) {
    let count = state.config.agent.worker_count.max(1);
    for i in 0..count {
        let state = state.clone();
        tokio::spawn(async move {
            let worker_id = format!("worker-{i}");
            loop {
                if let Err(err) = process_one(&state, &worker_id).await {
                    tracing::error!(error = %format_job_error(&err), "job worker error");
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }
}

async fn process_one(state: &AppState, worker_id: &str) -> anyhow::Result<()> {
    let pool = state.db.as_ref().context("no db")?;
    let job_svc = JobService::new(pool);
    let job = job_svc.claim_next(worker_id).await?;
    let Some(job) = job else {
        return Ok(());
    };

    let run_svc = RunService::new(pool);
    let run = run_svc.get(job.run_id).await.context("load run")?;

    if run.status != RunStatus::Queued {
        tracing::debug!(
            run_id = %run.id,
            job_id = %job.id,
            status = %run_status_to_str(run.status),
            "discarding stale job for inactive run"
        );
        job_svc
            .mark_failed(job.id, "stale job for inactive run")
            .await?;
        return Ok(());
    }

    match execute_job(state, pool, &run_svc, &run).await {
        Ok(()) => job_svc.mark_done(job.id).await?,
        Err(err) if err.downcast_ref::<JobCancelled>().is_some() => {
            state.run_streams.remove(run.id);
            job_svc.mark_cancelled(job.id).await?;
            publish_run_finished(state, pool, run.id, run.ticket_id, run.agent_id, RunStatus::Cancelled, None).await;
        }
        Err(err) => {
            state.run_streams.remove(run.id);
            if run_svc.is_cancelled(run.id).await.unwrap_or(false) {
                job_svc.mark_cancelled(job.id).await?;
                publish_run_finished(state, pool, run.id, run.ticket_id, run.agent_id, RunStatus::Cancelled, None).await;
            } else {
                let message = format_job_error(&err);
                fail_job(pool, run.id, job.id, &message).await?;
                publish_run_finished(
                    state,
                    pool,
                    run.id,
                    run.ticket_id,
                    run.agent_id,
                    RunStatus::Failed,
                    Some(message),
                )
                .await;
            }
            return Err(err);
        }
    }

    Ok(())
}

async fn execute_job(
    state: &AppState,
    pool: &PgPool,
    run_svc: &RunService<'_>,
    run: &crate::domain::run::AgentRun,
) -> anyhow::Result<()> {
    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    let mut ticket = TicketService::new(pool)
        .get(run.ticket_id)
        .await
        .context("load ticket")?;
    let agent = AgentService::new(pool)
        .get(run.agent_id)
        .await
        .context("load agent")?;

    let repo_id = ticket
        .ticket
        .repo_id
        .context("ticket has no repo")?;

    let repo_row = sqlx::query(
        "SELECT local_path, name, remote_url, default_branch FROM repos WHERE id = $1",
    )
        .bind(repo_id)
        .fetch_optional(pool)
        .await?
        .context("repo not found")?;

    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    // Register the live stream handle BEFORE marking the run running so that any
    // client that observes the run as active (DB status or the agent_run.started
    // event below) can attach deterministically. Registering after mark_running
    // left a window where wait_for_run_stream polled an empty registry; short
    // runs finished and unregistered before a client could attach.
    let stream = state.run_streams.register(run.id);
    let cancel_rx = stream.cancelled_rx();

    run_svc.mark_running(run.id).await.context("mark run running")?;

    tracing::info!(
        run_id = %run.id,
        ticket_id = %run.ticket_id,
        agent_id = %run.agent_id,
        job_type = %run.job_type,
        "agent run started"
    );

    if run.context_profile != ContextProfile::HumanAgent {
        if let Some(new_status) = WorkflowService::resolve_run_start_transition(
            ticket.ticket.status,
            &agent.role,
            &run.job_type,
        ) {
            let updated = TicketService::new(pool)
                .update_status(run.ticket_id, new_status, None, None)
                .await
                .context("apply run-start transition")?;
            crate::events::publish_ticket_updated(&state.event_bus, &updated);
            ticket = updated;
        }
    }

    let snapshot_handle = stream.clone();
    let artifacts_dir = state.config.storage.artifacts_dir.clone();
    let run_id_for_flush = run.id;
    let mut flush_cancel = stream.cancelled_rx();
    tokio::spawn(async move {
        let paths = RunArtifactPaths::new(&artifacts_dir, &run_id_for_flush.to_string());
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Some(snap) = snapshot_handle.snapshot() {
                        let _ = ArtifactService::write_session_snapshot(&paths, &snap);
                    }
                }
                _ = flush_cancel.changed() => break,
            }
        }
    });

    state.event_bus.publish(AppEvent::AgentRunStarted {
        run_id: run.id,
        ticket_id: run.ticket_id,
        agent_id: run.agent_id,
        status: "running".into(),
    });

    let agent_key = agent
        .preset_source
        .clone()
        .unwrap_or_else(|| slugify(&agent.name));

    let agents = AgentService::new(pool)
        .list_agents()
        .await
        .unwrap_or_default();
    let agent_names = agents
        .iter()
        .map(|a| (a.id, a.name.clone()))
        .collect::<HashMap<_, _>>();

    let assignee_agent_key_owned = ticket.ticket.assignee_agent_id.and_then(|id| {
        agents.iter().find(|a| a.id == id).map(|a| {
            a.preset_source
                .clone()
                .unwrap_or_else(|| slugify(&a.name))
        })
    });
    let assignee_agent_key_ref = assignee_agent_key_owned.as_deref();

    let resume_context = if run.context_profile == ContextProfile::Full {
        load_run_continuation_context(pool, run)
            .await
            .map_err(|e| anyhow::anyhow!("load run continuation context: {e}"))?
    } else {
        None
    };
    let resume_context_ref = resume_context.as_deref();

    let thread_excerpt_owned = if run.context_profile == ContextProfile::HumanChat {
        let comments = CommentService::new(pool)
            .list_by_ticket(run.ticket_id)
            .await
            .context("load comments for thread excerpt")?;
        ticket_thread::format_thread_excerpt(&comments, &agent_names, 3, 800)
    } else {
        None
    };
    let thread_excerpt_ref = thread_excerpt_owned.as_deref();

    let trigger_posted_at: Option<String>;
    let trigger_body: Option<String>;
    if let Some(comment_id) = run.trigger_comment_id {
        let trigger = CommentService::new(pool)
            .get(comment_id)
            .await
            .context("load trigger comment")?;
        trigger_posted_at = Some(
            trigger
                .created_at
                .format(&Rfc3339)
                .unwrap_or_default(),
        );
        trigger_body = Some(trigger.body);
    } else {
        trigger_posted_at = None;
        trigger_body = None;
    }

    let human_request = match (
        trigger_body.as_deref(),
        trigger_posted_at.as_deref(),
        human_request_mode_label(run.context_profile),
    ) {
        (Some(body), Some(posted_at), Some(mode_label)) => Some(HumanRequest {
            body,
            posted_at,
            mode_label,
        }),
        _ => None,
    };

    let local_path: String = repo_row.get("local_path");
    let repo_name: String = repo_row.get("name");
    let repo_remote_url: Option<String> = repo_row.get("remote_url");
    let repo_default_branch: String = repo_row.get("default_branch");

    let worktree_service = WorktreeService::new(
        state.config.agent.worktrees_path.clone().into(),
    );
    let paths = compute_paths(
        worktree_service.worktrees_root(),
        &repo_name,
        run.ticket_id,
    );
    let git_dir = PathBuf::from(&local_path);

    worktree_service
        .ensure_worktree(&git_dir, &paths.worktree_dir, &paths.branch_name)
        .await
        .context("ensure worktree")?;

    sync_worktree_to_branch_tip(&git_dir, &paths.worktree_dir, &paths.branch_name)
        .await
        .context("sync worktree to branch tip")?;

    let ticket_substatus = ticket
        .ticket
        .substatus
        .as_ref()
        .map(|s| substatus_to_str(*s));
    let worktree_path = paths.worktree_dir.to_string_lossy().into_owned();
    let ticket_description = match run.context_profile {
        ContextProfile::Full => ticket.ticket.description.as_str(),
        ContextProfile::HumanAgent | ContextProfile::HumanChat => "",
    };
    let context_input = ContextInput {
        ticket_title: &ticket.ticket.title,
        ticket_description,
        ticket_status: status_to_str(ticket.ticket.status),
        ticket_substatus,
        agent_name: &agent.name,
        agent_key: &agent_key,
        agent_role: &agent.role,
        agent_skills: &agent.skills,
        agent_responsibilities: &agent.responsibilities,
        agent_system_prompt: &agent.system_prompt,
        repo_name: Some(&repo_name),
        repo_remote_url: repo_remote_url.as_deref(),
        repo_default_branch: Some(&repo_default_branch),
        worktree_path: Some(&worktree_path),
        resume_context: resume_context_ref,
        context_profile: run.context_profile,
        human_request,
        ticket_id: Some(run.ticket_id),
        assignee_agent_key: assignee_agent_key_ref,
        thread_excerpt: thread_excerpt_ref,
    };
    write_context_file(&paths.worktree_dir, &context_input).context("write context file")?;

    if run.context_profile != ContextProfile::Full {
        let comments = CommentService::new(pool)
            .list_by_ticket(run.ticket_id)
            .await
            .context("load comments for context snapshot")?;
        let runs = RunService::new(pool)
            .list_for_ticket(run.ticket_id)
            .await
            .context("load runs for context snapshot")?;
        let ticket_json = build_ticket_json(&ticket, assignee_agent_key_ref);
        let comments_json = build_comments_json(&comments, &agent_names);
        let runs_json = build_runs_json(&runs, &agent_names);
        write_agent_context_files(
            &paths.worktree_dir,
            &ticket_json,
            &comments_json,
            &runs_json,
        )
        .context("write agent context files")?;
    }

    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    let context_path = paths
        .worktree_dir
        .join(".agent")
        .join("context.md")
        .to_string_lossy()
        .into_owned();

    let connector_name = &agent.connector;
    let connector = state
        .connector_registry
        .get(connector_name)
        .ok_or_else(|| anyhow::anyhow!("agent connector not configured: {connector_name}"))?;

    let session_created_tx = if connector_name == "opencode" || connector_name == "claude-code" || connector_name == "codex" || connector_name == "kilo-code" {
        let (tx, mut rx) = watch::channel(String::new());
        let pool = pool.clone();
        let run_id = run.id;
        tokio::spawn(async move {
            if rx.changed().await.is_ok() {
                let sid = rx.borrow().clone();
                if !sid.is_empty() {
                    let _ = RunService::new(&pool).set_session_id(run_id, &sid).await;
                }
            }
        });
        Some(tx)
    } else {
        None
    };

    let provider_result = connector
        .run(AgentRunInput {
            agent_id: run.agent_id.to_string(),
            agent_key: agent_key.clone(),
            job_type: run.job_type.clone(),
            ticket_id: Some(run.ticket_id.to_string()),
            context_path,
            run_id: Some(run.id.to_string()),
            artifacts_dir: Some(state.config.storage.artifacts_dir.clone()),
            model_provider: agent.model_provider.clone(),
            model: agent.model.clone(),
            stream: Some(stream.clone()),
            cancel_rx: Some(cancel_rx),
            session_created_tx,
            resume_context,
            resume_session_id: load_resume_session_id(pool, run, connector_name).await,
        })
        .await;

    let result = match provider_result {
        Ok(result) => result,
        Err(ProviderError::Cancelled) => {
            let session_id = run_session_id(pool, run.id).await;
            persist_artifacts(state, &stream, run.id, connector_name, session_id)?;
            state.run_streams.remove(run.id);
            return Err(JobCancelled.into());
        }
        Err(err) => {
            let session_id = run_session_id(pool, run.id).await;
            persist_artifacts(state, &stream, run.id, connector_name, session_id)?;
            state.run_streams.remove(run.id);
            return Err(anyhow::anyhow!("agent provider: {err}"));
        }
    };

    if run_svc.is_cancelled(run.id).await? {
        let session_id = run_session_id(pool, run.id).await;
        persist_artifacts(state, &stream, run.id, connector_name, session_id)?;
        state.run_streams.remove(run.id);
        return Err(JobCancelled.into());
    }

    let mut apply = result_contract::apply_agent_result(&result)
        .map_err(|err| anyhow::anyhow!("apply agent result: {err}"))?;
    if run.job_type == "respond_to_mention" && apply.run_status == RunStatus::Succeeded {
        apply.comment.intent = CommentIntent::ClarificationAnswer;
    } else if run.job_type == "work_on_ticket"
        && apply.run_status == RunStatus::Succeeded
        && ticket.ticket.status == TicketStatus::InReview
        && is_review_agent(&agent)
    {
        apply.comment.intent = CommentIntent::ReviewFeedback;
    }

    if run.job_type == "work_on_ticket"
        && apply.run_status == RunStatus::Succeeded
        && should_finalize_worktree_git(&agent, ticket.ticket.status)
    {
        let commit_message = format!(
            "[coppice] {}: {}",
            &agent_key,
            truncate_with_ellipsis(&ticket.ticket.title, 72)
        );
        match finalize_worktree_git(
            &paths.worktree_dir,
            &paths.branch_name,
            &commit_message,
        )
        .await
        {
            Ok(git_state) => {
                apply.comment.body.push_str(&format_git_comment_footer(&git_state));
            }
            Err(err) => {
                tracing::warn!(
                    run_id = %run.id,
                    error = %err,
                    "worktree auto-commit failed; comment will omit git footer"
                );
            }
        }
    }

    let worktree_path = paths.worktree_dir.to_string_lossy().into_owned();
    let orchestrator = RunOrchestrator::new(pool, &state.config.workflow);
    let finished_run = orchestrator
        .finish_run(
            run,
            &result,
            apply,
            Some(worktree_path),
            Some(paths.branch_name.clone()),
        )
        .await
        .context("finish run via orchestrator")?;

    let session_id = run_session_id(pool, run.id).await;
    persist_artifacts(state, &stream, run.id, connector_name, session_id)?;
    state.run_streams.remove(run.id);

    let updated_ticket = TicketService::new(pool)
        .get(run.ticket_id)
        .await
        .context("load updated ticket")?;
    crate::events::publish_ticket_updated(&state.event_bus, &updated_ticket);

    let comments = CommentService::new(pool)
        .list_by_ticket(run.ticket_id)
        .await
        .context("list comments after apply")?;
    if let Some(comment) = comments.last() {
        state.event_bus.publish(AppEvent::CommentCreated {
            comment_id: comment.id,
            ticket_id: run.ticket_id,
            author_type: author_type_to_str(comment.author_type).into(),
        });
    }

    publish_run_finished(
        state,
        pool,
        run.id,
        run.ticket_id,
        run.agent_id,
        finished_run.status,
        finished_run.error_message,
    )
    .await;

    Ok(())
}

fn persist_artifacts(
    state: &AppState,
    stream: &crate::sessions::run_registry::RunStreamHandle,
    run_id: uuid::Uuid,
    connector_name: &str,
    session_id: Option<String>,
) -> anyhow::Result<()> {
    let paths = RunArtifactPaths::new(
        &state.config.storage.artifacts_dir,
        &run_id.to_string(),
    );
    if let Some(snap) = stream.snapshot() {
        let _ = ArtifactService::write_session_snapshot(&paths, &snap);
    }
    let messages = stream.buffered_tail();
    let mut log_bytes = Vec::new();
    let mut frame_count = 0u64;
    let mut console_events = Vec::new();
    for msg in &messages {
        match msg {
            crate::sessions::LiveMessage::Frame { data, .. } => {
                log_bytes.extend_from_slice(data);
                frame_count += 1;
            }
            crate::sessions::LiveMessage::Event { event }
                if event
                    .get("type")
                    .and_then(|v| v.as_str())
                    .is_some_and(|ty| {
                        ty.starts_with("claude.console.")
                            || ty.starts_with("codex.console.")
                            || ty.starts_with("kilo.console.")
                    }) =>
            {
                console_events.push(event.clone());
            }
            _ => {}
        }
    }
    if !console_events.is_empty() {
        ArtifactService::write_console_events(&paths, &console_events)?;
    }
    if !log_bytes.is_empty() {
        ArtifactService::write_terminal_log(&paths, &log_bytes)?;
    } else if !console_events.is_empty() {
        ArtifactService::write_terminal_log(&paths, b"")?;
    }
    ArtifactService::write_meta(
        &paths,
        &RunArtifactMeta {
            provider: connector_name.into(),
            session_id,
            frame_count,
            ended_at: time::OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_default(),
        },
    )?;
    Ok(())
}

fn is_review_agent(agent: &crate::domain::agent::Agent) -> bool {
    if matches!(
        agent.preset_source.as_deref(),
        Some("tech_lead") | Some("reviewer")
    ) {
        return true;
    }
    let role = agent.role.to_lowercase();
    role.contains("tech lead")
        || role.contains("technical lead")
        || role.contains("review")
}

fn is_qc_agent(agent: &crate::domain::agent::Agent) -> bool {
    if matches!(agent.preset_source.as_deref(), Some("qc")) {
        return true;
    }
    let role = agent.role.to_ascii_lowercase();
    role == "qc" || role.contains("quality")
}

/// Whether a successful `work_on_ticket` run should auto-commit worktree changes.
///
/// QC is verification-only in `in_qa`: it must not present its (potential) edits as
/// committed implementation work. Skip git finalization for QC verification runs so
/// any stray changes never become the ticket's implementation commit.
fn should_finalize_worktree_git(
    agent: &crate::domain::agent::Agent,
    ticket_status: TicketStatus,
) -> bool {
    if is_qc_agent(agent) && ticket_status == TicketStatus::InQa {
        return false;
    }
    true
}

async fn run_session_id(pool: &PgPool, run_id: uuid::Uuid) -> Option<String> {
    RunService::new(pool)
        .get(run_id)
        .await
        .ok()
        .and_then(|r| r.session_id)
}

/// For claude-code continuation runs, look up the previous run's session_id
/// so the connector can pass `--resume <session_id>` to maintain conversation context.
///
/// Note: codex session resume is not implemented here. The codex connector includes
/// the `--resume` flag in its command invocation, but session resume is documented
/// as unreliable for codex (see docs/providers/codex.md). Cross-run continuity uses
/// the `Continued` + context.md checkpoint path instead, which is provider-agnostic.
async fn load_resume_session_id(
    pool: &PgPool,
    run: &crate::domain::run::AgentRun,
    connector_name: &str,
) -> Option<String> {
    if connector_name != "claude-code" {
        return None;
    }
    if run.job_type != "work_on_ticket" {
        return None;
    }
    let session_id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT session_id FROM agent_runs
        WHERE ticket_id = $1 AND agent_id = $2 AND id != $3
          AND session_id IS NOT NULL AND session_id != ''
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(run.ticket_id)
    .bind(run.agent_id)
    .bind(run.id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    session_id
}

async fn publish_run_finished(
    state: &AppState,
    pool: &PgPool,
    run_id: uuid::Uuid,
    ticket_id: uuid::Uuid,
    agent_id: uuid::Uuid,
    status: RunStatus,
    error_message: Option<String>,
) {
    state.event_bus.publish(AppEvent::AgentRunFinished {
        run_id,
        ticket_id,
        agent_id,
        status: run_status_to_str(status).into(),
        error_message,
    });

    // Persist durable in-app notifications for the four terminal statuses.
    // Failures are non-fatal: a missing notification row is preferable to a
    // dropped run-completion signal.
    let status_str = run_status_to_str(status);
    if let Err(err) = NotificationService::new(pool)
        .create_for_run_finished(run_id, ticket_id, agent_id, status_str)
        .await
    {
        tracing::warn!(error = %err, %run_id, "failed to create run-finished notification");
    } else {
        state.event_bus.publish(AppEvent::NotificationChanged {
            recipient_user_id: None,
        });
    }
}

async fn fail_job(
    pool: &PgPool,
    run_id: uuid::Uuid,
    job_id: uuid::Uuid,
    message: &str,
) -> anyhow::Result<()> {
    RunService::new(pool)
        .finish_failed(run_id, message)
        .await
        .context("finish run failed")?;
    JobService::new(pool)
        .mark_failed(job_id, message)
        .await
        .context("mark job failed")?;
    Ok(())
}

fn human_request_mode_label(profile: ContextProfile) -> Option<&'static str> {
    match profile {
        ContextProfile::HumanAgent => Some("Agent"),
        ContextProfile::HumanChat => Some("Chat"),
        ContextProfile::Full => None,
    }
}

fn split_ticket_description(description: &str) -> (String, Option<String>) {
    if let Some(idx) = description.find(ACCEPTANCE_CRITERIA_HEADER) {
        let body = description[..idx].trim_end().to_string();
        let acceptance = description[idx..].trim().to_string();
        (body, Some(acceptance))
    } else {
        (description.to_string(), None)
    }
}

fn build_ticket_json(
    ticket: &TicketWithDisplay,
    assignee_agent_key: Option<&str>,
) -> serde_json::Value {
    let (description, acceptance_criteria) =
        split_ticket_description(&ticket.ticket.description);
    serde_json::json!({
        "id": ticket.ticket.id,
        "title": ticket.ticket.title,
        "status": status_to_str(ticket.ticket.status),
        "substatus": ticket.ticket.substatus.as_ref().map(|s| substatus_to_str(*s)),
        "description": description,
        "acceptance_criteria": acceptance_criteria,
        "assignee_agent_id": ticket.ticket.assignee_agent_id,
        "assignee_agent_key": assignee_agent_key,
    })
}

fn build_comments_json(
    comments: &[Comment],
    agent_names: &HashMap<uuid::Uuid, String>,
) -> serde_json::Value {
    comments
        .iter()
        .map(|comment| {
            serde_json::json!({
                "id": comment.id,
                "author": ticket_thread::author_label(comment, agent_names),
                "intent": intent_to_str(comment.intent),
                "body": comment.body,
                "created_at": comment.created_at.format(&Rfc3339).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>()
        .into()
}

fn build_runs_json(
    runs: &[AgentRunWithConnector],
    agent_names: &HashMap<uuid::Uuid, String>,
) -> serde_json::Value {
    runs.iter()
        .take(10)
        .map(|entry| {
            let agent = agent_names
                .get(&entry.run.agent_id)
                .map(String::as_str)
                .unwrap_or("Agent");
            serde_json::json!({
                "id": entry.run.id,
                "agent": agent,
                "job_type": entry.run.job_type,
                "status": run_status_to_str(entry.run.status),
                "started_at": entry.run.started_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
                "ended_at": entry.run.ended_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
            })
        })
        .collect::<Vec<_>>()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::Agent;
    use time::OffsetDateTime;

    fn test_agent(role: &str, preset_source: Option<&str>) -> Agent {
        Agent {
            id: uuid::Uuid::new_v4(),
            name: format!("{role} Agent"),
            role: role.into(),
            skills: vec![],
            responsibilities: vec![],
            system_prompt: String::new(),
            connector: "mock".into(),
            model_provider: None,
            model: None,
            enabled: true,
            preset_source: preset_source.map(str::to_string),
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn is_qc_agent_matches_preset_and_role() {
        assert!(is_qc_agent(&test_agent("QC", Some("qc"))));
        assert!(is_qc_agent(&test_agent("QC", None)));
        assert!(is_qc_agent(&test_agent("Quality Engineer", None)));
        assert!(!is_qc_agent(&test_agent("Backend Engineer", Some("backend_engineer"))));
        assert!(!is_qc_agent(&test_agent("Reviewer", Some("reviewer"))));
    }

    #[test]
    fn should_not_finalize_git_for_qc_verification_in_qa() {
        let qc = test_agent("QC", Some("qc"));
        assert!(!should_finalize_worktree_git(&qc, TicketStatus::InQa));
    }

    #[test]
    fn should_finalize_git_for_engineer_work() {
        let engineer = test_agent("Backend Engineer", Some("backend_engineer"));
        assert!(should_finalize_worktree_git(&engineer, TicketStatus::InProgress));
        assert!(should_finalize_worktree_git(&engineer, TicketStatus::InQa));
    }

    #[test]
    fn should_finalize_git_for_reviewer_in_review() {
        let reviewer = test_agent("Reviewer", Some("reviewer"));
        assert!(should_finalize_worktree_git(&reviewer, TicketStatus::InReview));
    }

    #[test]
    fn should_finalize_git_for_qc_outside_in_qa() {
        // The guard is scoped to in_qa verification; a QC run elsewhere still finalizes
        // (it should not normally occur, but the guard must not over-reach).
        let qc = test_agent("QC", Some("qc"));
        assert!(should_finalize_worktree_git(&qc, TicketStatus::InProgress));
    }
}
