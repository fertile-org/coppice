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
        #[serde(default, rename = "projectId")]
        project_id: Option<Uuid>,
    },
    CreateKnowledge {
        title: String,
        content: String,
        #[serde(default = "default_knowledge_type", rename = "knowledgeType")]
        knowledge_type: String,
        #[serde(default = "default_scope")]
        scope: String,
        #[serde(default, rename = "projectId")]
        project_id: Option<Uuid>,
    },
    Cutoff,
}

fn default_knowledge_type() -> String {
    "coding_convention".into()
}

fn default_scope() -> String {
    "project".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatActionResultMeta {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_item_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_session_id: Option<Uuid>,
}

/// Parse `actions` array from a chat-turn result object.
pub fn parse_chat_actions(value: &serde_json::Value) -> Result<Vec<ChatAction>, String> {
    let Some(actions) = value.get("actions") else {
        return Ok(Vec::new());
    };
    let Some(arr) = actions.as_array() else {
        return Err("actions must be an array".into());
    };
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

/// Deterministic compact of a chat transcript for knowledge / cutoff seed.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_empty_actions_ok() {
        assert_eq!(parse_chat_actions(&json!({})).unwrap(), vec![]);
        assert_eq!(parse_chat_actions(&json!({ "actions": [] })).unwrap(), vec![]);
    }

    #[test]
    fn parse_create_ticket_knowledge_cutoff() {
        let value = json!({
            "actions": [
                {
                    "type": "create_ticket",
                    "title": "Fix cwd",
                    "description": "Details",
                    "projectId": "11111111-1111-1111-1111-111111111111"
                },
                {
                    "type": "create_knowledge",
                    "title": "Rule",
                    "content": "Always resolve chat cwd",
                    "knowledgeType": "coding_convention",
                    "scope": "project"
                },
                { "type": "cutoff" }
            ]
        });
        let actions = parse_chat_actions(&value).expect("parse");
        assert_eq!(actions.len(), 3);
        match &actions[0] {
            ChatAction::CreateTicket { title, project_id, .. } => {
                assert_eq!(title, "Fix cwd");
                assert!(project_id.is_some());
            }
            other => panic!("expected create_ticket, got {other:?}"),
        }
        assert!(matches!(actions[1], ChatAction::CreateKnowledge { .. }));
        assert!(matches!(actions[2], ChatAction::Cutoff));
    }

    #[test]
    fn unknown_action_type_fails() {
        let err = parse_chat_actions(&json!({
            "actions": [{ "type": "board_move", "status": "done" }]
        }))
        .expect_err("unknown");
        assert!(err.contains("invalid chat action") || err.contains("unknown"));
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
}
