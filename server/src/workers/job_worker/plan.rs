use std::path::{Path, PathBuf};

use anyhow::Context;
use sqlx::PgPool;

use super::{best_effort_persist_artifacts, JobCancelled};
use crate::copy::plan::{self, PLAN_CHANGES_DISCARDED, PLAN_MUST_FINISH};
use crate::domain::comment::{author_type_to_str, AuthorType, CommentIntent};
use crate::domain::run::{AgentRun, RunStatus};
use crate::domain::slug::slugify;
use crate::domain::ticket::status_to_str;
use crate::events::{publish_run_finished, AppEvent};
use crate::mcp::grant::grant_for_run;
use crate::mcp::token::NewRunToolScope;
use crate::mcp::tools::result::prefer_submitted_result;
use crate::providers::{
    connector_enforces_read_only, AgentProvider, AgentRunInput, AgentRunResult, ProviderError,
};
use crate::services::agent_service::AgentService;
use crate::services::artifact_service::{ArtifactService, RunArtifactPaths};
use crate::services::comment_service::CommentService;
use crate::services::context_builder::{
    build_tool_first_context, write_context_document, ContextInput,
};
use crate::services::planning_service::PlanningService;
use crate::services::repo_service::RepoService;
use crate::services::result_contract::{self, ApplyComment, ApplyResult, ApplyTicketUpdate};
use crate::services::run_orchestrator::RunOrchestrator;
use crate::services::run_service::RunService;
use crate::services::ticket_service::TicketService;
use crate::services::worktree_service::{
    compute_paths, snapshot_repo_refs, PlanScratch, PlanScratchSource, RepoRefSnapshot,
};
use crate::AppState;

/// Run a `plan_ticket` job in a detached scratch worktree.
/// Planning never changes your ticket's code.
pub(super) async fn execute_plan(
    state: &AppState,
    pool: &PgPool,
    run_svc: &RunService<'_>,
    run: &AgentRun,
) -> anyhow::Result<()> {
    let ticket_id = run.ticket_id.context("planning run has no ticket")?;
    let ticket = TicketService::new(pool)
        .get(ticket_id)
        .await
        .context("load ticket")?;
    let agent = AgentService::new(pool)
        .get(run.agent_id)
        .await
        .context("load agent")?;
    let connector_name = agent.connector.as_str();
    let connector = state
        .connectors
        .registry()
        .get(connector_name)
        .with_context(|| {
            crate::connectors_runtime::connector_unavailable_message(connector_name)
        })?;
    let agent_key = agent
        .preset_source
        .clone()
        .unwrap_or_else(|| slugify(&agent.name));

    let source = plan_scratch_source(state, pool, ticket_id, ticket.ticket.repo_id).await?;
    let git_dir = source.as_ref().map(|source| source.git_dir.clone());
    let refs_before = snapshot_plan_refs(git_dir.as_deref(), ticket_id).await;
    let created = PlanScratch::create(
        Path::new(&state.config.agent.worktrees_path),
        run.id,
        source,
    )
    .await
    .context("create plan scratch worktree");
    let outcome = match created {
        Ok(scratch) => {
            let cwd = scratch.path().to_path_buf();
            let outcome = drive_plan_run(
                state,
                pool,
                run_svc,
                run,
                &PlanDrive {
                    ticket: &ticket,
                    connector: connector.as_ref(),
                    agent: &agent,
                    agent_key: &agent_key,
                    cwd: &cwd,
                },
            )
            .await;
            if scratch.discard().await {
                note_discarded_plan_changes(pool, ticket_id).await;
            }
            outcome
        }
        Err(err) => Err(err),
    };
    record_plan_ref_changes(
        &state.config.storage.artifacts_dir,
        run.id,
        git_dir.as_deref(),
        refs_before.as_ref(),
    )
    .await;
    outcome
}

async fn snapshot_plan_refs(
    git_dir: Option<&Path>,
    ticket_id: uuid::Uuid,
) -> Option<RepoRefSnapshot> {
    let dir = git_dir?;
    match snapshot_repo_refs(dir).await {
        Ok(snapshot) => Some(snapshot),
        Err(err) => {
            tracing::warn!(error = %err, %ticket_id, "plan run could not snapshot repo refs");
            None
        }
    }
}

/// When branches, tags, or remotes moved, append the command diff to the run log.
async fn record_plan_ref_changes(
    artifacts_dir: &str,
    run_id: uuid::Uuid,
    git_dir: Option<&Path>,
    before: Option<&RepoRefSnapshot>,
) {
    let (Some(git_dir), Some(before)) = (git_dir, before) else {
        return;
    };
    let after = match snapshot_repo_refs(git_dir).await {
        Ok(after) => after,
        Err(err) => {
            tracing::warn!(error = %err, %run_id, "plan run could not snapshot repo refs");
            return;
        }
    };
    let Some(diff) = before.diff(&after) else {
        return;
    };
    append_plan_ref_diff(artifacts_dir, run_id, &diff);
}

