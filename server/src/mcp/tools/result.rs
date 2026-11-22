use crate::domain::Id;
use crate::domain::context_profile::ContextProfile;
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::providers::{AgentRunResult, ProviderError};
use crate::services::agent_service::AgentService;
use crate::services::mention_service::resolve_agent_keys;
use crate::services::result_contract::validate_for_profile;
use crate::services::workflow_service::MAX_MENTIONS_PER_RUN;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
/// Validates and stores the run's structured result. The latest *valid*
/// submission wins, except in connector-check runs, which keep the first
/// stored one; invalid ones are rejected and never overwrite it.
pub async fn call_result_submit(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let result: AgentRunResult = serde_json::from_value(args)
        .map_err(|e| ToolError::InvalidArgs(format!("invalid result: {e}")))?;
    if let AgentRunResult::Done {
        advanced: Some(_),
        ..
    } = &result
    {
        if ctx.scope.job_type != crate::domain::plugin_build::JOB_TYPE_PLAN_PLUGIN_BUILD {
            return Err(ToolError::InvalidArgs(
                "advanced is only allowed on plan_plugin_build results".into(),
            ));
        }
    }
    validate_for_profile(&result, ctx.scope.profile, &ctx.scope.job_type)
        .map_err(ToolError::InvalidArgs)?;
    refuse_foreign_chat_design_sets(ctx, &result).await?;
    if let AgentRunResult::Done {
        plan_questions,
        no_visible_change,
        summary,
        ..
    } = &result
    {
        if !plan_questions.is_empty() {
            questions_allowed(ctx).await?;
        }
        if *no_visible_change
            && ctx.scope.job_type != crate::domain::workflow::JOB_TYPE_PLAN_TICKET
            && ctx.scope.job_type != crate::domain::plugin_build::JOB_TYPE_PLAN_PLUGIN_BUILD
        {
            return Err(ToolError::InvalidArgs(
                crate::copy::plan::NO_VISIBLE_CHANGE_PLAN_ONLY.into(),
            ));
        }
        if ctx.scope.job_type == crate::domain::workflow::JOB_TYPE_PLAN_TICKET
            && !*no_visible_change
            && plan_questions.is_empty()
        {
            refuse_too_many_technical_sets(ctx).await?;
            refuse_unknown_design_markers(ctx, summary).await?;
        }
    }

    let stored = serde_json::to_value(&result).map_err(|e| ToolError::Internal(e.into()))?;
    // A connection check is decided by its first submitted outcome.
    let first_wins = ctx.scope.profile == ContextProfile::ConnectorCheck;
    let updated = sqlx::query(
        "UPDATE agent_runs SET submitted_result = $1 \
         WHERE id = $2 AND status = 'running' AND (NOT $3 OR submitted_result IS NULL)",
    )
    .bind(stored)
    .bind(ctx.scope.run_id)
    .bind(first_wins)
    .execute(ctx.pool)
    .await?
    .rows_affected();
    if updated == 0 {
        if first_wins && run_is_running(ctx.pool, ctx.scope.run_id).await? {
            return Err(ToolError::Denied(
                "a result was already submitted for this check".into(),
            ));
        }
        return Err(ToolError::Denied("this run is no longer running".into()));
    }

    let agents = AgentService::new(ctx.pool)
        .list_agents()
        .await
        .map_err(|e| ToolError::Internal(e.into()))?;
    let keys = resolve_agent_keys(&agents);
    let mut warnings = target_warnings(&result, &keys, ctx.scope.agent_id);
    if ctx.scope.job_type == crate::domain::workflow::JOB_TYPE_PLAN_TICKET
        || ctx.scope.job_type == crate::domain::plugin_build::JOB_TYPE_PLAN_PLUGIN_BUILD
    {
        warnings.extend(plan_warnings(&result));
    }

    Ok(json!({ "accepted": true, "warnings": warnings }))
}

/// Length advice on a written plan; questions stand in for a plan, so they get none.
fn plan_warnings(result: &AgentRunResult) -> Vec<String> {
    match result {
        AgentRunResult::Done {
            summary,
            plan_questions,
            ..
        } if plan_questions.is_empty() && !summary.trim().is_empty() => {
            crate::copy::plan::plan_length_warnings(summary)
        }
        _ => Vec::new(),
    }
}

/// `designSetId` on a chat `create_ticket` action must be a published set on this chat session.
async fn refuse_foreign_chat_design_sets(
    ctx: &ToolCtx<'_>,
    result: &AgentRunResult,
) -> Result<(), ToolError> {
    let AgentRunResult::Done {
        actions: Some(actions),
        ..
    } = result
    else {
        return Ok(());
    };
    if ctx.scope.profile != ContextProfile::Conversation || ctx.scope.job_type != "chat_turn" {
        return Ok(());
    }
    let Some(session_id) = ctx.scope.chat_session_id else {
        return Ok(());
    };
    let parsed = crate::domain::chat_action::parse_chat_actions(actions)
        .map_err(ToolError::InvalidArgs)?;
    for action in parsed {
        let crate::domain::chat_action::ChatAction::CreateTicket {
            design_set_id: Some(set_id),
            ..
        } = action
        else {
            continue;
        };
        let owned: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM design_sets
                WHERE id = $1
                  AND chat_session_id = $2
                  AND published_at IS NOT NULL
            )
            "#,
        )
        .bind(set_id)
        .bind(session_id)
        .fetch_one(ctx.pool)
        .await?;
        if !owned {
            return Err(ToolError::InvalidArgs(format!(
                "designSetId `{set_id}` is not a published design set on this chat session"
            )));
        }
    }
    Ok(())
}

