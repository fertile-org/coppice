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
use uuid::Uuid;

/// Validates and stores the run's structured result. The latest *valid*
/// submission wins; invalid ones are rejected and never overwrite it.
pub async fn call_result_submit(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let result: AgentRunResult = serde_json::from_value(args)
        .map_err(|e| ToolError::InvalidArgs(format!("invalid result: {e}")))?;
    validate_for_profile(&result, ctx.scope.profile, &ctx.scope.job_type)
        .map_err(ToolError::InvalidArgs)?;

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
    let warnings = target_warnings(&result, &keys, ctx.scope.agent_id);

    Ok(json!({ "accepted": true, "warnings": warnings }))
}

async fn run_is_running(pool: &PgPool, run_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT status = 'running' FROM agent_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(pool)
        .await
        .map(|running: Option<bool>| running.unwrap_or(false))
}

/// Advisory checks on the agents a result points at; nothing here rejects.
fn target_warnings(
    result: &AgentRunResult,
    keys: &HashMap<String, Uuid>,
    own_agent: Uuid,
) -> Vec<String> {
    let mut warnings = Vec::new();

    let (assign_to, mention_agents, agent_requests) = match result {
        AgentRunResult::Done {
            assign_to,
            mention_agents,
            agent_requests,
            ..
        } => (assign_to.as_deref(), mention_agents.as_slice(), agent_requests.as_slice()),
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
    run_id: Uuid,
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
    run_id: Uuid,
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

    fn keys() -> (HashMap<String, Uuid>, Uuid) {
        let own = Uuid::new_v4();
        let mut map = HashMap::new();
        map.insert("me".to_string(), own);
        for k in ["qc", "dba", "pm"] {
            map.insert(k.to_string(), Uuid::new_v4());
        }
        (map, own)
    }

    #[test]
    fn warns_on_unknown_self_duplicate_and_excess_targets() {
        let (map, own) = keys();
        let w = target_warnings(&done(Some("nobody"), &["me", "qc", "qc"], &["dba", "pm"]), &map, own);
        assert!(w.iter().any(|m| m.contains("assignTo") && m.contains("nobody")), "{w:?}");
        assert!(w.iter().any(|m| m.contains("mentionAgents[0]") && m.contains("itself")), "{w:?}");
        assert!(w.iter().any(|m| m.contains("mentionAgents[2]") && m.contains("duplicate")), "{w:?}");
        assert!(w.iter().any(|m| m.contains("agentRequests[1].agentKey") && m.contains("pm") && m.contains("limit")), "{w:?}");
        assert_eq!(w.len(), 4, "{w:?}");
    }

    #[test]
    fn clean_targets_have_no_warnings() {
        let (map, own) = keys();
        assert!(target_warnings(&done(Some("qc"), &["dba"], &[]), &map, own).is_empty());
    }
}
