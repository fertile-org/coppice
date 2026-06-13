use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use sqlx::PgPool;
use sqlx::Row;
use tokio::sync::watch;

use crate::domain::comment::{author_type_to_str, CommentIntent};
use crate::domain::run::{run_status_to_str, RunStatus};
use crate::domain::slug::slugify;
use crate::domain::ticket::{status_to_str, substatus_to_str};
use crate::events::bus::AppEvent;
use crate::providers::{AgentRunInput, ProviderError};
use crate::services::agent_service::AgentService;
use crate::services::artifact_service::{ArtifactService, RunArtifactMeta, RunArtifactPaths};
use crate::services::comment_service::CommentService;
use crate::services::context_builder::{write_context_file, ContextInput};
use crate::services::job_service::JobService;
use crate::services::result_contract;
use crate::services::run_orchestrator::{load_run_continuation_context, RunOrchestrator};
use crate::services::run_service::RunService;
use crate::services::ticket_service::TicketService;
use crate::services::workflow_service::WorkflowService;
use crate::services::worktree_service::{compute_paths, WorktreeService};
use crate::util::error_format::format_job_error;
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

    match execute_job(state, pool, &run_svc, &run).await {
        Ok(()) => job_svc.mark_done(job.id).await?,
        Err(err) if err.downcast_ref::<JobCancelled>().is_some() => {
            job_svc.mark_cancelled(job.id).await?;
            publish_run_finished(state, run.id, run.ticket_id, run.agent_id, RunStatus::Cancelled, None);
        }
        Err(err) => {
            if run_svc.is_cancelled(run.id).await.unwrap_or(false) {
                job_svc.mark_cancelled(job.id).await?;
                publish_run_finished(state, run.id, run.ticket_id, run.agent_id, RunStatus::Cancelled, None);
            } else {
                let message = format_job_error(&err);
                fail_job(pool, run.id, job.id, &message).await?;
                publish_run_finished(
                    state,
                    run.id,
                    run.ticket_id,
                    run.agent_id,
                    RunStatus::Failed,
                    Some(message),
                );
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

    let ticket = TicketService::new(pool)
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

    run_svc.mark_running(run.id).await.context("mark run running")?;

    if let Some(new_status) = WorkflowService::resolve_run_start_transition(
        ticket.ticket.status,
        &agent.role,
        &run.job_type,
    ) {
        TicketService::new(pool)
            .update_status(run.ticket_id, new_status, None, None)
            .await
            .context("apply run-start transition")?;
    }

    let agent_key = agent
        .preset_source
        .clone()
        .unwrap_or_else(|| slugify(&agent.name));
    let resume_context = load_run_continuation_context(pool, run)
        .await
        .map_err(|e| anyhow::anyhow!("load run continuation context: {e}"))?;
    let resume_context_ref = resume_context.as_deref();

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
        &agent.name,
    );
    let git_dir = PathBuf::from(&local_path);

    worktree_service
        .ensure_worktree(&git_dir, &paths.worktree_dir, &paths.branch_name)
        .await
        .context("ensure worktree")?;

    let ticket_substatus = ticket
        .ticket
        .substatus
        .as_ref()
        .map(|s| substatus_to_str(*s));
    let worktree_path = paths.worktree_dir.to_string_lossy().into_owned();
    let context_input = ContextInput {
        ticket_title: &ticket.ticket.title,
        ticket_description: &ticket.ticket.description,
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
    };
    write_context_file(&paths.worktree_dir, &context_input).context("write context file")?;

    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    let context_path = paths
        .worktree_dir
        .join(".agent")
        .join("context.md")
        .to_string_lossy()
        .into_owned();

    let stream = state.run_streams.register(run.id);
    let cancel_rx = stream.cancelled_rx();

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

    let connector_name = &agent.connector;
    let connector = state
        .connector_registry
        .get(connector_name)
        .ok_or_else(|| anyhow::anyhow!("agent connector not configured: {connector_name}"))?;

    let session_created_tx = if connector_name == "opencode" || connector_name == "claude-code" {
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
            agent_key,
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
    state.event_bus.publish(AppEvent::TicketUpdated {
        ticket_id: run.ticket_id,
        status: status_to_str(updated_ticket.ticket.status).into(),
        substatus: updated_ticket
            .ticket
            .substatus
            .map(|s| substatus_to_str(s).into()),
        updated_at: updated_ticket
            .ticket
            .updated_at
            .format(&Rfc3339)
            .unwrap_or_default(),
    });

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
        run.id,
        run.ticket_id,
        run.agent_id,
        finished_run.status,
        finished_run.error_message,
    );

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
    for msg in &messages {
        if let crate::sessions::LiveMessage::Frame { data, .. } = msg {
            log_bytes.extend_from_slice(data);
            frame_count += 1;
        }
    }
    ArtifactService::write_terminal_log(&paths, &log_bytes)?;
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

async fn run_session_id(pool: &PgPool, run_id: uuid::Uuid) -> Option<String> {
    RunService::new(pool)
        .get(run_id)
        .await
        .ok()
        .and_then(|r| r.session_id)
}

fn publish_run_finished(
    state: &AppState,
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