fn append_plan_ref_diff(artifacts_dir: &str, run_id: uuid::Uuid, diff: &str) {
    let paths = RunArtifactPaths::new(artifacts_dir, &run_id.to_string());
    let mut bytes = std::fs::read(&paths.terminal_log).unwrap_or_default();
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    bytes.extend_from_slice(diff.as_bytes());
    if !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    if let Err(err) = ArtifactService::write_terminal_log(&paths, &bytes) {
        tracing::warn!(error = %err, %run_id, "plan run could not write ref diff to the run log");
    }
}

async fn plan_scratch_source(
    state: &AppState,
    pool: &PgPool,
    ticket_id: uuid::Uuid,
    repo_id: Option<uuid::Uuid>,
) -> anyhow::Result<Option<PlanScratchSource>> {
    let Some(repo_id) = repo_id else {
        return Ok(None);
    };
    let repo = RepoService::new(pool)
        .get(repo_id)
        .await
        .context("load repo")?;
    let paths = compute_paths(
        Path::new(&state.config.agent.worktrees_path),
        &repo.name,
        ticket_id,
    );
    let fallback_branch = if repo.default_branch.is_empty() {
        "main".to_string()
    } else {
        repo.default_branch
    };
    Ok(Some(PlanScratchSource {
        git_dir: PathBuf::from(repo.local_path),
        ticket_branch: paths.branch_name,
        fallback_branch,
    }))
}

async fn note_discarded_plan_changes(pool: &PgPool, ticket_id: uuid::Uuid) {
    if let Err(err) = CommentService::new(pool)
        .create(
            ticket_id,
            AuthorType::System,
            None,
            PLAN_CHANGES_DISCARDED,
            CommentIntent::SystemEvent,
            &[],
            &[],
        )
        .await
    {
        tracing::warn!(error = %err, %ticket_id, "failed to note discarded plan changes");
    }
}

struct PlanDrive<'a> {
    ticket: &'a crate::services::ticket_service::TicketWithDisplay,
    connector: &'a dyn AgentProvider,
    agent: &'a crate::domain::agent::Agent,
    agent_key: &'a str,
    cwd: &'a Path,
}