/// Each `{{design:<id>}}` marker in the plan must point at a live published set on this ticket.
async fn refuse_unknown_design_markers(ctx: &ToolCtx<'_>, summary: &str) -> Result<(), ToolError> {
    let ids = crate::services::design_markers::design_marker_ids(summary);
    if ids.is_empty() {
        return Ok(());
    }
    let ticket_id = match ctx.scope.ticket_id {
        Some(id) => id,
        None => sqlx::query_scalar("SELECT ticket_id FROM agent_runs WHERE id = $1")
            .bind(ctx.scope.run_id)
            .fetch_optional(ctx.pool)
            .await?
            .flatten()
            .ok_or_else(|| {
                ToolError::InvalidArgs("design markers need a ticket.".into())
            })?,
    };
    for id in ids {
        let live: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM design_sets
                WHERE id = $1
                  AND ticket_id = $2
                  AND published_at IS NOT NULL
                  AND retired_at IS NULL
                  AND superseded_by IS NULL
            )
            "#,
        )
        .bind(id)
        .bind(ticket_id)
        .fetch_one(ctx.pool)
        .await?;
        if !live {
            return Err(ToolError::InvalidArgs(format!(
                "design marker {{design:{id}}} is not a live published set on this ticket; fix the id or call `design__list` to see available sets"
            )));
        }
    }
    Ok(())
}

/// At most two technical design sets may land on one planning run.
async fn refuse_too_many_technical_sets(ctx: &ToolCtx<'_>) -> Result<(), ToolError> {
    use crate::services::design_service::{DesignService, MAX_TECHNICAL_SETS_PER_PLAN};
    let count = DesignService::new(ctx.pool, &ctx.state.config.storage.artifacts_dir)
        .technical_sets_on_run(ctx.scope.run_id)
        .await
        .map_err(|err| ToolError::Internal(anyhow::anyhow!("{err}")))?;
    if count > MAX_TECHNICAL_SETS_PER_PLAN {
        return Err(ToolError::InvalidArgs(format!(
            "this plan has {count} technical design sets; keep at most {MAX_TECHNICAL_SETS_PER_PLAN}, or propose a split"
        )));
    }
    Ok(())
}

/// Questions belong to a planning run. Ticket plans also require no prior
/// question round; plugin-build plans allow one round per run.
async fn questions_allowed(ctx: &ToolCtx<'_>) -> Result<(), ToolError> {
    if ctx.scope.job_type == crate::domain::plugin_build::JOB_TYPE_PLAN_PLUGIN_BUILD {
        return Ok(());
    }
    if ctx.scope.job_type != crate::domain::workflow::JOB_TYPE_PLAN_TICKET {
        return Err(ToolError::InvalidArgs(
            "planQuestions is only for planning runs.".into(),
        ));
    }
    let ticket_id: Option<Id> =
        sqlx::query_scalar("SELECT ticket_id FROM agent_runs WHERE id = $1")
            .bind(ctx.scope.run_id)
            .fetch_optional(ctx.pool)
            .await?
            .flatten();
    let Some(ticket_id) = ticket_id else {
        return Err(ToolError::InvalidArgs(
            "planQuestions needs a ticket.".into(),
        ));
    };
    let allowed = crate::services::planning_service::PlanningService::new(ctx.pool)
        .can_ask_questions(ticket_id)
        .await
        .map_err(|e| ToolError::Internal(anyhow::anyhow!("{e}")))?;
    if allowed {
        Ok(())
    } else {
        Err(ToolError::InvalidArgs(
            crate::copy::plan::PLAN_QUESTIONS_ALREADY_ASKED.into(),
        ))
    }
}

async fn run_is_running(pool: &PgPool, run_id: Id) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT status = 'running' FROM agent_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(pool)
        .await
        .map(|running: Option<bool>| running.unwrap_or(false))
}

