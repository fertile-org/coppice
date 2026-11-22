use crate::domain::Id;
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    #[serde(rename = "ticket.updated")]
    TicketUpdated {
        ticket_id: Id,
        status: String,
        substatus: Option<String>,
        updated_at: String,
    },
    #[serde(rename = "agent_run.started")]
    AgentRunStarted {
        run_id: Id,
        ticket_id: Id,
        agent_id: Id,
        status: String,
    },
    #[serde(rename = "agent_run.finished")]
    AgentRunFinished {
        run_id: Id,
        ticket_id: Id,
        agent_id: Id,
        status: String,
        error_message: Option<String>,
    },
    #[serde(rename = "comment.created")]
    CommentCreated {
        comment_id: Id,
        ticket_id: Id,
        author_type: String,
    },
    #[serde(rename = "agent.mentioned")]
    AgentMentioned {
        mention_id: Id,
        ticket_id: Id,
        comment_id: Id,
        mentioned_agent_id: Id,
    },
    /// A ticket run's final usage snapshot was saved by the worker.
    #[serde(rename = "usage.updated")]
    UsageUpdated { ticket_id: Id, run_id: Id },
    #[serde(rename = "notification.changed")]
    NotificationChanged { recipient_user_id: Option<Id> },
}

pub struct EventBus {
    tx: tokio::sync::broadcast::Sender<AppEvent>,
}

/// Backstop only: a lagging socket gets a `resync`, and the web app
/// refetches just the event-driven queries.
pub const EVENT_BUS_CAPACITY: usize = 1024;

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(EVENT_BUS_CAPACITY);
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
            run_id: Id::nil(),
            ticket_id: Id::nil(),
            agent_id: Id::nil(),
            status: "succeeded".into(),
            error_message: None,
        };
        let raw = serde_json::to_string(&event).unwrap();
        assert!(raw.contains("agent_run.finished"));
    }
}
