use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use sqlx::PgPool;
use sqlx::Row;

use crate::domain::comment::author_type_to_str;
use crate::domain::run::{run_status_to_str, RunStatus};
use crate::domain::ticket::{status_to_str, substatus_to_str};
use crate::events::bus::AppEvent;
use crate::providers::{AgentRunInput, ProviderError};
use crate::services::agent_service::AgentService;
use crate::services::artifact_service::{ArtifactService, RunArtifactMeta, RunArtifactPaths};
use crate::services::comment_service::CommentService;
use crate::services::context_builder::{write_context_file, ContextInput};
use crate::services::job_service::JobService;
use crate::services::result_contract;
use crate::services::run_service::RunService;
use crate::services::ticket_service::TicketService;
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
                    tracing::error!(%err, "job worker error");
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
        agent_role: &agent.role,
        agent_skills: &agent.skills,
        agent_responsibilities: &agent.responsibilities,
        agent_system_prompt: &agent.system_prompt,
        repo_name: Some(&repo_name),
        repo_remote_url: repo_remote_url.as_deref(),
        repo_default_branch: Some(&repo_default_branch),
        worktree_path: Some(&worktree_path),
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

    let model = agent.model.clone();

    let provider_result = connector
        .run(AgentRunInput {
            agent_id: run.agent_id.to_string(),
            ticket_id: Some(run.ticket_id.to_string()),
            context_path,
            run_id: Some(run.id.to_string()),
            artifacts_dir: Some(state.config.storage.artifacts_dir.clone()),
            model,
            stream: Some(stream.clone()),
            cancel_rx: Some(cancel_rx),
        })
        .await;

    let result = match provider_result {
        Ok(result) => result,
        Err(ProviderError::Cancelled) => {
            persist_artifacts(state, &stream, run.id, connector_name, None)?;
            state.run_streams.remove(run.id);
            return Err(JobCancelled.into());
        }
        Err(err) => {
            persist_artifacts(state, &stream, run.id, connector_name, None)?;
            state.run_streams.remove(run.id);
            return Err(anyhow::anyhow!("agent provider: {err}"));
        }
    };

    if run_svc.is_cancelled(run.id).await? {
        persist_artifacts(state, &stream, run.id, connector_name, None)?;
        state.run_streams.remove(run.id);
        return Err(JobCancelled.into());
    }

    let apply = result_contract::apply_agent_result(&result)
        .map_err(|err| anyhow::anyhow!("apply agent result: {err}"))?;

    let worktree_path = paths.worktree_dir.to_string_lossy().into_owned();
    let finished_run = run_svc
        .finish_with_apply(
            run.id,
            apply,
            Some(worktree_path),
            Some(paths.branch_name.clone()),
        )
        .await
        .context("finish run with apply")?;

    persist_artifacts(state, &stream, run.id, connector_name, None)?;
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
    let frames = stream.buffered_tail();
    let mut log_bytes = Vec::new();
    for frame in &frames {
        log_bytes.extend_from_slice(&frame.data);
    }
    ArtifactService::write_terminal_log(&paths, &log_bytes)?;
    ArtifactService::write_meta(
        &paths,
        &RunArtifactMeta {
            provider: connector_name.into(),
            session_id,
            frame_count: frames.len() as u64,
            ended_at: time::OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_default(),
        },
    )?;
    Ok(())
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
