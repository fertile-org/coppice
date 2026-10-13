//! The model string a connector's own output carried.
//!
//! The agent's saved picker value is never a source. A missing or blank field
//! is not a report.

use serde_json::Value;

/// Model identity from one CLI event, in the form that connector's live model
/// list uses as an id. `None` when this event does not carry a model.
pub fn reported_model(connector_id: &str, event: &Value) -> Option<String> {
    match connector_id {
        "cursor" => system_init_model(event),
        "claude-code" => system_init_model(event).or_else(|| claude_message_model(event)),
        "codex" => {
            if event.get("type").and_then(Value::as_str) == Some("thread.started") {
                nonempty_string(event, "model")
            } else {
                None
            }
        }
        "kilo-code" => kilo_model(event),
        "opencode" => opencode_model(event),
        _ => None,
    }
}

/// First model any event in `events` reports. Later events do not replace it.
pub fn first_reported_model<'a>(
    connector_id: &str,
    events: impl IntoIterator<Item = &'a Value>,
) -> Option<String> {
    events
        .into_iter()
        .find_map(|event| reported_model(connector_id, event))
}

fn system_init_model(event: &Value) -> Option<String> {
    if event.get("type").and_then(Value::as_str) != Some("system") {
        return None;
    }
    if event.get("subtype").and_then(Value::as_str) != Some("init") {
        return None;
    }
    nonempty_string(event, "model")
}

fn claude_message_model(event: &Value) -> Option<String> {
    if event.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    event
        .get("message")
        .and_then(|message| nonempty_string(message, "model"))
}

fn kilo_model(event: &Value) -> Option<String> {
    let (provider, model) = provider_and_model(event)?;
    Some(match provider {
        Some(provider) if !model.contains('/') => format!("{provider}/{model}"),
        _ => model,
    })
}

fn opencode_model(event: &Value) -> Option<String> {
    provider_and_model(event).map(|(_provider, model)| model)
}

/// `(providerID, model id)` from the shapes OpenCode and Kilo emit.
fn provider_and_model(event: &Value) -> Option<(Option<String>, String)> {
    const PATHS: &[&[&str]] = &[
        &["properties", "message", "info"],
        &["properties", "info"],
        &["properties", "message"],
        &["info"],
        &[],
    ];
    for path in PATHS {
        let Some(node) = dig(event, path) else {
            continue;
        };
        if let Some(pair) = identity_at(node) {
            return Some(pair);
        }
    }
    None
}

fn dig<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn identity_at(node: &Value) -> Option<(Option<String>, String)> {
    if let Some(model) = node.get("model").filter(|value| value.is_object()) {
        if let Some(pair) = identity_at(model) {
            return Some(pair);
        }
    }
    let provider = nonempty_string(node, "providerID");
    let model = nonempty_string(node, "modelID").or_else(|| {
        // A bare `model` string. An object was handled above.
        node.get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    });
    model.map(|model| (provider, model))
}

