use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub enum LiveMessage {
    Frame { seq: u64, data: Vec<u8> },
    Snapshot { snapshot: Value },
    Event { event: Value },
    Heartbeat {
        session_status: Option<String>,
        elapsed_secs: u64,
    },
    End {
        status: String,
        reason: Option<String>,
        recoverable: bool,
    },
}

impl LiveMessage {
    pub fn to_ws_json(&self) -> Value {
        match self {
            LiveMessage::Frame { seq, data } => json!({
                "type": "frame",
                "seq": seq,
                "data": String::from_utf8_lossy(data),
            }),
            LiveMessage::Snapshot { snapshot } => json!({
                "type": "snapshot",
                "messages": snapshot.get("messages").cloned().unwrap_or(json!([])),
                "parts": snapshot.get("parts").cloned().unwrap_or(json!({})),
                "sessionId": snapshot.get("sessionId"),
            }),
            LiveMessage::Event { event } => json!({
                "type": "event",
                "event": event,
            }),
            LiveMessage::Heartbeat {
                session_status,
                elapsed_secs,
            } => json!({
                "type": "heartbeat",
                "sessionStatus": session_status,
                "elapsedSecs": elapsed_secs,
            }),
            LiveMessage::End {
                status,
                reason,
                recoverable,
            } => json!({
                "type": "end",
                "status": status,
                "reason": reason,
                "recoverable": recoverable,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frame_encodes_as_legacy_type() {
        let msg = LiveMessage::Frame {
            seq: 1,
            data: b"hello\n".to_vec(),
        };
        let json = msg.to_ws_json();
        assert_eq!(json["type"], "frame");
        assert_eq!(json["seq"], 1);
        assert_eq!(json["data"], "hello\n");
    }

    #[test]
    fn event_encodes_raw_payload() {
        let event = json!({"type": "message.part.delta", "properties": {}});
        let msg = LiveMessage::Event { event: event.clone() };
        let json = msg.to_ws_json();
        assert_eq!(json["type"], "event");
        assert_eq!(json["event"], event);
    }

    #[test]
    fn heartbeat_encodes_session_status_and_elapsed() {
        let msg = LiveMessage::Heartbeat {
            session_status: Some("busy".into()),
            elapsed_secs: 120,
        };
        let json = msg.to_ws_json();
        assert_eq!(json["type"], "heartbeat");
        assert_eq!(json["sessionStatus"], "busy");
        assert_eq!(json["elapsedSecs"], 120);
    }

    #[test]
    fn end_includes_recoverable_flag() {
        let msg = LiveMessage::End {
            status: "failed".into(),
            reason: Some("interrupted: server restarted".into()),
            recoverable: false,
        };
        let json = msg.to_ws_json();
        assert_eq!(json["type"], "end");
        assert_eq!(json["recoverable"], false);
        assert_eq!(json["reason"], "interrupted: server restarted");
    }
}
