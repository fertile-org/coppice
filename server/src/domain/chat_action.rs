use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Compact server-validated chat actions (distinct from ticket result contract).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatAction {
    CreateTicket {
        title: String,
        #[serde(default)]
        description: String,
        #[serde(default, rename = "repoId")]
        repo_id: Option<Uuid>,
        #[serde(default, rename = "boardId")]
        board_id: Option<Uuid>,
    },
    CreateKnowledge {
        title: String,
        content: String,
        #[serde(default = "default_knowledge_type", rename = "knowledgeType")]
        knowledge_type: String,
        #[serde(default = "default_scope")]
        scope: String,
        #[serde(default, rename = "boardId")]
        board_id: Option<Uuid>,
    },
}

fn default_knowledge_type() -> String {
    "coding_convention".into()
}

fn default_scope() -> String {
    "board".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatActionResultMeta {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_item_id: Option<Uuid>,
}

/// Most proposals one chat turn may carry.
pub const MAX_CHAT_ACTIONS: usize = 5;

/// Parse and validate the `actions` array of a chat-turn result.
pub fn parse_chat_actions(actions: &serde_json::Value) -> Result<Vec<ChatAction>, String> {
    let Some(arr) = actions.as_array() else {
        return Err("actions must be an array".into());
    };
    if arr.len() > MAX_CHAT_ACTIONS {
        return Err(format!("actions allows at most {MAX_CHAT_ACTIONS} per turn"));
    }
    let mut out = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let action: ChatAction = serde_json::from_value(item.clone()).map_err(|err| {
            format!(
                "invalid chat action at index {i}: {err} (unknown or malformed action type)"
            )
        })?;
        match &action {
            ChatAction::CreateTicket { title, .. } if title.trim().is_empty() => {
                return Err(format!("create_ticket at index {i}: title is required"));
            }
            ChatAction::CreateKnowledge { title, content, .. }
                if title.trim().is_empty() || content.trim().is_empty() =>
            {
                return Err(format!(
                    "create_knowledge at index {i}: title and content are required"
                ));
            }
            _ => {}
        }
        out.push(action);
    }
    Ok(out)
}

/// Pending proposal cards for an agent message's `action_metadata`.
/// Returns `None` when there is nothing to propose.
pub fn proposals_metadata(actions: &[ChatAction]) -> Option<serde_json::Value> {
    if actions.is_empty() {
        return None;
    }
    let proposals: Vec<serde_json::Value> = actions
        .iter()
        .map(|action| match action {
            ChatAction::CreateTicket {
                title,
                description,
                repo_id,
                board_id,
            } => serde_json::json!({
                "id": Uuid::new_v4(),
                "type": "create_ticket",
                "title": title,
                "description": description,
                "repoId": repo_id,
                "boardId": board_id,
                "status": "pending",
            }),
            ChatAction::CreateKnowledge {
                title,
                content,
                knowledge_type,
                scope,
                board_id,
            } => serde_json::json!({
                "id": Uuid::new_v4(),
                "type": "create_knowledge",
                "title": title,
                "content": content,
                "knowledgeType": knowledge_type,
                "scope": scope,
                "boardId": board_id,
                "status": "pending",
            }),
        })
        .collect();
    Some(serde_json::json!({ "proposals": proposals }))
}

/// Find one proposal in an agent message's `action_metadata` by id.
pub fn proposal_mut<'a>(
    metadata: &'a mut serde_json::Value,
    proposal_id: &str,
) -> Option<&'a mut serde_json::Value> {
    metadata
        .get_mut("proposals")?
        .as_array_mut()?
        .iter_mut()
        .find(|p| p.get("id").and_then(serde_json::Value::as_str) == Some(proposal_id))
}

/// Deterministic compact of a chat transcript for knowledge drafts.
pub fn compact_transcript(transcript: &str, max_chars: usize) -> String {
    let trimmed = transcript.trim();
    if trimmed.is_empty() {
        return "(empty conversation)".into();
    }
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{truncated}…")
}

/// Draft a ticket title from the latest human turn in a transcript.
pub fn draft_ticket_title(transcript: &str) -> String {
    let blocks: Vec<&str> = transcript.split("### ").collect();
    for block in blocks.into_iter().rev() {
        let block = block.trim();
        if let Some(rest) = block.strip_prefix("Human\n\n") {
            let line = rest.lines().next().unwrap_or(rest).trim();
            if !line.is_empty() {
                let title: String = line.chars().take(120).collect();
                return title;
            }
        }
    }
    "From agent chat".into()
}

