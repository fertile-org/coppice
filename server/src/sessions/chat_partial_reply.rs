use crate::sessions::live_message::LiveMessage;
use crate::sessions::run_registry::RunStreamHandle;
use serde_json::Value;

/// Visible reply text from the live stream sources the chat UI uses
/// (console `.console.text` / result, then OpenCode snapshot assistant text).
pub fn partial_chat_reply(handle: &RunStreamHandle) -> String {
    let from_console = reply_from_live_buffer(&handle.buffered_tail());
    if !from_console.is_empty() {
        return from_console;
    }
    handle
        .snapshot()
        .as_ref()
        .map(reply_from_opencode_snapshot)
        .unwrap_or_default()
}

fn reply_from_live_buffer(messages: &[LiveMessage]) -> String {
    let mut thinking = Vec::new();
    let mut post_result = Vec::new();
    let mut result_summary = String::new();
    let mut seen_result = false;

    for message in messages {
        let LiveMessage::Event { event } = message else {
            continue;
        };
        let Some(ty) = event.get("type").and_then(|v| v.as_str()) else {
            continue;
        };
        if !ty.contains(".console.") {
            continue;
        }
        if ty.ends_with(".console.thinking") {
            if let Some(text) = event.get("text").and_then(|v| v.as_str()) {
                let text = text.trim();
                if !text.is_empty() {
                    thinking.push(text.to_string());
                }
            }
        } else if ty.ends_with(".console.text") {
            if let Some(markdown) = event.get("markdown").and_then(|v| v.as_str()) {
                let markdown = markdown.trim();
                if markdown.is_empty() {
                    continue;
                }
                if seen_result {
                    post_result.push(markdown.to_string());
                } else {
                    thinking.push(markdown.to_string());
                }
            }
        } else if ty.ends_with(".console.result") {
            seen_result = true;
            if let Some(summary) = result_summary_from_event(event) {
                result_summary = summary;
            }
        } else if ty.ends_with(".console.continued") {
            seen_result = true;
            if result_summary.is_empty() {
                if let Some(summary) = event
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    result_summary = summary.to_string();
                }
            }
        }
    }

    let reply = if !result_summary.is_empty() {
        result_summary
    } else if !post_result.is_empty() {
        post_result.join("\n\n")
    } else {
        thinking.join("\n\n")
    };
    reply.trim().to_string()
}

fn result_summary_from_event(event: &Value) -> Option<String> {
    event
        .get("contract")
        .and_then(|c| c.get("summary"))
        .and_then(|s| s.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn reply_from_opencode_snapshot(snapshot: &Value) -> String {
    let Some(messages) = snapshot.get("messages").and_then(|v| v.as_array()) else {
        return String::new();
    };
    let parts_map = snapshot.get("parts").and_then(|v| v.as_object());
    let mut thinking = Vec::new();
    let mut reply = Vec::new();

    for message in messages {
        let role = message
            .get("info")
            .and_then(|info| info.get("role"))
            .or_else(|| message.get("role"))
            .and_then(|v| v.as_str());
        if role != Some("assistant") {
            continue;
        }
        let message_id = message
            .get("id")
            .or_else(|| message.pointer("/info/id"))
            .and_then(|v| v.as_str());
        let Some(message_id) = message_id else {
            continue;
        };
        let Some(parts) = parts_map
            .and_then(|map| map.get(message_id))
            .and_then(|v| v.as_array())
        else {
            continue;
        };
        for part in parts {
            let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let text = part
                .get("text")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let Some(text) = text else {
                continue;
            };
            if part_type == "reasoning" {
                thinking.push(text.to_string());
            } else if part_type == "text" {
                reply.push(text.to_string());
            }
        }
    }

    if !reply.is_empty() {
        reply.join("\n\n")
    } else {
        thinking.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Id;
    use crate::sessions::run_registry::RunStreamRegistry;
    use serde_json::json;

    #[test]
    fn console_text_before_result_is_kept_as_partial() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(Id::new());
        handle.publish(LiveMessage::Event {
            event: json!({
                "type": "claude-code.console.text",
                "markdown": "Half a reply…"
            }),
        });
        assert_eq!(partial_chat_reply(&handle), "Half a reply…");
    }

    #[test]
    fn opencode_snapshot_assistant_text_is_used_when_no_console() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(Id::new());
        handle.set_snapshot(json!({
            "sessionId": "ses_1",
            "messages": [{ "id": "msg_1", "info": { "id": "msg_1", "role": "assistant" } }],
            "parts": {
                "msg_1": [{ "type": "text", "text": "From the snapshot." }]
            }
        }));
        assert_eq!(partial_chat_reply(&handle), "From the snapshot.");
    }
}
