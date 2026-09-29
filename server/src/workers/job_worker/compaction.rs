use std::path::PathBuf;

use anyhow::Context;
use sqlx::PgPool;
use uuid::Uuid;

use super::{best_effort_persist_artifacts, JobCancelled};
use crate::domain::context_profile::ContextProfile;
use crate::domain::run::{AgentRun, RunStatus};
use crate::domain::slug::slugify;
use crate::knowledge::compaction_context::load_compaction_context;
use crate::mcp::grant::grant_for_run;
use crate::mcp::token::NewRunToolScope;
use crate::providers::{
    connector_enforces_read_only, AgentRunInput, AgentRunResult, ProviderError,
    READ_ONLY_CAPABLE_CONNECTORS,
};
use crate::services::agent_service::AgentService;
use crate::services::context_builder::write_context_document;
use crate::services::knowledge_compaction_service::KnowledgeCompactionService;
use crate::services::run_service::RunService;
use crate::AppState;

/// Run a `compact_knowledge` job: read-only agent over a scratch directory, no
/// ticket side effects, no knowledge retrieval. Any error fails the run; the
/// scheduler's reconcile step then fails the batch and notifies users.
pub(super) async fn execute_compaction(
    state: &AppState,
    pool: &PgPool,
    run_svc: &RunService<'_>,
    run: &AgentRun,
) -> anyhow::Result<()> {
    let batch_id: Uuid = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT compaction_batch_id FROM agent_runs WHERE id = $1",
    )
    .bind(run.id)
    .fetch_one(pool)
    .await?
    .context("compaction run has no compaction_batch_id")?;
    let agent = AgentService::new(pool)
        .get(run.agent_id)
        .await
        .context("load agent")?;
    let connector_name = agent.connector.as_str();
    if !connector_enforces_read_only(connector_name) {
        anyhow::bail!(
            "connector `{connector_name}` cannot enforce read-only tools; knowledge compaction requires one of: {}",
            READ_ONLY_CAPABLE_CONNECTORS.join(", ")
        );
    }
    let connector = state
        .connector_registry
        .get(connector_name)
        .with_context(|| format!("agent connector not configured: {connector_name}"))?;
    let agent_key = agent
        .preset_source
        .clone()
        .unwrap_or_else(|| slugify(&agent.name));

    let knowledge_config = &state.config.knowledge;
    let compaction = KnowledgeCompactionService::new(pool, knowledge_config);
    compaction.mark_batch_running(batch_id).await?;

    let cwd = PathBuf::from(&state.config.agent.worktrees_path)
        .join("knowledge-compaction")
        .join(batch_id.to_string());
    std::fs::create_dir_all(&cwd).context("create compaction scratch directory")?;
    let markdown = load_compaction_context(
        pool,
        batch_id,
        knowledge_config.compaction.max_candidates_per_batch,
        knowledge_config.compaction.batch_max_source_bytes,
    )
    .await
    .context("build compaction context")?;
    write_context_document(&cwd, &markdown).context("write compaction context")?;
    let context_path = cwd.join(".agent").join("context.md");

    let stream = state.run_streams.register(run.id);
    let cancel_rx = stream.cancelled_rx();
    run_svc
        .mark_running(run.id)
        .await
        .context("mark compaction run running")?;
    let compaction_ticket_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT ticket_id FROM knowledge_compaction_batch_tickets WHERE batch_id = $1 ORDER BY ticket_id",
    )
    .bind(batch_id)
    .fetch_all(pool)
    .await
    .context("load compaction batch tickets")?;
    let grant = grant_for_run(
        state,
        pool,
        NewRunToolScope {
            run_id: run.id,
            agent_id: run.agent_id,
            ticket_id: None,
            chat_session_id: None,
            board_id: None,
            profile: ContextProfile::KnowledgeCompaction,
            job_type: run.job_type.clone(),
            compaction_ticket_ids,
        },
    )
    .await
    .context("mint compaction tool token")?;
    tracing::info!(run_id = %run.id, %batch_id, connector = connector_name, "knowledge compaction started");

    let provider_result = connector
        .run(AgentRunInput {
            agent_id: run.agent_id.to_string(),
            agent_key,
            agent_role: agent.role.clone(),
            job_type: run.job_type.clone(),
            ticket_id: None,
            ticket_status: None,
            context_profile: ContextProfile::KnowledgeCompaction,
            context_path: context_path.to_string_lossy().into_owned(),
            run_id: Some(run.id.to_string()),
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
        })
        .await;
    grant.revoke().await;

    let result = match provider_result {
        Ok(result) => result,
        Err(ProviderError::Cancelled) => {
            best_effort_persist_artifacts(state, &stream, run.id, connector_name, None, "after cancel");
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
                .context(format!("connector `{connector_name}` knowledge compaction failed")));
        }
    };
    best_effort_persist_artifacts(state, &stream, run.id, connector_name, None, "after compaction");
    state.run_streams.remove(run.id);
    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    let (summary, candidates) = match result {
        AgentRunResult::Done {
            summary,
            knowledge_candidates,
            ..
        } => (summary, knowledge_candidates),
        AgentRunResult::Blocked { summary, .. } => {
            anyhow::bail!("compaction agent reported blocked: {summary}")
        }
        AgentRunResult::Continued { .. } => {
            anyhow::bail!("compaction agent returned `continued`; compaction must finish in one run")
        }
    };

    let outcome = compaction
        .complete_batch(batch_id, run.id, &summary, &candidates)
        .await
        .context("apply compaction result")?;
    run_svc
        .finish_run(
            run.id,
            RunStatus::Succeeded,
            Some(cwd.to_string_lossy().into_owned()),
            None,
        )
        .await
        .context("finish compaction run")?;
    if let Err(err) = std::fs::remove_dir_all(&cwd) {
        tracing::debug!(error = %err, path = %cwd.display(), "failed to remove compaction scratch directory");
    }
    tracing::info!(
        run_id = %run.id,
        %batch_id,
        created = outcome.created,
        dropped = outcome.dropped.len(),
        "knowledge compaction finished"
    );
    Ok(())
}