/// Where a create-from-chat draft came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftTicketSource {
    Agent,
    Fallback,
}

impl DraftTicketSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Fallback => "fallback",
        }
    }
}

/// Validated board-ready draft fields (title required; description preferred).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftTicketFields {
    pub title: String,
    pub description: String,
}

/// Map a Done agent result into draft fields. Returns None when title is empty
/// or missing; empty description is treated as invalid so callers fall back.
pub fn draft_fields_from_agent_summary(
    summary: &str,
    updated_description: Option<&str>,
) -> Option<DraftTicketFields> {
    let title = summary.trim();
    if title.is_empty() {
        return None;
    }
    let description = updated_description.map(str::trim).unwrap_or("");
    if description.is_empty() {
        return None;
    }
    Some(DraftTicketFields {
        title: title.to_string(),
        description: description.to_string(),
    })
}

/// Deterministic fallback used when the agent draft fails or is invalid.
pub fn fallback_draft_ticket(transcript: &str, max_chars: usize) -> DraftTicketFields {
    DraftTicketFields {
        title: draft_ticket_title(transcript),
        description: compact_transcript(transcript, max_chars),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_empty_actions_ok() {
        assert_eq!(parse_chat_actions(&json!([])).unwrap(), vec![]);
    }

    #[test]
    fn parse_create_ticket_and_knowledge() {
        let value = json!([
                {
                    "type": "create_ticket",
                    "title": "Fix cwd",
                    "description": "Details",
                    "boardId": "11111111-1111-1111-1111-111111111111"
                },
                {
                    "type": "create_knowledge",
                    "title": "Rule",
                    "content": "Always resolve chat cwd",
                    "knowledgeType": "coding_convention",
                    "scope": "board"
                }
            ]);
        let actions = parse_chat_actions(&value).expect("parse");
        assert_eq!(actions.len(), 2);
        match &actions[0] {
            ChatAction::CreateTicket { title, board_id, .. } => {
                assert_eq!(title, "Fix cwd");
                assert!(board_id.is_some());
            }
            other => panic!("expected create_ticket, got {other:?}"),
        }
        assert!(matches!(actions[1], ChatAction::CreateKnowledge { .. }));
    }

    #[test]
    fn unknown_action_type_fails() {
        let err = parse_chat_actions(&json!([{ "type": "board_move", "status": "done" }]))
            .expect_err("unknown");
        assert!(err.contains("invalid chat action") || err.contains("unknown"));
    }

    #[test]
    fn parse_rejects_non_array_and_too_many() {
        assert!(parse_chat_actions(&json!({ "type": "create_ticket" })).is_err());
        let one = json!({ "type": "create_ticket", "title": "T" });
        let six = json!([one, one, one, one, one, one]);
        assert!(parse_chat_actions(&six).expect_err("cap").contains("at most 5"));
    }

    #[test]
    fn compact_transcript_truncates() {
        let long = "a".repeat(100);
        let out = compact_transcript(&long, 20);
        assert_eq!(out.chars().count(), 20);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn draft_ticket_title_from_last_human() {
        let transcript = "### Human\n\nFirst\n\n### Agent\n\nOk\n\n### Human\n\nShip the feature please\n\n";
        assert_eq!(draft_ticket_title(transcript), "Ship the feature please");
    }

    #[test]
    fn draft_fields_from_agent_requires_title_and_description() {
        assert!(draft_fields_from_agent_summary("  ", Some("body")).is_none());
        assert!(draft_fields_from_agent_summary("Title", None).is_none());
        assert!(draft_fields_from_agent_summary("Title", Some("  ")).is_none());
        let fields = draft_fields_from_agent_summary("  Fix cwd  ", Some("  ## Context\n\nDetails  "))
            .expect("valid");
        assert_eq!(fields.title, "Fix cwd");
        assert_eq!(fields.description, "## Context\n\nDetails");
    }

    #[test]
    fn fallback_draft_uses_deterministic_helpers() {
        let transcript = "### Human\n\nShip the feature please\n\n";
        let fields = fallback_draft_ticket(transcript, 4_000);
        assert_eq!(fields.title, "Ship the feature please");
        assert!(fields.description.contains("Ship the feature please"));
    }
}
