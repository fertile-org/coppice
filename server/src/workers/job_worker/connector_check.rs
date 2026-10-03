use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use sqlx::PgPool;
use uuid::Uuid;

use super::{best_effort_persist_artifacts, JobCancelled};
use crate::domain::connector_check::{evaluate_check, sanitize_failure};
use crate::domain::context_profile::ContextProfile;
use crate::domain::run::{AgentRun, RunStatus};
use crate::domain::slug::slugify;
use crate::mcp::grant::grant_for_run;
use crate::mcp::token::NewRunToolScope;
use crate::mcp::tools::result::prefer_submitted_result;
use crate::providers::{AgentRunInput, AgentRunResult, ProviderError};
use crate::services::agent_service::AgentService;
use crate::services::connector_check_service::ConnectorCheckService;
use crate::services::context_builder::{connector_check_context, write_context_document};
use crate::services::run_service::RunService;
use crate::util::error_format::format_job_error;
use crate::AppState;

const MAX_CHECK_TIMEOUT_SECS: u64 = 180;

/// Run a `connector_check` job: the agent's provider against a scratch
/// directory with the two-tool `connector_check` profile. No ticket, repository,
/// comment, workflow, or notification is touched. Every exit finishes the check.
pub(super) async fn execute_connector_check(
    state: &AppState,
    pool: &PgPool,
    run_svc: &RunService<'_>,
    run: &AgentRun,
) -> anyhow::Result<()> {
    let check_id: Uuid = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT connector_check_id FROM agent_runs WHERE id = $1",
    )
    .bind(run.id)
    .fetch_one(pool)
    .await?
    .context("connector check run has no connector_check_id")?;
    let checks = ConnectorCheckService::new(pool);
    let scratch = PathBuf::from(&state.config.storage.artifacts_dir)
        .join("runs")
        .join(run.id.to_string())
        .join("check");

    let mut secrets = auth_env_values();
    let outcome = run_check(
        state,
        pool,
        run_svc,
        run,
        &checks,
        check_id,
        &scratch,
        &mut secrets,
    )
    .await;
    if let Err(err) = std::fs::remove_dir_all(&scratch) {
        tracing::debug!(error = %err, path = %scratch.display(), "failed to remove connector check scratch directory");
    }
    let secret_refs: Vec<&str> = secrets.iter().map(String::as_str).collect();
    let (passed, failure, result) = match outcome {
        Ok(Ok(())) => (true, None, Ok(())),
        Ok(Err(reason)) => (false, Some(reason), Ok(())),
        Err(err) => {
            let reason = if err.downcast_ref::<JobCancelled>().is_some() {
                "run cancelled".to_string()
            } else {
                format_job_error(&err)
            };
            (false, Some(reason), Err(err))
        }
    };
    let failure = failure.map(|f| sanitize_failure(&f, &secret_refs));
    if let Err(err) = checks.finish(check_id, passed, failure.as_deref()).await {
        tracing::warn!(run_id = %run.id, %check_id, error = %err, "failed to record connector check result");
    }
    tracing::info!(run_id = %run.id, %check_id, passed, "connector check finished");
    result
}

