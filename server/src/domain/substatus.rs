use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Backlog,
    Ready,
    InProgress,
    InReview,
    InQa,
    WaitForFinalReview,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Substatus {
    WaitingForAgent,
    WaitingForHuman,
    WaitingForOwner,
    WaitingForCi,
    BlockedByMissingCapability,
    BlockedByMissingSecret,
    BlockedByPermission,
    BlockedByError,
}

pub fn substatus_label(s: Substatus) -> &'static str {
    match s {
        Substatus::WaitingForAgent => "Waiting for agent",
        Substatus::WaitingForHuman => "Waiting for you",
        Substatus::WaitingForOwner => "Waiting for owner",
        Substatus::WaitingForCi => "Waiting for CI",
        Substatus::BlockedByMissingCapability => "Blocked — capability",
        Substatus::BlockedByMissingSecret => "Blocked — secret",
        Substatus::BlockedByPermission => "Blocked — permission",
        Substatus::BlockedByError => "Blocked — error",
    }
}

pub fn validate_substatus(
    substatus: Option<Substatus>,
    metadata: &Option<Value>,
) -> Option<&'static str> {
    let s = substatus?;
    let empty = Value::Object(Default::default());
    let meta = metadata.as_ref().unwrap_or(&empty);
    match s {
        Substatus::WaitingForAgent => {
            let valid = meta
                .get("agentId")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .is_some();
            if !valid {
                return Some("agentId required");
            }
        }
        Substatus::BlockedByMissingCapability
            if meta.get("capability").and_then(|v| v.as_str()).is_none() =>
        {
            return Some("capability required");
        }
        Substatus::BlockedByMissingSecret
            if meta.get("secretKey").and_then(|v| v.as_str()).is_none() =>
        {
            return Some("secretKey required");
        }
        _ => {}
    }
    None
}

pub fn validate_status_substatus_combo(
    status: TicketStatus,
    substatus: Option<Substatus>,
    metadata: &Option<Value>,
) -> Option<&'static str> {
    if let Some(msg) = validate_substatus(substatus, metadata) {
        return Some(msg);
    }
    if status == TicketStatus::Done {
        if let Some(s) = substatus {
            return Some(match s {
                Substatus::WaitingForAgent
                | Substatus::WaitingForHuman
                | Substatus::WaitingForOwner
                | Substatus::WaitingForCi => "done not allowed with waiting substatus",
                Substatus::BlockedByMissingCapability
                | Substatus::BlockedByMissingSecret
                | Substatus::BlockedByPermission
                | Substatus::BlockedByError => "done not allowed with blocked substatus",
            });
        }
    }
    None
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubstatusDisplay {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub fn build_substatus_display(
    substatus: Option<Substatus>,
    metadata: &Option<Value>,
    agent_name: Option<&str>,
) -> Option<SubstatusDisplay> {
    let s = substatus?;
    let label = substatus_label(s).to_string();
    let detail = match s {
        Substatus::WaitingForAgent => agent_name.map(String::from),
        Substatus::BlockedByMissingSecret => metadata
            .as_ref()
            .and_then(|m| m.get("secretKey"))
            .and_then(|v| v.as_str())
            .map(String::from),
        Substatus::BlockedByMissingCapability => metadata
            .as_ref()
            .and_then(|m| m.get("capability"))
            .and_then(|v| v.as_str())
            .map(String::from),
        Substatus::WaitingForHuman | Substatus::WaitingForOwner => metadata
            .as_ref()
            .and_then(|m| m.get("reason"))
            .and_then(|v| v.as_str())
            .map(String::from),
        _ => None,
    };
    Some(SubstatusDisplay { label, detail })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn waiting_for_agent_requires_agent_id() {
        let err = validate_substatus(
            Some(Substatus::WaitingForAgent),
            &Some(serde_json::json!({})),
        );
        assert!(err.is_some());
    }

    #[test]
    fn waiting_for_agent_accepts_valid_metadata() {
        let agent_id = Uuid::new_v4();
        assert!(validate_substatus(
            Some(Substatus::WaitingForAgent),
            &Some(serde_json::json!({ "agentId": agent_id })),
        )
        .is_none());
    }

    #[test]
    fn done_rejects_waiting_substatus() {
        assert!(validate_status_substatus_combo(
            TicketStatus::Done,
            Some(Substatus::WaitingForHuman),
            &None,
        )
        .is_some());
    }

    #[test]
    fn display_waiting_for_agent_uses_generic_label() {
        let label = substatus_label(Substatus::WaitingForAgent);
        assert_eq!(label, "Waiting for agent");
    }
}
