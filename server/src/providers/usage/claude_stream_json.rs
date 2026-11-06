//! Claude Code `--output-format stream-json --verbose --include-partial-messages`.
//!
//! What the stream carries (checked on 2.1.318):
//! - `stream_event` → `message_start` / `message_delta`: each main-thread
//!   request's usage; `message_delta` holds the final output count.
//! - `assistant`: repeats the request's usage once per content block, with the
//!   output count from `message_start`. Used only when no `stream_event`
//!   arrived (flag missing).
//! - Subagent requests are not streamed. `system/task_started` names the
//!   subagent; its totals arrive in the Agent tool result
//!   (`tool_use_result.usage`) and `system/task_notification`.
//! - Only `task_type == "local_agent"` tasks are subagents. Background Bash
//!   tasks emit the same `task_started` / `task_notification` events with
//!   another `task_type`; they get no row, and a notification for an id that
//!   never started as a subagent is dropped.
//! - The Agent tool result's `usage` covers only the subagent's *last* request
//!   (checked on a two-request subagent: its cache writes were 2432 there and
//!   19968 in `modelUsage`). It is not kept as a breakdown, so a partial sum
//!   cannot count it as the subagent's usage; the subagent keeps
//!   `total_tokens` only. An async launch (`status: async_launched`) carries
//!   no usage and does not set the status.
//! - `result`: `modelUsage` is the run total with subagents; `usage` is the
//!   main thread only. A session with several results also carries
//!   `model_usage_since_last_result`, which is summed.

use super::UsageReader;
use crate::domain::usage::{
    Granularity, RequestUsage, RunTotalUsage, SubagentUsage, TokenCounts, UsageEvent,
};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Default)]
pub struct ClaudeStreamJsonReader {
    /// Main-thread request whose `message_delta` is still to come.
    open: Option<String>,
    /// Requests seen through `stream_event`; their `assistant` copies are skipped.
    streamed: HashSet<String>,
    /// Sum of `model_usage_since_last_result` across results.
    since_last: Option<RunTotalUsage>,
    /// `tool_use_id`s whose `task_started` was a `local_agent` task.
    subagent_tasks: HashSet<String>,
}

impl UsageReader for ClaudeStreamJsonReader {
    fn read(&mut self, event: &Value) -> Vec<UsageEvent> {
        let main_thread = event.get("parent_tool_use_id").is_none_or(Value::is_null);
        match event.get("type").and_then(Value::as_str) {
            Some("stream_event") if main_thread => self.stream_event(event),
            Some("assistant") if main_thread => self.assistant(event),
            Some("system") => self.system(event),
            Some("user") => subagent_result(event).into_iter().collect(),
            Some("result") => self.result(event),
            _ => Vec::new(),
        }
    }
}