async fn drive_plan_run(
    state: &AppState,
    pool: &PgPool,
    run_svc: &RunService<'_>,
    run: &AgentRun,
    drive: &PlanDrive<'_>,
) -> anyhow::Result<()> {
    let ticket = drive.ticket;
    let agent = drive.agent;
    let agent_key = drive.agent_key;
    let cwd = drive.cwd;
    let connector = drive.connector;
    let connector_name = agent.connector.as_str();
    let ticket_id = ticket.ticket.id;

    let agents = AgentService::new(pool)
        .list_agents()
        .await
        .context("load agents")?;
    let assignee_key = ticket.ticket.assignee_agent_id.and_then(|id| {
        agents
            .iter()
            .find(|candidate| candidate.id == id)
            .map(|candidate| {
                candidate
                    .preset_source
                    .clone()
                    .unwrap_or_else(|| slugify(&candidate.name))
            })
    });

    let changes = if let Some(comment_id) = run.trigger_comment_id {
        Some(
            CommentService::new(pool)
                .get(comment_id)
                .await
                .context("load plan change request")?
                .body,
        )
    } else {
        None
    };

    let context_input = ContextInput {
        ticket_title: &ticket.ticket.title,
        ticket_description: "",
        ticket_status: status_to_str(ticket.ticket.status),
        ticket_substatus: None,
        agent_name: &agent.name,
        agent_key,
        agent_role: &agent.role,
        agent_skills: &agent.skills,
        agent_responsibilities: &agent.responsibilities,
        agent_system_prompt: &agent.system_prompt,
        repo_name: None,
        repo_remote_url: None,
        repo_default_branch: None,
        worktree_path: None,
        latest_comments: None,
        project_rules: None,
        resume_context: None,
        context_profile: run.context_profile,
        human_request: None,
        ticket_id: Some(ticket_id),
        assignee_agent_key: assignee_key.as_deref(),
        // Empty excerpt keeps this out of the "work on ticket" path.
        thread_excerpt: Some(""),
    };
    let plugin_ids = super::run_plugin_snapshot(pool, run).await;
    let skills = state.skills.skills_for(&plugin_ids);
    let mut markdown = build_tool_first_context(&context_input, &skills, None);
    markdown.push_str("\n# Plan\n\n");
    markdown.push_str(&plan::plan_instructions(changes.as_deref()));
    write_context_document(cwd, &markdown).context("write plan context")?;
    let context_path = cwd.join(".agent").join("context.md");

    let stream = state.run_streams.register(run.id);
    let cancel_rx = stream.cancelled_rx();
    run_svc
        .mark_running(run.id)
        .await
        .context("mark planning run running")?;

    let grant = grant_for_run(
        state,
        pool,
        NewRunToolScope {
            run_id: run.id,
            agent_id: run.agent_id,
            ticket_id: Some(ticket_id),
            chat_session_id: None,
            board_id: Some(ticket.ticket.board_id),
            profile: run.context_profile,
            job_type: run.job_type.clone(),
            compaction_ticket_ids: Vec::new(),
            plugin_ids,
        },
    )
    .await
    .context("mint planning tool token")?;

    tracing::info!(run_id = %run.id, %ticket_id, connector = connector_name, "planning run started");

    let provider_result = connector
        .run(AgentRunInput {
            agent_id: run.agent_id.to_string(),
            agent_key: agent_key.to_string(),
            agent_role: agent.role.clone(),
            job_type: run.job_type.clone(),
            ticket_id: Some(ticket_id.to_string()),
            ticket_status: Some(ticket.ticket.status),
            context_profile: run.context_profile,
            context_path: context_path.to_string_lossy().into_owned(),
            run_id: Some(run.id.to_string()),
            chat_session_id: None,
            artifacts_dir: Some(state.config.storage.artifacts_dir.clone()),
            stream: Some(stream.clone()),
            cancel_rx: Some(cancel_rx),
            model_provider: agent.model_provider.clone(),
            model: agent.model.clone(),
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
            read_only_tools: connector_enforces_read_only(connector_name),
            mcp: Some(grant.access.clone()),
        })
        .await;
    grant.revoke().await;
    let provider_result = prefer_submitted_result(pool, run.id, provider_result).await;

    let result = match provider_result {
        Ok(result) => result,
        Err(ProviderError::Cancelled) => {
            best_effort_persist_artifacts(
                state,
                &stream,
                run.id,
                connector_name,
                None,
                "after cancel",
            );
            state.run_streams.remove(run.id);
            return Err(JobCancelled.into());
        }
        Err(err) => {
            best_effort_persist_artifacts(
                state,
                &stream,
                run.id,
                connector_name,
                None,
                "after provider error",
            );
            state.run_streams.remove(run.id);
            return Err(anyhow::Error::new(err)
                .context(format!("connector `{connector_name}` planning run failed")));
        }
    };
    best_effort_persist_artifacts(state, &stream, run.id, connector_name, None, "after plan");
    state.run_streams.remove(run.id);
    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    let summary = match &result {
        AgentRunResult::Done { summary, .. } => summary.clone(),
        _ => anyhow::bail!(PLAN_MUST_FINISH),
    };
    result_contract::validate_for_profile(&result, run.context_profile, &run.job_type)
        .map_err(|err| anyhow::anyhow!(err))?;

    let apply = ApplyResult {
        run_status: RunStatus::Succeeded,
        ticket: ApplyTicketUpdate {
            status: None,
            substatus: None,
            substatus_metadata: None,
            updated_description: None,
            acceptance_criteria: None,
        },
        comment: ApplyComment {
            body: plan::format_plan(&summary),
            intent: CommentIntent::Plan,
            mentions: Vec::new(),
        },
    };

    let orchestrator =
        RunOrchestrator::new(pool, &state.config.workflow).with_event_bus(&state.event_bus);
    let finished = orchestrator
        .finish_run(run, &result, apply, None, None)
        .await
        .context("finish planning run")?;

    PlanningService::new(pool)
        .bind_latest_plan(ticket_id)
        .await
        .context("bind plan comment")?;

    let updated = TicketService::new(pool)
        .get(ticket_id)
        .await
        .context("load ticket after plan")?;
    crate::events::publish_ticket_updated(&state.event_bus, &updated);
    let comments = CommentService::new(pool)
        .list_by_ticket(ticket_id)
        .await
        .context("list comments after plan")?;
    if let Some(comment) = comments.last() {
        state.event_bus.publish(AppEvent::CommentCreated {
            comment_id: comment.id,
            ticket_id,
            author_type: author_type_to_str(comment.author_type).into(),
        });
    }
    publish_run_finished(
        state,
        pool,
        finished.id,
        finished.ticket_id,
        finished.agent_id,
        finished.status,
        finished.error_message,
    )
    .await;

    tracing::info!(run_id = %run.id, %ticket_id, "planning run finished");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::append_plan_ref_diff;
    use crate::services::artifact_service::{ArtifactService, RunArtifactPaths};
    use uuid::Uuid;

    #[test]
    fn ref_diff_is_appended_to_the_run_log() {
        let dir = tempfile::tempdir().expect("tempdir");
        let artifacts = dir.path().to_str().expect("utf8");
        let run_id = Uuid::from_u128(9);
        let paths = RunArtifactPaths::new(artifacts, &run_id.to_string());
        ArtifactService::write_terminal_log(&paths, b"Mock agent starting...\n").expect("log");
        append_plan_ref_diff(
            artifacts,
            run_id,
            "git remote -v\n--- before\n\n+++ after\norigin\thttps://example.test/repo.git (fetch)\n",
        );
        let log = std::fs::read_to_string(&paths.terminal_log).expect("read log");
        assert!(log.starts_with("Mock agent starting...\n"));
        assert!(log.contains("git remote -v\n--- before\n"));
        assert!(log.contains("origin"));
    }
}
