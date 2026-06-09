use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone, Debug)]
struct PendingDelta {
    field: String,
    delta: String,
}

pub struct SessionSnapshot {
    pub session_id: String,
    pub messages: Vec<Value>,
    pub parts: HashMap<String, Vec<Value>>,
    pending_deltas: HashMap<String, Vec<PendingDelta>>,
    part_locations: HashMap<String, (String, usize)>,
}

impl SessionSnapshot {
    pub fn empty(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            messages: Vec::new(),
            parts: HashMap::new(),
            pending_deltas: HashMap::new(),
            part_locations: HashMap::new(),
        }
    }

    pub fn to_value(&self) -> Value {
        json!({
            "sessionId": self.session_id,
            "messages": self.messages,
            "parts": self.parts,
        })
    }

    /// Build OpenCode API-shaped messages (info + parts) for contract extraction.
    pub fn messages_for_extraction(&self) -> Vec<serde_json::Value> {
        use serde_json::json;

        self.messages
            .iter()
            .filter_map(|message| {
                let message_id = message_id_from_value(message)?;
                let parts = self.parts.get(message_id)?.clone();
                let info = message
                    .get("info")
                    .cloned()
                    .unwrap_or_else(|| message.clone());
                Some(json!({ "info": info, "parts": parts }))
            })
            .collect()
    }

    pub fn apply_event(&mut self, event: &Value) {
        let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match event_type {
            "message.part.updated" => self.apply_part_updated(event),
            "message.part.delta" => self.apply_part_delta(event),
            "message.updated" => self.apply_message_updated(event),
            _ => {}
        }
    }

    fn apply_part_delta(&mut self, event: &Value) {
        let Some(props) = event.get("properties") else {
            return;
        };
        if props.get("sessionID").and_then(|s| s.as_str()) != Some(self.session_id.as_str()) {
            return;
        }
        let Some(part_id) = props.get("partID").and_then(|s| s.as_str()) else {
            return;
        };
        let Some(field) = props.get("field").and_then(|s| s.as_str()) else {
            return;
        };
        let Some(delta) = props.get("delta").and_then(|s| s.as_str()) else {
            return;
        };

        let pending = PendingDelta {
            field: field.to_string(),
            delta: delta.to_string(),
        };

        if let Some((message_id, idx)) = self.part_locations.get(part_id).cloned() {
            if let Some(parts) = self.parts.get_mut(&message_id) {
                if let Some(part) = parts.get_mut(idx) {
                    append_field_delta(part, field, delta);
                    return;
                }
            }
            self.part_locations.remove(part_id);
        }

        self.pending_deltas
            .entry(part_id.to_string())
            .or_default()
            .push(pending);
    }

    fn apply_part_updated(&mut self, event: &Value) {
        let Some(props) = event.get("properties") else {
            return;
        };
        if props.get("sessionID").and_then(|s| s.as_str()) != Some(self.session_id.as_str()) {
            return;
        };
        let Some(part) = props.get("part") else {
            return;
        };
        let Some(part_id) = part.get("id").and_then(|s| s.as_str()) else {
            return;
        };
        let message_id = part
            .get("messageID")
            .or_else(|| part.get("messageId"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string();

        self.upsert_part(&message_id, part.clone());
        self.replay_pending_deltas(part_id);
    }

    fn apply_message_updated(&mut self, event: &Value) {
        let Some(props) = event.get("properties") else {
            return;
        };
        if props.get("sessionID").and_then(|s| s.as_str()) != Some(self.session_id.as_str()) {
            return;
        }
        let message = props
            .get("message")
            .or_else(|| props.get("info"))
            .cloned();
        let Some(message) = message else {
            return;
        };
        let message_id = message
            .get("id")
            .or_else(|| message.get("info").and_then(|i| i.get("id")))
            .and_then(|id| id.as_str());
        let Some(message_id) = message_id else {
            return;
        };

        if let Some(pos) = self
            .messages
            .iter()
            .position(|m| message_id_from_value(m) == Some(message_id))
        {
            self.messages[pos] = message;
        } else {
            self.messages.push(message);
        }
    }

    fn upsert_part(&mut self, message_id: &str, incoming: Value) {
        let Some(part_id) = incoming.get("id").and_then(|i| i.as_str()) else {
            return;
        };
        let part_id = part_id.to_string();

        let parts_vec = self.parts.entry(message_id.to_string()).or_default();

        if let Some(pos) = parts_vec
            .iter()
            .position(|p| p.get("id").and_then(|i| i.as_str()) == Some(part_id.as_str()))
        {
            let existing = parts_vec[pos].clone();
            parts_vec[pos] = merge_part(&existing, &incoming);
            self.part_locations
                .insert(part_id, (message_id.to_string(), pos));
        } else {
            let pos = parts_vec.len();
            parts_vec.push(incoming);
            self.part_locations
                .insert(part_id, (message_id.to_string(), pos));
        }
    }

    fn replay_pending_deltas(&mut self, part_id: &str) {
        let Some(deltas) = self.pending_deltas.remove(part_id) else {
            return;
        };
        for pending in deltas {
            if let Some((message_id, idx)) = self.part_locations.get(part_id).cloned() {
                if let Some(parts) = self.parts.get_mut(&message_id) {
                    if let Some(part) = parts.get_mut(idx) {
                        append_field_delta(part, &pending.field, &pending.delta);
                    }
                }
            }
        }
    }
}

fn message_id_from_value(message: &Value) -> Option<&str> {
    message
        .get("id")
        .or_else(|| message.get("info").and_then(|i| i.get("id")))
        .and_then(|id| id.as_str())
}

fn merge_part(existing: &Value, incoming: &Value) -> Value {
    let part_type = incoming.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if part_type != "text" && part_type != "reasoning" {
        return incoming.clone();
    }

    let existing_text = existing.get("text").and_then(|t| t.as_str()).unwrap_or("");
    let incoming_text = incoming.get("text").and_then(|t| t.as_str()).unwrap_or("");
    if existing_text.chars().count() > incoming_text.chars().count() {
        let mut merged = incoming.clone();
        if let Some(obj) = merged.as_object_mut() {
            obj.insert("text".to_string(), Value::String(existing_text.to_string()));
        }
        merged
    } else {
        incoming.clone()
    }
}

fn append_field_delta(part: &mut Value, field: &str, delta: &str) {
    let Some(obj) = part.as_object_mut() else {
        return;
    };
    let current = obj
        .get(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    obj.insert(field.to_string(), Value::String(format!("{current}{delta}")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn apply_delta_appends_text() {
        let mut snap = SessionSnapshot::empty("ses_1");
        snap.apply_event(&json!({
            "type": "message.part.updated",
            "properties": {
                "sessionID": "ses_1",
                "part": { "id": "prt_1", "type": "text", "text": "", "messageID": "msg_1" }
            }
        }));
        snap.apply_event(&json!({
            "type": "message.part.delta",
            "properties": {
                "sessionID": "ses_1",
                "partID": "prt_1",
                "field": "text",
                "delta": "hello"
            }
        }));
        assert_eq!(snap.parts["msg_1"][0]["text"], "hello");
    }

    #[test]
    fn delta_before_updated_buffers_then_applies() {
        let mut snap = SessionSnapshot::empty("ses_1");
        snap.apply_event(&json!({
            "type": "message.part.delta",
            "properties": {
                "sessionID": "ses_1",
                "partID": "prt_1",
                "field": "text",
                "delta": "early"
            }
        }));
        snap.apply_event(&json!({
            "type": "message.part.updated",
            "properties": {
                "sessionID": "ses_1",
                "part": { "id": "prt_1", "type": "text", "text": "", "messageID": "msg_1" }
            }
        }));
        assert_eq!(snap.parts["msg_1"][0]["text"], "early");
    }

    #[test]
    fn part_updated_skips_duplicate_full_text_after_deltas() {
        let mut snap = SessionSnapshot::empty("ses_1");
        snap.apply_event(&json!({
            "type": "message.part.updated",
            "properties": {
                "sessionID": "ses_1",
                "part": { "id": "prt_1", "type": "text", "text": "", "messageID": "msg_1" }
            }
        }));
        snap.apply_event(&json!({
            "type": "message.part.delta",
            "properties": {
                "sessionID": "ses_1",
                "partID": "prt_1",
                "field": "text",
                "delta": "full message"
            }
        }));
        snap.apply_event(&json!({
            "type": "message.part.updated",
            "properties": {
                "sessionID": "ses_1",
                "part": { "id": "prt_1", "type": "text", "text": "full message", "messageID": "msg_1" }
            }
        }));
        assert_eq!(snap.parts["msg_1"][0]["text"], "full message");
    }

    #[test]
    fn message_updated_upserts_by_id() {
        let mut snap = SessionSnapshot::empty("ses_1");
        snap.apply_event(&json!({
            "type": "message.updated",
            "properties": {
                "sessionID": "ses_1",
                "message": { "id": "msg_1", "role": "assistant", "sessionID": "ses_1" }
            }
        }));
        snap.apply_event(&json!({
            "type": "message.updated",
            "properties": {
                "sessionID": "ses_1",
                "message": { "id": "msg_1", "role": "assistant", "sessionID": "ses_1", "finish": "stop" }
            }
        }));
        assert_eq!(snap.messages.len(), 1);
        assert_eq!(snap.messages[0]["finish"], "stop");
    }

    #[test]
    fn to_value_includes_session_fields() {
        let snap = SessionSnapshot::empty("ses_1");
        let value = snap.to_value();
        assert_eq!(value["sessionId"], "ses_1");
        assert_eq!(value["messages"], json!([]));
        assert_eq!(value["parts"], json!({}));
    }

    #[test]
    fn messages_for_extraction_merges_parts() {
        let mut snap = SessionSnapshot::empty("ses_1");
        snap.messages.push(serde_json::json!({
            "id": "msg_1",
            "info": { "role": "assistant", "id": "msg_1" }
        }));
        snap.parts.insert(
            "msg_1".into(),
            vec![serde_json::json!({
                "type": "text",
                "text": r#"{"status":"done","summary":"From snapshot.","nextStatus":"In Review"}"#
            })],
        );

        let messages = snap.messages_for_extraction();
        let result = crate::sessions::opencode_events::extract_result_from_messages(&messages)
            .expect("extract from snapshot-shaped messages");
        match result {
            crate::providers::AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "From snapshot.");
            }
            _ => panic!("expected done"),
        }
    }
}