fn nonempty_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn fixtures(connector: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures")
            .join(connector)
    }

    fn events_in(path: PathBuf) -> Vec<Value> {
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path:?}: {err}"));
        raw.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap_or_else(|err| panic!("{line}: {err}")))
            .collect()
    }

    #[test]
    fn cursor_fixture_reports_the_init_model_verbatim() {
        let events = events_in(fixtures("cursor").join("done.jsonl"));
        assert_eq!(
            first_reported_model("cursor", &events).as_deref(),
            Some("composer-2.5")
        );
    }

    #[test]
    fn cursor_blank_or_missing_model_is_not_a_report() {
        assert_eq!(
            reported_model(
                "cursor",
                &json!({"type":"system","subtype":"init","model":"  "})
            ),
            None
        );
        assert_eq!(
            reported_model("cursor", &json!({"type":"system","subtype":"init"})),
            None
        );
        assert_eq!(
            reported_model(
                "cursor",
                &json!({"type":"result","subtype":"success","result":"done"})
            ),
            None
        );
    }

    #[test]
    fn claude_fixture_reports_the_init_model_verbatim() {
        let events = events_in(fixtures("claude-code").join("done.jsonl"));
        assert_eq!(
            first_reported_model("claude-code", &events).as_deref(),
            Some("claude-sonnet-4-20250514")
        );
    }

    #[test]
    fn claude_assistant_message_model_counts_when_init_has_none() {
        let model = reported_model(
            "claude-code",
            &json!({
                "type": "assistant",
                "message": {"role": "assistant", "model": "claude-opus-4-8", "content": []}
            }),
        );
        assert_eq!(model.as_deref(), Some("claude-opus-4-8"));
    }

    #[test]
    fn codex_fixture_does_not_report_a_model() {
        let events = events_in(fixtures("codex").join("done.jsonl"));
        assert_eq!(first_reported_model("codex", &events), None);
    }

    #[test]
    fn codex_thread_started_model_is_verbatim() {
        let model = reported_model(
            "codex",
            &json!({"type":"thread.started","thread_id":"thr_1","model":"gpt-5.4"}),
        );
        assert_eq!(model.as_deref(), Some("gpt-5.4"));
        assert_eq!(
            reported_model(
                "codex",
                &json!({"type":"thread.started","thread_id":"thr_1"})
            ),
            None
        );
    }

    #[test]
    fn kilo_fixture_does_not_report_a_model() {
        let events = events_in(fixtures("kilo-code").join("done.jsonl"));
        assert_eq!(first_reported_model("kilo-code", &events), None);
    }

    #[test]
    fn kilo_joins_provider_and_model_id_to_match_the_live_list() {
        let model = reported_model(
            "kilo-code",
            &json!({
                "type": "session.message",
                "properties": {
                    "message": {
                        "info": {
                            "role": "assistant",
                            "providerID": "anthropic",
                            "modelID": "claude-sonnet-4-20250514"
                        }
                    }
                }
            }),
        );
        assert_eq!(model.as_deref(), Some("anthropic/claude-sonnet-4-20250514"));
    }

    #[test]
    fn kilo_keeps_a_model_id_that_already_includes_the_provider() {
        let model = reported_model(
            "kilo-code",
            &json!({
                "type": "session.message",
                "properties": {
                    "message": {
                        "info": {
                            "providerID": "anthropic",
                            "modelID": "anthropic/claude-sonnet-4-20250514"
                        }
                    }
                }
            }),
        );
        assert_eq!(model.as_deref(), Some("anthropic/claude-sonnet-4-20250514"));
    }

    #[test]
    fn opencode_fixture_does_not_report_a_model() {
        let events = events_in(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../fixtures/opencode-events/sample.jsonl"),
        );
        assert_eq!(first_reported_model("opencode", &events), None);
    }

    #[test]
    fn opencode_reports_model_id_without_the_provider_prefix() {
        let from_event = reported_model(
            "opencode",
            &json!({
                "type": "message.updated",
                "properties": {
                    "info": {
                        "role": "assistant",
                        "providerID": "zai-coding-plan",
                        "modelID": "glm-5.1"
                    }
                }
            }),
        );
        assert_eq!(from_event.as_deref(), Some("glm-5.1"));

        let from_message = reported_model(
            "opencode",
            &json!({
                "info": {"role": "assistant", "providerID": "openai", "modelID": "gpt-5"},
                "parts": []
            }),
        );
        assert_eq!(from_message.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn first_event_wins_when_a_later_event_names_another_model() {
        let events = vec![
            json!({"type":"system","subtype":"init","model":"composer-2.5"}),
            json!({"type":"system","subtype":"init","model":"gpt-5"}),
        ];
        assert_eq!(
            first_reported_model("cursor", &events).as_deref(),
            Some("composer-2.5")
        );
    }
}
