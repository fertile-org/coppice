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
//! - `result`: `modelUsage` is the total with subagents; `usage` is the main
//!   thread only. Since 2.1.309 every result also carries
//!   `model_usage_since_last_result`, which covers this process only; the
//!   reader sums it and prefers it.
//! - On `--resume`, `modelUsage` covers the whole session, earlier processes
//!   included, from 2.1.277 on (checked on 2.0.0 through 2.1.318). Without
//!   `model_usage_since_last_result` (2.1.277–2.1.308) a resumed process's
//!   total cannot be told apart from the session's, so it is not used and the
//!   run falls back to the sum of its rows.
//! - One run can start several processes (connector-check phases). Their
//!   totals are added up; while a later process has not reported its total,
//!   the run falls back to the sum of its rows.

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
    /// `tool_use_id`s whose `task_started` was a `local_agent` task.
    subagent_tasks: HashSet<String>,
    /// Totals of the processes that already ended.
    finished: Option<RunTotalUsage>,
    /// A process reported usage but ended without a total that can be
    /// trusted, so no CLI total covers the run.
    total_unknown: bool,
    process: Process,
}

/// The CLI process currently writing to the run.
#[derive(Default)]
struct Process {
    started: bool,
    resumed: bool,
    /// It sent a request or subagent row.
    reported_rows: bool,
    /// Sum of its `model_usage_since_last_result`.
    since_last: Option<RunTotalUsage>,
    /// Its own total, once a `result` gave one that can be trusted.
    total: Option<RunTotalUsage>,
}

impl UsageReader for ClaudeStreamJsonReader {
    fn start_process(&mut self, resumed: bool) -> Vec<UsageEvent> {
        if self.process.started {
            match self.process.total.take() {
                Some(total) => add_total(&mut self.finished, &total),
                None if self.process.reported_rows => self.total_unknown = true,
                None => {}
            }
        }
        self.open = None;
        self.process = Process {
            started: true,
            resumed,
            ..Process::default()
        };
        Vec::new()
    }