impl ClaudeStreamJsonReader {
    fn stream_event(&mut self, event: &Value) -> Vec<UsageEvent> {
        let Some(inner) = event.get("event") else {
            return Vec::new();
        };
        match inner.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                let Some(message) = inner.get("message") else {
                    return Vec::new();
                };
                let Some(id) = str_field(message, "id") else {
                    return Vec::new();
                };
                self.streamed.insert(id.clone());
                self.open = Some(id.clone());
                vec![request(
                    id,
                    str_field(message, "model"),
                    message.get("usage").map(api_usage).unwrap_or_default(),
                )]
            }
            Some("message_delta") => {
                let (Some(id), Some(usage)) = (self.open.take(), inner.get("usage")) else {
                    return Vec::new();
                };
                vec![request(id, None, api_usage(usage))]
            }
            _ => Vec::new(),
        }
    }

    fn assistant(&mut self, event: &Value) -> Vec<UsageEvent> {
        let Some(message) = event.get("message") else {
            return Vec::new();
        };
        let Some(id) = str_field(message, "id") else {
            return Vec::new();
        };
        if self.streamed.contains(&id) {
            return Vec::new();
        }
        let Some(usage) = message.get("usage") else {
            return Vec::new();
        };
        vec![request(id, str_field(message, "model"), api_usage(usage))]
    }

    /// `task_started` opens a subagent row for `local_agent` tasks only;
    /// `task_notification` adds the total of a task that was opened.
    fn system(&mut self, event: &Value) -> Vec<UsageEvent> {
        let Some(id) = str_field(event, "tool_use_id") else {
            return Vec::new();
        };
        let subagent = match event.get("subtype").and_then(Value::as_str) {
            Some("task_started") => {
                if str_field(event, "task_type").as_deref() != Some("local_agent") {
                    return Vec::new();
                }
                self.subagent_tasks.insert(id.clone());
                SubagentUsage {
                    id,
                    agent_type: str_field(event, "subagent_type"),
                    description: str_field(event, "description"),
                    status: Some("running".into()),
                    ..SubagentUsage::default()
                }
            }
            Some("task_notification") => {
                if !self.subagent_tasks.contains(&id) {
                    return Vec::new();
                }
                let usage = event.get("usage");
                SubagentUsage {
                    id,
                    status: str_field(event, "status"),
                    total_tokens: usage
                        .and_then(|u| u.get("total_tokens"))
                        .and_then(Value::as_i64),
                    tool_uses: usage
                        .and_then(|u| u.get("tool_uses"))
                        .and_then(Value::as_i64),
                    duration_ms: usage
                        .and_then(|u| u.get("duration_ms"))
                        .and_then(Value::as_i64),
                    ..SubagentUsage::default()
                }
            }
            _ => return Vec::new(),
        };
        vec![UsageEvent::Subagent(subagent)]
    }

    fn result(&mut self, event: &Value) -> Vec<UsageEvent> {
        if let Some(delta) = event.get("model_usage_since_last_result") {
            let delta = model_usage(delta);
            let total = self.since_last.get_or_insert_with(RunTotalUsage::default);
            total.tokens.add(&delta.tokens);
            total.cost_usd = match (total.cost_usd, delta.cost_usd) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
            };
            return vec![UsageEvent::RunTotal(total.clone())];
        }
        if let Some(all) = event.get("modelUsage") {
            let mut total = model_usage(all);
            if total.cost_usd.is_none() {
                total.cost_usd = event.get("total_cost_usd").and_then(Value::as_f64);
            }
            return vec![UsageEvent::RunTotal(total)];
        }
        Vec::new()
    }
}

fn request(id: String, model: Option<String>, tokens: TokenCounts) -> UsageEvent {
    UsageEvent::Request(RequestUsage {
        id: Some(id),
        granularity: Granularity::Request,
        model,
        tokens,
    })
}

/// The Agent tool's result: `tool_use_result` with `agentId` and `usage`.
fn subagent_result(event: &Value) -> Option<UsageEvent> {
    let result = event.get("tool_use_result")?;
    result.get("agentId")?;
    let id = event
        .pointer("/message/content")
        .and_then(Value::as_array)?
        .iter()
        .find_map(|block| str_field(block, "tool_use_id"))?;
    Some(UsageEvent::Subagent(SubagentUsage {
        id,
        agent_type: str_field(result, "agentType"),
        description: None,
        model: str_field(result, "resolvedModel"),
        status: str_field(result, "status").filter(|status| status != "async_launched"),
        tokens: TokenCounts::default(),
        total_tokens: result.get("totalTokens").and_then(Value::as_i64),
        tool_uses: result.get("totalToolUseCount").and_then(Value::as_i64),
        duration_ms: result.get("totalDurationMs").and_then(Value::as_i64),
    }))
}

/// Anthropic API usage (`input_tokens`, `cache_read_input_tokens`, ...).
fn api_usage(usage: &Value) -> TokenCounts {
    TokenCounts {
        input: int(usage, "input_tokens"),
        cache_read: int(usage, "cache_read_input_tokens"),
        cache_write: int(usage, "cache_creation_input_tokens"),
        output: int(usage, "output_tokens"),
        reasoning: usage
            .pointer("/output_tokens_details/thinking_tokens")
            .and_then(Value::as_i64),
    }
}

