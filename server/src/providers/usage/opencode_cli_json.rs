//! OpenCode `opencode run --format json`.
//!
//! Production input is raw JSON lines from [`cli_runner`](crate::providers::cli_runner).
//!
//! What the stream carries (checked on 2.0.20):
//! - `step_finish.part.tokens`: `input`, `output`, `reasoning`,
//!   `cache.read` / `cache.write`. `input` does not include cache reads.
//! - `reasoning` sits outside `output`; add it into Coppice `output`.
//! - Granularity is one LLM step ending in `step_finish`. A trailing `text`
//!   without a following `step_finish` means the capture is incomplete —
//!   emit Request rows only (no reported `RunTotal`), so the run stays
//!   `partial`.
//! - Resume emits only this process's steps; no resume subtraction.
//! - No model, context window, or subagent tokens on this transport.

use super::{ProcessContext, UsageReader};
use crate::domain::usage::{Granularity, RequestUsage, TokenCounts, UsageEvent};
use serde_json::Value;

#[derive(Default)]
pub struct OpenCodeCliJsonReader {
    /// Turn index across every process on this run, so ids stay unique.
    turns: u32,
}

impl UsageReader for OpenCodeCliJsonReader {
    fn start_process(&mut self, _process: &ProcessContext) -> Vec<UsageEvent> {
        Vec::new()
    }

    fn read(&mut self, event: &Value) -> Vec<UsageEvent> {
        match event.get("type").and_then(Value::as_str) {
            Some("step_finish") => self.step_finish(event),
            _ => Vec::new(),
        }
    }
}

impl OpenCodeCliJsonReader {
    fn step_finish(&mut self, event: &Value) -> Vec<UsageEvent> {
        let Some(tokens_obj) = event.pointer("/part/tokens") else {
            return Vec::new();
        };
        let tokens = step_tokens(tokens_obj);
        if tokens.is_empty() {
            return Vec::new();
        }
        self.turns += 1;
        vec![UsageEvent::Request(RequestUsage {
            id: Some(format!("turn-{}", self.turns)),
            granularity: Granularity::Turn,
            model: None,
            tokens,
        })]
    }
}

/// Converts OpenCode CLI step tokens into Coppice [`TokenCounts`].
fn step_tokens(tokens: &Value) -> TokenCounts {
    let reasoning = int(tokens, "reasoning");
    let output_only = int(tokens, "output");
    TokenCounts {
        input: int(tokens, "input"),
        cache_read: tokens.pointer("/cache/read").and_then(Value::as_i64),
        cache_write: tokens.pointer("/cache/write").and_then(Value::as_i64),
        output: match (output_only, reasoning) {
            (Some(output), Some(reasoning)) => Some(output + reasoning),
            (Some(output), None) => Some(output),
            (None, Some(reasoning)) => Some(reasoning),
            (None, None) => None,
        },
        reasoning,
    }
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
        let mut reader = OpenCodeCliJsonReader::default();
        for usage in reader.start_process(&ProcessContext::default()) {
            tracker.apply(usage);
        }
        for event in events {
            for usage in reader.read(event) {
                tracker.apply(usage);
            }
        }
        tracker.snapshot()
    }

    fn step(input: i64, output: i64, reasoning: i64, cache_read: i64, cache_write: i64) -> Value {
        json!({
            "type": "step_finish",
            "part": {
                "type": "step-finish",
                "tokens": {
                    "input": input,
                    "output": output,
                    "reasoning": reasoning,
                    "cache": { "read": cache_read, "write": cache_write }
                }
            }
        })
    }

    #[test]
    fn reasoning_is_added_into_output() {
        let mut reader = OpenCodeCliJsonReader::default();
        let events = reader.read(&step(8601, 62, 128, 768, 0));
        let UsageEvent::Request(req) = &events[0] else {
            panic!("request");
        };
        assert_eq!(req.tokens.input, Some(8601));
        assert_eq!(req.tokens.cache_read, Some(768));
        assert_eq!(req.tokens.cache_write, Some(0));
        assert_eq!(req.tokens.output, Some(190));
        assert_eq!(req.tokens.reasoning, Some(128));
    }

    #[test]
    fn input_does_not_include_cache_read() {
        let mut reader = OpenCodeCliJsonReader::default();
        let events = reader.read(&step(275, 18, 25, 9472, 0));
        let UsageEvent::Request(req) = &events[0] else {
            panic!("request");
        };
        assert_eq!(req.tokens.input, Some(275));
        assert_eq!(req.tokens.cache_read, Some(9472));
    }

    #[test]
    fn step_finish_rows_stay_partial_without_a_run_total() {
        let snap = track(&[
            step(10, 2, 1, 0, 0),
            json!({ "type": "text", "part": { "type": "text", "text": "DONE" } }),
        ]);
        assert_eq!(snap.requests.len(), 1);
        assert_eq!(snap.totals.status, UsageStatus::Partial);
        assert_eq!(snap.totals.tokens.output, Some(3));
        assert!(snap.subagents.is_empty());
    }

    #[test]
    fn step_without_tokens_is_ignored() {
        let snap = track(&[json!({
            "type": "step_finish",
            "part": { "type": "step-finish", "reason": "tool-calls" }
        })]);
        assert!(snap.requests.is_empty());
        assert_eq!(snap.totals.status, UsageStatus::None);
    }
}