/// `Ok(verdict)` once the run itself finished; `Err` for every run failure.
#[allow(clippy::too_many_arguments)]
async fn run_check(
    state: &AppState,
    pool: &PgPool,
    run_svc: &RunService<'_>,
    run: &AgentRun,
    checks: &ConnectorCheckService<'_>,
    check_id: Uuid,
    scratch: &Path,
    secrets: &mut Vec<String>,
) -> anyhow::Result<Result<(), String>> {
    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }
    let agent = AgentService::new(pool)
        .get(run.agent_id)
        .await
        .context("load agent")?;
    let connector_name = agent.connector.as_str();
    let connector = state
        .connector_registry
        .get(connector_name)
        .with_context(|| format!("agent connector not configured: {connector_name}"))?;
    let agent_key = agent
        .preset_source
        .clone()
        .unwrap_or_else(|| slugify(&agent.name));

    std::fs::create_dir_all(scratch).context("create connector check scratch directory")?;
    write_context_document(scratch, &connector_check_context(&agent.name))
        .context("write connector check context")?;
    let context_path = scratch.join(".agent").join("context.md");

    let stream = state.run_streams.register(run.id);
    let cancel_rx = stream.cancelled_rx();
    run_svc
        .mark_running(run.id)
        .await
        .context("mark connector check run running")?;
    checks
        .mark_running(check_id)
        .await
        .context("mark connector check running")?;
    let grant = grant_for_run(
        state,
        pool,
        NewRunToolScope {
            run_id: run.id,
            agent_id: run.agent_id,
            ticket_id: None,
            chat_session_id: None,
            board_id: None,
            profile: ContextProfile::ConnectorCheck,
            job_type: run.job_type.clone(),
            compaction_ticket_ids: Vec::new(),
            plugin_ids: Vec::new(),
        },
    )
    .await
    .context("mint connector check tool token")?;
    secrets.push(grant.access.token.clone());
    tracing::info!(run_id = %run.id, %check_id, connector = connector_name, "connector check started");

    let timeout_secs = state
        .config
        .agent
        .connectors
        .run_timeout_secs(connector_name)
        .unwrap_or(MAX_CHECK_TIMEOUT_SECS)
        .min(MAX_CHECK_TIMEOUT_SECS);
    let provider_run = connector.run(AgentRunInput {
        agent_id: run.agent_id.to_string(),
        agent_key,
        agent_role: agent.role.clone(),
        job_type: run.job_type.clone(),
        ticket_id: None,
        ticket_status: None,
        context_profile: ContextProfile::ConnectorCheck,
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
        read_only_tools: true,
        mcp: Some(grant.access.clone()),
    });
    let timed = tokio::time::timeout(Duration::from_secs(timeout_secs), provider_run).await;
    grant.revoke().await;
    let Ok(provider_result) = timed else {
        stream.cancel();
        best_effort_persist_artifacts(
            state,
            &stream,
            run.id,
            connector_name,
            None,
            "after timeout",
        );
        state.run_streams.remove(run.id);
        anyhow::bail!("connection check timed out after {timeout_secs}s");
    };
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
            return Err(anyhow::Error::new(err).context(format!(
                "connector `{connector_name}` connection check failed"
            )));
        }
    };
    best_effort_persist_artifacts(state, &stream, run.id, connector_name, None, "after check");
    state.run_streams.remove(run.id);
    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    let (outcome, run_status) = match result {
        AgentRunResult::Done { .. } => ("done", RunStatus::Succeeded),
        AgentRunResult::Blocked { .. } => ("blocked", RunStatus::Blocked),
        AgentRunResult::Continued { .. } => ("continued", RunStatus::Succeeded),
    };
    run_svc
        .finish_run(
            run.id,
            run_status,
            Some(scratch.to_string_lossy().into_owned()),
            None,
        )
        .await
        .context("finish connector check run")?;

    let (ticket_get_ok, result_submit_ok): (bool, bool) = sqlx::query_as(
        "SELECT \
             EXISTS (SELECT 1 FROM run_tool_calls WHERE run_id = $1 AND tool = 'ticket_get' AND status = 'ok'), \
             EXISTS (SELECT 1 FROM run_tool_calls WHERE run_id = $1 AND tool = 'result_submit' AND status = 'ok')",
    )
    .bind(run.id)
    .fetch_one(pool)
    .await
    .context("load connector check tool calls")?;
    Ok(evaluate_check(ticket_get_ok, result_submit_ok, outcome))
}

/// Values of every connector auth env var set in this process, so failure
/// text can never echo one back.
fn auth_env_values() -> Vec<String> {
    coppice_connectors::all()
        .iter()
        .flat_map(|d| d.install.auth_env.iter())
        .filter_map(|name| std::env::var(name).ok())
        .filter(|value| !value.is_empty())
        .collect()
}
