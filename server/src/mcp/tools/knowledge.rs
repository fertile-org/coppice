use crate::domain::context_profile::ContextProfile;
use crate::domain::knowledge::{
    confidence_from_str, type_from_str, KnowledgeConfidence, KnowledgeRevisionInput,
    KnowledgeScope, KnowledgeSourceType,
};
use crate::knowledge::citations::{
    resolve_anchors, validate_citation_path, ResolvedCitation, MAX_CITATIONS_PER_CANDIDATE,
};
use crate::knowledge::freshness::{freshness_for, Freshness};
use crate::knowledge::retrieval::{fetch_visible, retrieve, RetrievalScope};
use crate::domain::chat_action::KNOWLEDGE_TYPES;
use crate::mcp::tools::{ToolCtx, ToolError};
use crate::services::knowledge_service::{
    find_close_item, KnowledgeError, KnowledgeService, RunProposal,
};
use crate::services::context_budget::{
    record_usage, render_knowledge, ByteTokenCounter, RenderedKnowledge, KNOWLEDGE_DATA_NOTE,
};
use serde_json::{json, Value};
use uuid::Uuid;

const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 20;
const MAX_QUERY_CHARS: usize = 2_000;
const SNIPPET_CHARS: usize = 200;
const MAX_GET_IDS: usize = 5;
const MAX_FLAG_REASON_CHARS: usize = 1_000;
const MAX_PROPOSALS_PER_RUN: i64 = 3;
const VERIFY_NOTE: &str = "Cited code changed since this was written. Check it before relying on it, then call knowledge_flag.";

fn parse_limit(args: &Value) -> Result<usize, ToolError> {
    match args.get("limit").filter(|v| !v.is_null()) {
        None => Ok(DEFAULT_LIMIT),
        Some(v) => {
            let n = v
                .as_i64()
                .ok_or_else(|| ToolError::InvalidArgs("limit must be an integer".into()))?;
            Ok(usize::try_from(n.clamp(1, MAX_LIMIT as i64)).unwrap_or(DEFAULT_LIMIT))
        }
    }
}

/// Searches approved knowledge visible to the run's agent.
/// Conversation chat searches every board. Compaction runs search the boards of
/// their batch tickets. Other runs stay on their board, or return nothing when
/// they have none. Retrieval already restricts to approved,
/// live, in-scope entries, including agent-scoped notes for this agent only.
pub async fn call_knowledge_search(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or_else(|| ToolError::InvalidArgs("query is required".into()))?;
    let limit = parse_limit(&args)?;

    if !ctx.state.config.knowledge.enabled {
        return Ok(response(Vec::new()));
    }

    let query: String = query.chars().take(MAX_QUERY_CHARS).collect();
    let workspace = ctx.scope.workspace_chat();
    let Some(scope) = tool_scope(ctx).await? else {
        return Ok(response(Vec::new()));
    };
    let mut found = retrieve(
        ctx.pool,
        &scope,
        ctx.scope.agent_id,
        &query,
        "",
        &ctx.state.config.knowledge.retrieval,
    )
    .await?;
    // Retrieval returns the whole eligible set; non-matches carry score 0.
    found.retain(|k| k.score > 0.0);
    found.truncate(limit);

    // Rows carry a snippet and the token cost of reading the whole entry with
    // `knowledge_get`; usage is logged there, not here.
    let section = render_knowledge(&found, usize::MAX, &ByteTokenCounter);
    let revision_ids: Vec<Uuid> = found.iter().map(|k| k.revision_id).collect();
    let freshness = freshness_for(ctx.pool, &revision_ids).await;
    let items: Vec<Value> = found
        .iter()
        .zip(&section.entries)
        .map(|(k, entry)| {
            let mut item = json!({
                "id": k.item_id,
                "title": k.title,
                "type": k.knowledge_type,
                "snippet": k.content.chars().take(SNIPPET_CHARS).collect::<String>(),
                "tokens": entry.token_count,
                "freshness": freshness.get(&k.revision_id).map_or(Freshness::Unknown, |r| r.state),
            });
            if workspace {
                item["boardName"] = json!(k.board_name.as_deref());
            }
            item
        })
        .collect();
    Ok(response(items))
}

