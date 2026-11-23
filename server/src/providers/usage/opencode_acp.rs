//! OpenCode ACP (`opencode acp`).
//!
//! Production input is what [`AcpConsole`](crate::providers::acp_connector::AcpConsole)
//! already observes: unwrapped `session/update` payloads and the
//! `session/prompt` result object (not raw JSON-RPC stdout).
//!
//! What the stream carries (checked on 2.0.20):
//! - Prompt result `usage`: one object per prompt with `inputTokens`,
//!   `outputTokens`, `cachedReadTokens`, optional `thoughtTokens`, and
//!   `totalTokens` (context fill — not a run total).
//! - `inputTokens` excludes cache; do not subtract `cachedReadTokens`.
//! - `thoughtTokens` sit outside `outputTokens`; add them into Coppice
//!   `output` and map them to `reasoning`.
//! - `sessionUpdate: usage_update`: context window `used` / `size`.
//! - Resume emits only this process's prompt; no resume subtraction.
//! - Subagents carry `_meta.opencode/child-session` but no token block.

use super::{ProcessContext, UsageReader};
use crate::domain::usage::{
    ContextWindowUsage, Granularity, RequestUsage, RunTotalUsage, TokenCounts, UsageEvent,
};
use serde_json::Value;

#[derive(Default)]
pub struct OpenCodeAcpReader {
    /// Totals of the processes that already ended.
    finished: Option<RunTotalUsage>,
    /// Turn index across every process on this run, so ids stay unique.
    turns: u32,
    process: Process,
}

#[derive(Default)]
struct Process {
    started: bool,
    total: Option<RunTotalUsage>,
}

impl UsageReader for OpenCodeAcpReader {
    fn start_process(&mut self, _process: &ProcessContext) -> Vec<UsageEvent> {
        if self.process.started {
            if let Some(total) = self.process.total.take() {
                add_total(&mut self.finished, &total);
            }
        }
        self.process = Process {
            started: true,
            total: None,
        };
        Vec::new()
    }

    fn read(&mut self, event: &Value) -> Vec<UsageEvent> {
        self.process.started = true;
        if event.get("sessionUpdate").and_then(Value::as_str) == Some("usage_update") {
            return usage_update(event).into_iter().collect();
        }
        if event.get("usage").is_some() {
            return self.prompt_usage(event);
        }
        Vec::new()
    }
}

impl OpenCodeAcpReader {
    fn prompt_usage(&mut self, event: &Value) -> Vec<UsageEvent> {
        let Some(usage) = event.get("usage") else {
            return Vec::new();
        };
        let tokens = prompt_tokens(usage);
        if tokens.is_empty() {
            return Vec::new();
        }
        self.turns += 1;
        let turn = RequestUsage {
            id: Some(format!("turn-{}", self.turns)),
            granularity: Granularity::Turn,
            model: None,
            tokens,
        };
        let process_total = RunTotalUsage {
            tokens: turn.tokens,
            cost_usd: None,
            by_model: Vec::new(),
        };
        add_total(&mut self.process.total, &process_total);
        let mut run_total = self.finished.clone();
        add_total(&mut run_total, self.process.total.as_ref().unwrap());
        vec![
            UsageEvent::Request(turn),
            UsageEvent::RunTotal(run_total.expect("process total was just set")),
        ]
    }
}

/// Converts OpenCode ACP prompt usage into Coppice [`TokenCounts`].
fn prompt_tokens(usage: &Value) -> TokenCounts {
    let thought = int(usage, "thoughtTokens");
    let output_only = int(usage, "outputTokens");
    TokenCounts {
        input: int(usage, "inputTokens"),
        cache_read: int(usage, "cachedReadTokens"),
        cache_write: None,
        output: match (output_only, thought) {
            (Some(output), Some(thought)) => Some(output + thought),
            (Some(output), None) => Some(output),
            (None, Some(thought)) => Some(thought),
            (None, None) => None,
        },
        reasoning: thought,
    }
}

fn usage_update(event: &Value) -> Option<UsageEvent> {
    let used = int(event, "used")?;
    Some(UsageEvent::ContextWindow(ContextWindowUsage {
        used,
        size: int(event, "size"),
    }))
}

fn add_total(sum: &mut Option<RunTotalUsage>, other: &RunTotalUsage) {
    let sum = sum.get_or_insert_with(RunTotalUsage::default);
    sum.tokens.add(&other.tokens);
}

fn int(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::usage::{UsageStatus, UsageTracker};
    use serde_json::json;

    fn track(events: &[Value]) -> crate::domain::usage::UsageSnapshot {
        let mut tracker = UsageTracker::new();
        let mut reader = OpenCodeAcpReader::default();
        let process = ProcessContext::default();
        for usage in reader.start_process(&process) {
            tracker.apply(usage);
        }
        for event in events {
            for usage in reader.read(event) {
                tracker.apply(usage);
            }
        }
        tracker.snapshot()
    }

    #[test]
    fn thought_tokens_are_added_into_output() {
        let mut reader = OpenCodeAcpReader::default();
        let events = reader.read(&json!({
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 58,
                "outputTokens": 14,
                "thoughtTokens": 17,
                "cachedReadTokens": 7808,
                "totalTokens": 7897
            }
        }));
        let UsageEvent::Request(req) = &events[0] else {
            panic!("request");
        };
        assert_eq!(req.tokens.input, Some(58));
        assert_eq!(req.tokens.cache_read, Some(7808));
        assert_eq!(req.tokens.output, Some(31));
        assert_eq!(req.tokens.reasoning, Some(17));
        assert_eq!(req.tokens.cache_write, None);
    }

    #[test]
    fn input_does_not_subtract_cache() {
        let mut reader = OpenCodeAcpReader::default();
        let events = reader.read(&json!({
            "usage": {
                "inputTokens": 230,
                "outputTokens": 4,
                "cachedReadTokens": 8704,
                "totalTokens": 8938
            }
        }));
        let UsageEvent::Request(req) = &events[0] else {
            panic!("request");
        };
        assert_eq!(req.tokens.input, Some(230));
        assert_eq!(req.tokens.cache_read, Some(8704));
        assert_eq!(req.tokens.output, Some(4));
        assert_eq!(req.tokens.reasoning, None);
    }

    #[test]
    fn usage_update_is_context_window() {
        let mut reader = OpenCodeAcpReader::default();
        let events = reader.read(&json!({
            "sessionUpdate": "usage_update",
            "used": 8938,
            "size": 200000
        }));
        assert_eq!(
            events,
            vec![UsageEvent::ContextWindow(ContextWindowUsage {
                used: 8938,
                size: Some(200000),
            })]
        );
    }

    #[test]
    fn prompt_rows_sum_to_a_reported_total() {
        let snap = track(&[json!({
            "usage": {
                "inputTokens": 10,
                "outputTokens": 2,
                "cachedReadTokens": 5
            }
        })]);
        assert_eq!(snap.totals.status, UsageStatus::Reported);
        assert_eq!(snap.totals.tokens.input, Some(10));
        assert_eq!(snap.totals.tokens.output, Some(2));
        assert!(snap.subagents.is_empty());
    }
}
