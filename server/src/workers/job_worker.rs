use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use sqlx::PgPool;
use sqlx::Row;

use crate::domain::ticket::{status_to_str, substatus_to_str};
use crate::providers::AgentRunInput;
use crate::services::agent_service::AgentService;
use crate::services::context_builder::{write_context_file, ContextInput};
use crate::services::job_service::JobService;
use crate::services::result_contract;
use crate::services::run_service::RunService;
use crate::services::ticket_service::TicketService;
use crate::services::worktree_service::{compute_paths, WorktreeService};
use crate::AppState;

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
        }
        Err(err) => {
            if run_svc.is_cancelled(run.id).await.unwrap_or(false) {
                job_svc.mark_cancelled(job.id).await?;
            } else {
                fail_job(pool, run.id, job.id, &err.to_string()).await?;
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

    let repo_row = sqlx::query("SELECT name, remote_url FROM repos WHERE id = $1")
        .bind(repo_id)
        .fetch_optional(pool)
        .await?
        .context("repo not found")?;
    let repo_name: String = repo_row.get("name");
    let remote_url: Option<String> = repo_row.get("remote_url");
    let remote_url = remote_url
        .filter(|url| !url.trim().is_empty())
        .context("repo has no remote_url")?;

    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    run_svc.mark_running(run.id).await.context("mark run running")?;

    let worktree_svc = WorktreeService::new(
        state.config.agent.git_repos_path.clone().into(),
        state.config.agent.worktrees_path.clone().into(),
    );
    let paths = compute_paths(
        worktree_svc.repos_root(),
        worktree_svc.worktrees_root(),
        repo_id,
        &repo_name,
        run.ticket_id,
        &agent.name,
    );

    worktree_svc
        .ensure_repo_clone(&remote_url, &paths.repo_dir)
        .await
        .context("ensure repo clone")?;
    worktree_svc
        .ensure_worktree(&paths.repo_dir, &paths.worktree_dir, &paths.branch_name)
        .await
        .context("ensure worktree")?;

    let ticket_substatus = ticket
        .ticket
        .substatus
        .as_ref()
        .map(|s| substatus_to_str(*s));
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

    let result = state
        .agent_provider
        .run(AgentRunInput {
            agent_id: run.agent_id.to_string(),
            ticket_id: Some(run.ticket_id.to_string()),
            context_path,
        })
        .await
        .map_err(|err| anyhow::anyhow!("agent provider: {err}"))?;

    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    let apply = result_contract::apply_agent_result(&result)
        .map_err(|err| anyhow::anyhow!("apply agent result: {err}"))?;

    let worktree_path = paths.worktree_dir.to_string_lossy().into_owned();
    run_svc
        .finish_with_apply(
            run.id,
            apply,
            Some(worktree_path),
            Some(paths.branch_name.clone()),
        )
        .await
        .context("finish run with apply")?;

    Ok(())
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