    fn read(&mut self, event: &Value) -> Vec<UsageEvent> {
        self.process.started = true;
        let main_thread = event.get("parent_tool_use_id").is_none_or(Value::is_null);
        let mut events = match event.get("type").and_then(Value::as_str) {
            Some("stream_event") if main_thread => self.stream_event(event),
            Some("assistant") if main_thread => self.assistant(event),
            Some("system") => self.system(event),
            Some("user") => subagent_result(event).into_iter().collect(),
            Some("result") => return self.result(event),
            _ => Vec::new(),
        };
        if !events.is_empty() && !self.process.reported_rows {
            self.process.reported_rows = true;
            // The earlier processes' total no longer covers the run.
            if self.finished.is_some() {
                events.insert(0, UsageEvent::RunTotalUnknown);
            }
        }
        events
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

    /// The run total: earlier processes' totals plus this one's, or
    /// `RunTotalUnknown` when some process has no total that can be trusted.
    fn result(&mut self, event: &Value) -> Vec<UsageEvent> {
        let process_total = if let Some(delta) = event.get("model_usage_since_last_result") {
            let since_last = &mut self.process.since_last;
            add_total(since_last, &model_usage(delta));
            since_last.clone()
        } else if let Some(all) = event.get("modelUsage") {
            // On a resumed session this may include earlier processes.
            (!self.process.resumed).then(|| {
                let mut total = model_usage(all);
                if total.cost_usd.is_none() {
                    total.cost_usd = event.get("total_cost_usd").and_then(Value::as_f64);
                }
                total
            })
        } else {
            return Vec::new();
        };
        // An empty total (`modelUsage: {}`) reports nothing.
        self.process.total = process_total.filter(|total| !total.tokens.is_empty());
        match &self.process.total {
            Some(total) if !self.total_unknown => {
                let mut run_total = self.finished.clone();
                add_total(&mut run_total, total);
                run_total.map(UsageEvent::RunTotal).into_iter().collect()
            }
            _ => vec![UsageEvent::RunTotalUnknown],
        }
    }
}

/// Adds tokens and cost; the cost stays `None` only when neither side has one.
fn add_total(sum: &mut Option<RunTotalUsage>, other: &RunTotalUsage) {
    let sum = sum.get_or_insert_with(RunTotalUsage::default);
    sum.tokens.add(&other.tokens);
    sum.cost_usd = match (sum.cost_usd, other.cost_usd) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
    };
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
        let mut tracker = UsageTracker::new();
        track_processes(
            &mut tracker,
            &mut ClaudeStreamJsonReader::default(),
            false,
            events,
        );
        tracker.snapshot()
    }

    /// One CLI process: `start_process`, then its output.
    fn track_processes(
        tracker: &mut UsageTracker,
        reader: &mut ClaudeStreamJsonReader,
        resumed: bool,
        events: &[Value],
    ) {
        for usage in reader.start_process(resumed) {
            tracker.apply(usage);
        }
        for event in events {
            for usage in reader.read(event) {
                tracker.apply(usage);
            }
        }
    }

    fn assistant(id: &str, output: i64) -> Value {
        json!({ "type": "assistant", "parent_tool_use_id": null,
            "message": { "id": id, "usage": { "input_tokens": 1, "output_tokens": output } } })
    }

    fn result_model_usage(output: i64) -> Value {
        json!({ "type": "result", "modelUsage": { "m": { "inputTokens": 1, "outputTokens": output } } })
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

    #[test]
    fn resumed_process_without_since_last_falls_back_to_its_requests() {
        // 2.1.300 on --resume: one request with 3 output tokens, while
        // `modelUsage` reports 6 (the earlier process's 3 as well).
        let mut tracker = UsageTracker::new();
        let mut reader = ClaudeStreamJsonReader::default();
        let events = fixture("claude-code-2.1.300-usage-resume.jsonl");
        track_processes(&mut tracker, &mut reader, true, &events);
        let totals = tracker.snapshot().totals;
        assert_eq!(totals.status, UsageStatus::Partial);
        assert_eq!(totals.tokens.output, Some(3));
        assert_eq!(totals.request_count, 1);

        // The same output from a fresh session is its own total.
        let mut tracker = UsageTracker::new();
        track_processes(
            &mut tracker,
            &mut ClaudeStreamJsonReader::default(),
            false,
            &events,
        );
        let totals = tracker.snapshot().totals;
        assert_eq!(totals.status, UsageStatus::Reported);
        assert_eq!(totals.tokens.output, Some(6));
    }

    #[test]
    fn resumed_process_with_since_last_uses_it() {
        let mut reader = ClaudeStreamJsonReader::default();
        reader.start_process(true);
        let result = json!({ "type": "result",
            "modelUsage": { "m": { "outputTokens": 6 } },
            "model_usage_since_last_result": { "m": { "outputTokens": 3 } } });
        let UsageEvent::RunTotal(total) = &reader.read(&result)[0] else {
            panic!("total");
        };
        assert_eq!(total.tokens.output, Some(3));
    }

    #[test]
    fn totals_of_several_processes_are_added() {
        // Connector checks run one process per phase on the same run.
        let mut tracker = UsageTracker::new();
        let mut reader = ClaudeStreamJsonReader::default();
        track_processes(
            &mut tracker,
            &mut reader,
            false,
            &[assistant("a1", 4), result_model_usage(10)],
        );
        assert_eq!(tracker.snapshot().totals.tokens.output, Some(10));

        // While the next process runs, the earlier total no longer covers the run.
        track_processes(&mut tracker, &mut reader, false, &[assistant("b1", 2)]);
        let totals = tracker.snapshot().totals;
        assert_eq!(totals.status, UsageStatus::Partial);
        assert_eq!(totals.tokens.output, Some(6));

        track_processes(&mut tracker, &mut reader, false, &[]);
        // `b` ended without a total: no CLI total covers the run any more.
        track_processes(
            &mut tracker,
            &mut reader,
            false,
            &[assistant("c1", 1), result_model_usage(5)],
        );
        let totals = tracker.snapshot().totals;
        assert_eq!(totals.status, UsageStatus::Partial);
        assert_eq!(totals.tokens.output, Some(7));
    }

    #[test]
    fn process_without_rows_keeps_the_earlier_total() {
        let mut tracker = UsageTracker::new();
        let mut reader = ClaudeStreamJsonReader::default();
        track_processes(
            &mut tracker,
            &mut reader,
            false,
            &[assistant("a1", 4), result_model_usage(10)],
        );
        track_processes(&mut tracker, &mut reader, false, &[]);
        let totals = tracker.snapshot().totals;
        assert_eq!(totals.status, UsageStatus::Reported);
        assert_eq!(totals.tokens.output, Some(10));
    }

    #[test]
    fn empty_model_usage_leaves_the_row_sum() {
        let mut tracker = UsageTracker::new();
        let result = json!({ "type": "result", "modelUsage": {} });
        track_processes(
            &mut tracker,
            &mut ClaudeStreamJsonReader::default(),
            false,
            &[assistant("a1", 4), result],
        );
        let totals = tracker.snapshot().totals;
        assert_eq!(totals.status, UsageStatus::Partial);
        assert_eq!(totals.tokens.output, Some(4));
    }
}
