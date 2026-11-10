//! Cursor `cursor-agent --output-format stream-json`.
//!
//! What the stream carries (checked on 2026.11.13-eacade1):
//! - The terminal `result` event has a camelCase `usage` object:
//!   `inputTokens`, `outputTokens`, `cacheReadTokens`, `cacheWriteTokens`.
//!   It may also report `reasoningTokens`; Coppice does not record that.
//! - No per-request usage is streamed, and there is no cost field.
//! - One run can start several processes (connector-check phases). Their
//!   `result.usage` totals are added up. A process whose `result` lacks
//!   `usage` (or has an empty object) emits nothing and does not invalidate
//!   an earlier process's total.

use super::{ProcessContext, UsageReader};
use crate::domain::usage::{RunTotalUsage, TokenCounts, UsageEvent};
use serde_json::Value;

#[derive(Default)]
pub struct CursorStreamJsonReader {
    /// Totals of the processes that already ended.
    finished: Option<RunTotalUsage>,
    process: Process,
}

#[derive(Default)]
struct Process {
    started: bool,
    total: Option<RunTotalUsage>,
}

impl UsageReader for CursorStreamJsonReader {
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
        match event.get("type").and_then(Value::as_str) {
            Some("result") => self.result(event),
            _ => Vec::new(),
        }
    }
}

impl CursorStreamJsonReader {
    fn result(&mut self, event: &Value) -> Vec<UsageEvent> {
        let Some(usage) = event.get("usage") else {
            return Vec::new();
        };
        let tokens = TokenCounts {
            input: int(usage, "inputTokens"),
            cache_read: int(usage, "cacheReadTokens"),
            cache_write: int(usage, "cacheWriteTokens"),
            output: int(usage, "outputTokens"),
            reasoning: None,
        };
        // An empty `usage: {}` reports nothing.
        if tokens.is_empty() {
            return Vec::new();
        }
        let process_total = RunTotalUsage {
            tokens,
            cost_usd: None,
        };
        self.process.total = Some(process_total.clone());
        let mut run_total = self.finished.clone();
        add_total(&mut run_total, &process_total);
        run_total.map(UsageEvent::RunTotal).into_iter().collect()
    }
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

    fn fixture(name: &str) -> Vec<Value> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/cursor")
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
            &mut CursorStreamJsonReader::default(),
            events,
        );
        tracker.snapshot()
    }

    fn track_processes(
        tracker: &mut UsageTracker,
        reader: &mut CursorStreamJsonReader,
        events: &[Value],
    ) {
        for usage in reader.start_process(&ProcessContext::default()) {
            tracker.apply(usage);
        }
        for event in events {
            for usage in reader.read(event) {
                tracker.apply(usage);
            }
        }
    }

    fn result_usage(input: i64, output: i64) -> Value {
        json!({
            "type": "result",
            "usage": {
                "inputTokens": input,
                "outputTokens": output,
                "cacheReadTokens": 1,
                "cacheWriteTokens": 2,
            }
        })
    }

    #[test]
    fn live_capture_yields_run_totals() {
        let snap = track(&fixture("cursor-agent-2026.11.13-eacade1-usage.jsonl"));
        assert!(snap.requests.is_empty());
        assert!(snap.subagents.is_empty());
        assert_eq!(snap.totals.status, UsageStatus::Reported);
        assert_eq!(snap.totals.tokens.input, Some(8686));
        assert_eq!(snap.totals.tokens.output, Some(85));
        assert_eq!(snap.totals.tokens.cache_read, Some(3968));
        assert_eq!(snap.totals.tokens.cache_write, Some(0));
        assert_eq!(snap.totals.tokens.reasoning, None);
        assert_eq!(snap.totals.cost_usd, None);
    }

    #[test]
    fn result_without_usage_is_not_reported() {
        let snap = track(&fixture("done.jsonl"));
        assert_eq!(snap.totals.status, UsageStatus::None);
        assert!(snap.totals.tokens.is_empty());
        assert!(snap.requests.is_empty());
    }

    #[test]
    fn empty_usage_object_reports_nothing() {
        let snap = track(&[json!({ "type": "result", "usage": {} })]);
        assert_eq!(snap.totals.status, UsageStatus::None);
        assert!(snap.totals.tokens.is_empty());
    }

    #[test]
    fn missing_counts_stay_none() {
        let mut reader = CursorStreamJsonReader::default();
        let UsageEvent::RunTotal(total) = &reader.read(&json!({
            "type": "result",
            "usage": { "outputTokens": 3 }
        }))[0]
        else {
            panic!("total");
        };
        assert_eq!(total.tokens.output, Some(3));
        assert_eq!(total.tokens.input, None);
        assert_eq!(total.tokens.cache_read, None);
        assert_eq!(total.tokens.cache_write, None);
        assert_eq!(total.tokens.reasoning, None);
    }

    #[test]
    fn totals_of_several_processes_are_added() {
        let mut tracker = UsageTracker::new();
        let mut reader = CursorStreamJsonReader::default();
        track_processes(&mut tracker, &mut reader, &[result_usage(10, 4)]);
        assert_eq!(tracker.snapshot().totals.tokens.output, Some(4));
        assert_eq!(tracker.snapshot().totals.tokens.input, Some(10));

        track_processes(&mut tracker, &mut reader, &[result_usage(5, 2)]);
        let totals = tracker.snapshot().totals;
        assert_eq!(totals.status, UsageStatus::Reported);
        assert_eq!(totals.tokens.input, Some(15));
        assert_eq!(totals.tokens.output, Some(6));
        assert_eq!(totals.tokens.cache_read, Some(2));
        assert_eq!(totals.tokens.cache_write, Some(4));
    }

    #[test]
    fn process_without_rows_keeps_the_earlier_total() {
        let mut tracker = UsageTracker::new();
        let mut reader = CursorStreamJsonReader::default();
        track_processes(&mut tracker, &mut reader, &[result_usage(10, 4)]);
        track_processes(
            &mut tracker,
            &mut reader,
            &[json!({ "type": "result", "subtype": "success" })],
        );
        let totals = tracker.snapshot().totals;
        assert_eq!(totals.status, UsageStatus::Reported);
        assert_eq!(totals.tokens.output, Some(4));
        assert_eq!(totals.tokens.input, Some(10));
    }
}