/// Claude Code `modelUsage`: per-model camelCase totals, summed over models.
fn model_usage(by_model: &Value) -> RunTotalUsage {
    let mut total = RunTotalUsage::default();
    let Some(models) = by_model.as_object() else {
        return total;
    };
    for usage in models.values() {
        total.tokens.add(&TokenCounts {
            input: int(usage, "inputTokens"),
            cache_read: int(usage, "cacheReadInputTokens"),
            cache_write: int(usage, "cacheCreationInputTokens"),
            output: int(usage, "outputTokens"),
            reasoning: int(usage, "thinkingTokens"),
        });
        if let Some(cost) = usage.get("costUSD").and_then(Value::as_f64) {
            total.cost_usd = Some(total.cost_usd.unwrap_or(0.0) + cost);
        }
    }
    total
}

fn int(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::usage::{UsageStatus, UsageTracker};
    use serde_json::json;

    fn fixture(name: &str) -> Vec<Value> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/claude-code")
            .join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()))
            .lines()
            .map(|line| serde_json::from_str(line).expect("json line"))
            .collect()
    }

    fn track(events: &[Value]) -> crate::domain::usage::UsageSnapshot {
        let mut reader = ClaudeStreamJsonReader::default();
        let mut tracker = UsageTracker::new();
        for event in events {
            for usage in reader.read(event) {
                tracker.apply(usage);
            }
        }
        tracker.snapshot().expect("usage")
    }

    #[test]
    fn partial_messages_give_exact_output_per_request() {
        let snap = track(&fixture("claude-code-2.1.318-usage-partial.jsonl"));
        let outputs: Vec<_> = snap.requests.iter().map(|r| r.tokens.output).collect();
        // message_delta values, not the 16 and 5 that message_start and the
        // assistant events carry.
        assert_eq!(outputs, [Some(77), Some(25)]);
        assert_eq!(snap.requests[0].model.as_deref(), Some("claude-haiku-5-5"));
        assert_eq!(snap.requests[0].tokens.cache_read, Some(14296));
        assert_eq!(snap.requests[0].tokens.cache_write, Some(6621));
        assert_eq!(snap.requests[0].tokens.input, Some(2));
        assert_eq!(snap.totals.status, UsageStatus::Reported);
        assert_eq!(snap.totals.tokens.output, Some(102));
        assert!(snap.subagents.is_empty());
    }

    #[test]
    fn subagent_run_counts_subagent_and_dedupes_requests() {
        let snap = track(&fixture("claude-code-2.1.318-usage-subagent.jsonl"));
        // Two main-thread requests, each repeated across content blocks.
        assert_eq!(snap.requests.len(), 2);
        assert_eq!(snap.subagents.len(), 1);
        let sub = &snap.subagents[0];
        assert_eq!(sub.id, "toolu_01NTxmpEGSyCizLGv3JgMNA5");
        assert_eq!(sub.agent_type.as_deref(), Some("general-purpose"));
        assert_eq!(sub.description.as_deref(), Some("Reply with number 4"));
        assert_eq!(sub.model.as_deref(), Some("claude-haiku-5-5"));
        assert_eq!(sub.status.as_deref(), Some("completed"));
        assert!(sub.tokens.is_empty());
        assert_eq!(sub.total_tokens, Some(12847));
        assert_eq!(sub.total(), Some(12847));
        // modelUsage: main thread plus subagent.
        let totals = &snap.totals;
        assert_eq!(totals.status, UsageStatus::Reported);
        assert_eq!(totals.tokens.output, Some(353));
        assert_eq!(totals.tokens.cache_write, Some(22493));
        assert_eq!(totals.tokens.cache_read, Some(30191));
        assert_eq!(totals.tokens.reasoning, Some(137));
        assert_eq!(totals.cost_usd, Some(0.00401926));
        assert_eq!(totals.subagent_count, 1);
    }

    #[test]
    fn background_bash_task_is_not_a_subagent() {
        let snap = track(&fixture("claude-code-2.1.318-usage-bg-task.jsonl"));
        // The capture has a `local_bash` task and a `local_agent` task.
        assert_eq!(snap.subagents.len(), 1);
        assert_eq!(snap.totals.subagent_count, 1);
        let sub = &snap.subagents[0];
        assert_eq!(sub.id, "toolu_01AwBcR53NG3e8pJxHz96nWm");
        assert_eq!(sub.agent_type.as_deref(), Some("general-purpose"));
        assert_eq!(sub.status.as_deref(), Some("completed"));
        assert_eq!(sub.total_tokens, Some(18594));
        assert_eq!(sub.tool_uses, Some(2));
        assert!(sub.tokens.is_empty());
        assert_eq!(snap.totals.status, UsageStatus::Reported);
    }

    #[test]
    fn notification_for_an_unknown_task_is_dropped() {
        let mut reader = ClaudeStreamJsonReader::default();
        let started = |task_type: &str| {
            json!({ "type": "system", "subtype": "task_started", "tool_use_id": "t1",
                "task_type": task_type })
        };
        let done = json!({ "type": "system", "subtype": "task_notification",
            "tool_use_id": "t1", "status": "completed" });
        assert!(reader.read(&started("local_bash")).is_empty());
        assert!(reader.read(&done).is_empty());
        assert_eq!(reader.read(&started("local_agent")).len(), 1);
        assert_eq!(reader.read(&done).len(), 1);
    }

    #[test]
    fn subagent_result_usage_is_not_a_breakdown() {
        // Two requests; `tool_use_result.usage` is only the second one.
        let snap = track(&fixture(
            "claude-code-2.1.318-usage-subagent-requests.jsonl",
        ));
        assert_eq!(snap.subagents.len(), 1);
        let sub = &snap.subagents[0];
        assert_eq!(sub.tool_uses, Some(2));
        assert!(sub.tokens.is_empty());
        assert!(sub.total_tokens.is_some());
        assert_eq!(sub.model.as_deref(), Some("claude-haiku-5-5"));
        // The run total still includes everything the subagent used.
        assert_eq!(snap.totals.tokens.cache_write, Some(33005));
    }

    #[test]
    fn assistant_usage_is_kept_when_nothing_was_streamed() {
        let mut reader = ClaudeStreamJsonReader::default();
        let event = json!({ "type": "assistant", "parent_tool_use_id": null,
            "message": { "id": "m1", "model": "x", "usage": { "input_tokens": 3, "output_tokens": 4 } } });
        let first = reader.read(&event);
        assert_eq!(first.len(), 1);
        // A repeat for another content block updates the same id.
        let UsageEvent::Request(req) = &reader.read(&event)[0] else {
            panic!("request");
        };
        assert_eq!(req.id.as_deref(), Some("m1"));
    }

    #[test]
    fn subagent_stream_events_are_left_to_the_subagent_total() {
        let mut reader = ClaudeStreamJsonReader::default();
        let event = json!({ "type": "stream_event", "parent_tool_use_id": "toolu_x",
            "event": { "type": "message_start", "message": { "id": "m2", "usage": { "output_tokens": 1 } } } });
        assert!(reader.read(&event).is_empty());
    }

    #[test]
    fn results_since_last_are_summed() {
        let mut reader = ClaudeStreamJsonReader::default();
        let result = |out: i64| {
            json!({ "type": "result",
                "modelUsage": { "m": { "outputTokens": 999 } },
                "model_usage_since_last_result": { "m": { "outputTokens": out, "costUSD": 0.25 } } })
        };
        reader.read(&result(10));
        let UsageEvent::RunTotal(total) = &reader.read(&result(5))[0] else {
            panic!("total");
        };
        assert_eq!(total.tokens.output, Some(15));
        assert_eq!(total.cost_usd, Some(0.5));
    }
}