/// Returns whole entries for up to five ids from `knowledge_search`, and logs
/// each revision once per run. An id the run cannot see fails the call the
/// same way an unknown id does.
pub async fn call_knowledge_get(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let ids = parse_ids(&args)?;
    let not_found = || ToolError::NotFound("knowledge item not found".into());
    if !ctx.state.config.knowledge.enabled {
        return Err(not_found());
    }
    let Some(scope) = tool_scope(ctx).await? else {
        return Err(not_found());
    };
    let found = fetch_visible(
        ctx.pool,
        &scope,
        ctx.scope.agent_id,
        &ids,
        &ctx.state.config.knowledge.retrieval,
    )
    .await?;
    if found.len() != ids.len() {
        return Err(not_found());
    }
    let section = render_knowledge(&found, usize::MAX, &ByteTokenCounter);
    log_usage(ctx, &section.entries).await?;
    let workspace = ctx.scope.workspace_chat();
    let revision_ids: Vec<Uuid> = found.iter().map(|k| k.revision_id).collect();
    let freshness = freshness_for(ctx.pool, &revision_ids).await;
    let items: Vec<Value> = found
        .iter()
        .zip(&section.entries)
        .map(|(k, entry)| {
            let report = freshness.get(&k.revision_id);
            let state = report.map_or(Freshness::Unknown, |r| r.state);
            let mut item = json!({
                "id": k.item_id,
                "revisionId": k.revision_id,
                "title": k.title,
                "type": k.knowledge_type,
                "content": entry.rendered_content,
                "freshness": state,
                "changedPaths": report.map_or(&[][..], |r| r.paths.as_slice()),
            });
            if matches!(state, Freshness::Changed | Freshness::Missing) {
                item["verify"] = json!(VERIFY_NOTE);
            }
            if workspace {
                item["boardName"] = json!(k.board_name.as_deref());
            }
            item
        })
        .collect();
    Ok(response(items))
}

/// Records an agent's verdict on one visible knowledge entry. A flag never
/// retires the entry: reviewers see it in the Inbox. One row per run and
/// revision; a repeat call replaces the earlier verdict. `confirmed` moves the
/// revision's citations to the current head of each repo, for paths that
/// still exist there.
pub async fn call_knowledge_flag(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let item_id = args
        .get("itemId")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ToolError::InvalidArgs("itemId must be a knowledge item id".into()))?;
    let verdict = args
        .get("verdict")
        .and_then(Value::as_str)
        .filter(|v| matches!(*v, "outdated" | "wrong" | "confirmed"))
        .ok_or_else(|| {
            ToolError::InvalidArgs("verdict must be outdated, wrong or confirmed".into())
        })?;
    let reason = args
        .get("reason")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if reason.chars().count() > MAX_FLAG_REASON_CHARS {
        return Err(ToolError::InvalidArgs(format!(
            "reason must be at most {MAX_FLAG_REASON_CHARS} characters"
        )));
    }
    if verdict != "confirmed" && reason.is_empty() {
        return Err(ToolError::InvalidArgs(format!(
            "reason is required when the verdict is {verdict}"
        )));
    }

    let not_found = || ToolError::NotFound("knowledge item not found".into());
    if !ctx.state.config.knowledge.enabled {
        return Err(not_found());
    }
    let Some(scope) = tool_scope(ctx).await? else {
        return Err(not_found());
    };
    let found = fetch_visible(
        ctx.pool,
        &scope,
        ctx.scope.agent_id,
        &[item_id],
        &ctx.state.config.knowledge.retrieval,
    )
    .await?;
    let Some(entry) = found.first() else {
        return Err(not_found());
    };
    let revision_id = entry.revision_id;

    sqlx::query(
        r#"
        INSERT INTO knowledge_feedback (id, item_id, revision_id, run_id, agent_id, verdict, reason)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (run_id, revision_id)
        DO UPDATE SET verdict = EXCLUDED.verdict, reason = EXCLUDED.reason, created_at = now()
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(item_id)
    .bind(revision_id)
    .bind(ctx.scope.run_id)
    .bind(ctx.scope.agent_id)
    .bind(verdict)
    .bind(reason)
    .execute(ctx.pool)
    .await?;

    if verdict == "confirmed" {
        reanchor_citations(ctx.pool, revision_id).await?;
    }
    Ok(json!({ "recorded": true, "itemId": item_id, "verdict": verdict }))
}

