use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    #[serde(rename = "ticket.updated")]
    TicketUpdated {
        ticket_id: Uuid,
        status: String,
        substatus: Option<String>,
        updated_at: String,
    },
    #[serde(rename = "agent_run.started")]
    AgentRunStarted {
        run_id: Uuid,
        ticket_id: Uuid,
        agent_id: Uuid,
        status: String,
    },
    #[serde(rename = "agent_run.finished")]
    AgentRunFinished {
        run_id: Uuid,
        ticket_id: Uuid,
        agent_id: Uuid,
        status: String,
        error_message: Option<String>,
    },
    #[serde(rename = "comment.created")]
    CommentCreated {
        comment_id: Uuid,
        ticket_id: Uuid,
        author_type: String,
    },
}

pub struct EventBus {
    tx: tokio::sync::broadcast::Sender<AppEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(256);
        Self { tx }
    }

    pub fn publish(&self, event: AppEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_run_finished_serializes() {
        let event = AppEvent::AgentRunFinished {
            run_id: Uuid::nil(),
            ticket_id: Uuid::nil(),
            agent_id: Uuid::nil(),
            status: "succeeded".into(),
            error_message: None,
        };
        let raw = serde_json::to_string(&event).unwrap();
        assert!(raw.contains("agent_run.finished"));
    }
}