/// Advisory checks on the agents a result points at; nothing here rejects.
fn target_warnings(
    result: &AgentRunResult,
    keys: &HashMap<String, Id>,
    own_agent: Id,
) -> Vec<String> {
    let mut warnings = Vec::new();

    let (assign_to, mention_agents, agent_requests) = match result {
        AgentRunResult::Done {
            assign_to,
            mention_agents,
            agent_requests,
            ..
        } => (
            assign_to.as_deref(),
            mention_agents.as_slice(),
            agent_requests.as_slice(),
        ),
        AgentRunResult::Blocked {
            assign_to,
            mention_agents,
            ..
        } => (assign_to.as_deref(), mention_agents.as_slice(), &[][..]),
        AgentRunResult::Continued { .. } => return warnings,
    };

    if let Some(key) = assign_to.map(str::trim).filter(|k| !k.is_empty()) {
        match keys.get(key) {
            None => warnings.push(format!(
                "assignTo `{key}` is not a known enabled agent; see board_agents"
            )),
            Some(id) if *id == own_agent => {
                warnings.push(format!("assignTo `{key}` is this agent itself"));
            }
            Some(_) => {}
        }
    }

    let labelled = mention_agents
        .iter()
        .enumerate()
        .map(|(i, key)| (format!("mentionAgents[{i}]"), key.as_str()))
        .chain(
            agent_requests
                .iter()
                .enumerate()
                .map(|(i, r)| (format!("agentRequests[{i}].agentKey"), r.agent_key.as_str())),
        );

    let mut seen = HashSet::new();
    let mut accepted = 0usize;
    for (label, key) in labelled {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        match keys.get(key) {
            None => warnings.push(format!(
                "{label} `{key}` is not a known enabled agent; see board_agents"
            )),
            Some(id) if *id == own_agent => {
                warnings.push(format!("{label} `{key}` is this agent itself"));
            }
            Some(_) if !seen.insert(key.to_string()) => {
                warnings.push(format!("{label} `{key}` is a duplicate"));
            }
            Some(_) => {
                accepted += 1;
                if accepted > MAX_MENTIONS_PER_RUN as usize {
                    warnings.push(format!(
                        "{label} `{key}` exceeds the limit of {MAX_MENTIONS_PER_RUN} mentioned agents per run and will be ignored"
                    ));
                }
            }
        }
    }

    warnings
}

/// The last valid result the agent submitted through `result_submit`, if any.
/// A stored value that no longer deserializes is treated as absent.
pub async fn take_submitted_result(
    pool: &PgPool,
    run_id: Id,
) -> anyhow::Result<Option<AgentRunResult>> {
    let stored: Option<Option<Value>> =
        sqlx::query_scalar("SELECT submitted_result FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_optional(pool)
            .await?;
    let Some(value) = stored.flatten() else {
        return Ok(None);
    };
    match serde_json::from_value(value) {
        Ok(result) => Ok(Some(result)),
        Err(err) => {
            tracing::warn!(%run_id, error = %err, "stored submitted_result is unreadable; ignoring");
            Ok(None)
        }
    }
}

/// Finish-time rule shared by every run path: a submitted result wins over
/// whatever the provider returned (or failed to find) on stdout. Cancellation
/// and other provider errors ignore any submission.
pub async fn prefer_submitted_result(
    pool: &PgPool,
    run_id: Id,
    provider_result: Result<AgentRunResult, ProviderError>,
) -> Result<AgentRunResult, ProviderError> {
    if !matches!(
        provider_result,
        Ok(_) | Err(ProviderError::MissingResult(_))
    ) {
        return provider_result;
    }
    match take_submitted_result(pool, run_id).await {
        Ok(Some(submitted)) => Ok(submitted),
        Ok(None) => provider_result,
        Err(err) => {
            tracing::warn!(%run_id, error = %err, "could not read submitted result; using provider result");
            provider_result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(assign_to: Option<&str>, mentions: &[&str], requests: &[&str]) -> AgentRunResult {
        serde_json::from_value(json!({
            "status": "done",
            "summary": "s",
            "assignTo": assign_to,
            "mentionAgents": mentions,
            "agentRequests": requests.iter().map(|k| json!({"agentKey": k, "intent": "help", "request": "r"})).collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    fn keys() -> (HashMap<String, Id>, Id) {
        let own = Id::new();
        let mut map = HashMap::new();
        map.insert("me".to_string(), own);
        for k in ["qc", "dba", "pm"] {
            map.insert(k.to_string(), Id::new());
        }
        (map, own)
    }

    #[test]
    fn warns_on_unknown_self_duplicate_and_excess_targets() {
        let (map, own) = keys();
        let w = target_warnings(
            &done(Some("nobody"), &["me", "qc", "qc"], &["dba", "pm"]),
            &map,
            own,
        );
        assert!(
            w.iter()
                .any(|m| m.contains("assignTo") && m.contains("nobody")),
            "{w:?}"
        );
        assert!(
            w.iter()
                .any(|m| m.contains("mentionAgents[0]") && m.contains("itself")),
            "{w:?}"
        );
        assert!(
            w.iter()
                .any(|m| m.contains("mentionAgents[2]") && m.contains("duplicate")),
            "{w:?}"
        );
        assert!(
            w.iter().any(|m| m.contains("agentRequests[1].agentKey")
                && m.contains("pm")
                && m.contains("limit")),
            "{w:?}"
        );
        assert_eq!(w.len(), 4, "{w:?}");
    }

    #[test]
    fn clean_targets_have_no_warnings() {
        let (map, own) = keys();
        assert!(target_warnings(&done(Some("qc"), &["dba"], &[]), &map, own).is_empty());
    }
}