/// Lets a working agent propose one Pending item for the run's board. Nothing
/// a run proposes is searchable until a reviewer approves it. A close match
/// already on the board comes back as `existing` instead of a second copy. A
/// citation that fails validation is dropped and reported; the item is still
/// created.
pub async fn call_knowledge_propose(ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    let (Some(ticket_id), Some(board_id)) = (ctx.scope.ticket_id, ctx.scope.board_id) else {
        return Err(ToolError::Denied(
            "knowledge_propose is only available on ticket runs".into(),
        ));
    };
    let text = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| ToolError::InvalidArgs(format!("{key} is required")))
    };
    let knowledge_type = type_from_str(text("type")?).ok_or_else(|| {
        ToolError::InvalidArgs(format!(
            "type must be one of: {}",
            KNOWLEDGE_TYPES.join(", ")
        ))
    })?;
    let title = text("title")?.to_string();
    let content = text("content")?.to_string();
    let confidence = match args.get("confidence").filter(|v| !v.is_null()) {
        None => KnowledgeConfidence::Medium,
        Some(v) => v
            .as_str()
            .and_then(confidence_from_str)
            .ok_or_else(|| ToolError::InvalidArgs("confidence must be low, medium or high".into()))?,
    };
    let requested: Vec<String> = match args.get("citations").filter(|v| !v.is_null()) {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .ok_or_else(|| ToolError::InvalidArgs("citations must be an array of paths".into()))?
            .iter()
            .map(|p| {
                p.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| ToolError::InvalidArgs("citations must be an array of paths".into()))
            })
            .collect::<Result<_, _>>()?,
    };

    if !ctx.state.config.knowledge.enabled {
        return Err(ToolError::Denied("knowledge is turned off".into()));
    }
    let mut paths: Vec<String> = Vec::new();
    let mut dropped: Vec<Value> = Vec::new();
    for raw in requested {
        match validate_citation_path(&raw) {
            Ok(path) if paths.contains(&path) => {}
            Ok(path) if paths.len() >= MAX_CITATIONS_PER_CANDIDATE => dropped.push(json!({
                "path": path,
                "reason": format!("at most {MAX_CITATIONS_PER_CANDIDATE} citations are kept"),
            })),
            Ok(path) => paths.push(path),
            Err(reason) => dropped.push(json!({ "path": raw, "reason": reason })),
        }
    }
    let repo_id: Option<Uuid> = sqlx::query_scalar("SELECT repo_id FROM tickets WHERE id = $1")
        .bind(ticket_id)
        .fetch_optional(ctx.pool)
        .await?
        .flatten();
    let citations: Vec<ResolvedCitation> = resolve_anchors(ctx.pool, repo_id, &paths).await;

    let input = KnowledgeRevisionInput {
        scope: KnowledgeScope::Board,
        board_id: Some(board_id),
        agent_id: None,
        knowledge_type,
        title,
        content,
        source_type: KnowledgeSourceType::Ticket,
        source_id: Some(ticket_id),
        source_run_id: Some(ctx.scope.run_id),
        confidence,
    };
    let run = RunProposal {
        run_id: ctx.scope.run_id,
        ticket_id,
        board_id,
        repo_id,
    };
    // One run's proposals are serialized, so parallel calls cannot slip past
    // the limit or both insert the same note.
    let mut tx = ctx.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(ctx.scope.run_id)
        .execute(&mut *tx)
        .await?;
    let proposed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM knowledge_revisions r \
         JOIN knowledge_provenance p ON p.id = r.provenance_id WHERE p.run_id = $1",
    )
    .bind(ctx.scope.run_id)
    .fetch_one(&mut *tx)
    .await?;
    if proposed >= MAX_PROPOSALS_PER_RUN {
        return Err(ToolError::Limit(format!(
            "knowledge_propose limit reached for this run ({MAX_PROPOSALS_PER_RUN})"
        )));
    }
    if let Some(item_id) = find_close_item(&mut tx, board_id, &input.title, &input.content).await? {
        return Ok(json!({ "itemId": item_id, "status": "existing" }));
    }
    let item_id = KnowledgeService::new(ctx.pool, &ctx.state.config.knowledge)
        .create_from_run_in_tx(&mut tx, run, input, &citations)
        .await
        .map_err(|e| match e {
            KnowledgeError::Validation(m) => ToolError::InvalidArgs(m),
            KnowledgeError::Capacity(m) => ToolError::Limit(m),
            other => ToolError::Internal(other.into()),
        })?;
    tx.commit().await?;
    let mut out = json!({ "itemId": item_id, "status": "pending" });
    if !dropped.is_empty() {
        out["droppedCitations"] = Value::Array(dropped);
    }
    Ok(out)
}

/// Moves each citation of the revision to its repo's current head. A path that
/// no longer exists there keeps its old anchor and so keeps reading as missing.
async fn reanchor_citations(pool: &sqlx::PgPool, revision_id: Uuid) -> Result<(), ToolError> {
    let rows: Vec<(Option<Uuid>, String)> = sqlx::query_as(
        "SELECT repo_id, path FROM knowledge_citations WHERE revision_id = $1 ORDER BY path",
    )
    .bind(revision_id)
    .fetch_all(pool)
    .await?;
    let mut by_repo: Vec<(Uuid, Vec<String>)> = Vec::new();
    for (repo_id, path) in rows {
        let Some(repo_id) = repo_id else { continue };
        match by_repo.iter_mut().find(|(id, _)| *id == repo_id) {
            Some((_, paths)) => paths.push(path),
            None => by_repo.push((repo_id, vec![path])),
        }
    }
    for (repo_id, paths) in by_repo {
        for anchor in resolve_anchors(pool, Some(repo_id), &paths).await {
            let Some(sha) = anchor.commit_sha else {
                continue;
            };
            sqlx::query(
                "UPDATE knowledge_citations SET commit_sha = $4 \
                 WHERE revision_id = $1 AND repo_id = $2 AND path = $3",
            )
            .bind(revision_id)
            .bind(repo_id)
            .bind(&anchor.path)
            .bind(sha)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

fn parse_ids(args: &Value) -> Result<Vec<Uuid>, ToolError> {
    let raw = args
        .get("ids")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| ToolError::InvalidArgs("ids must be a non-empty array".into()))?;
    if raw.len() > MAX_GET_IDS {
        return Err(ToolError::InvalidArgs(format!(
            "ids takes at most {MAX_GET_IDS} entries"
        )));
    }
    let mut ids: Vec<Uuid> = Vec::with_capacity(raw.len());
    for value in raw {
        let id = value
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| ToolError::InvalidArgs("ids must be knowledge item ids".into()))?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// The boards a run's knowledge tools may read; `None` when it has no board.
pub(crate) async fn tool_scope(ctx: &ToolCtx<'_>) -> Result<Option<RetrievalScope>, ToolError> {
    if ctx.scope.workspace_chat() {
        return Ok(Some(RetrievalScope::AllBoards));
    }
    if ctx.scope.profile == ContextProfile::KnowledgeCompaction {
        let boards: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT board_id FROM tickets WHERE id = ANY($1) AND board_id IS NOT NULL",
        )
        .bind(&ctx.scope.compaction_ticket_ids)
        .fetch_all(ctx.pool)
        .await?;
        return Ok(Some(RetrievalScope::Boards(boards)));
    }
    Ok(ctx.scope.board_id.map(|id| RetrievalScope::Boards(vec![id])))
}

fn response(items: Vec<Value>) -> Value {
    json!({ "note": KNOWLEDGE_DATA_NOTE, "items": items })
}

/// Records each returned revision once per run (earlier searches may already
/// have logged some).
async fn log_usage(ctx: &ToolCtx<'_>, entries: &[RenderedKnowledge]) -> Result<(), ToolError> {
    if entries.is_empty() {
        return Ok(());
    }
    let logged: Vec<Uuid> =
        sqlx::query_scalar("SELECT revision_id FROM knowledge_usage_logs WHERE run_id = $1")
            .bind(ctx.scope.run_id)
            .fetch_all(ctx.pool)
            .await?;
    let existing = i32::try_from(logged.len()).unwrap_or(i32::MAX);
    let fresh: Vec<RenderedKnowledge> = entries
        .iter()
        .filter(|e| !logged.contains(&e.revision_id))
        .enumerate()
        .map(|(i, e)| RenderedKnowledge {
            rank: existing.saturating_add(i32::try_from(i + 1).unwrap_or(i32::MAX)),
            ..e.clone()
        })
        .collect();
    if fresh.is_empty() {
        return Ok(());
    }
    record_usage(ctx.pool, ctx.scope.run_id, &fresh).await?;
    Ok(())
}
