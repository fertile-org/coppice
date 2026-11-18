//! Codex `codex exec --json`.
//!
//! What the stream carries (checked on 0.163.0):
//! - `turn.completed.usage`: one object per turn with `input_tokens`,
//!   `cached_input_tokens`, `cache_write_input_tokens`, `output_tokens`,
//!   `reasoning_output_tokens`.
//! - `input_tokens` includes cached reads. Uncached input is
//!   `input_tokens - cached_input_tokens`. `output_tokens` includes
//!   reasoning (`total_tokens` in the session rollout is input + output).
//! - Usage is per turn, not a session cumulative: a resumed process emits
//!   only that process's turn, so earlier turns are not counted again.
//! - The stream names no model and carries no subagent usage.
//! - One run can start several processes (connector-check phases, resume).
//!   Their turn rows and totals are added up.

use super::{ProcessContext, UsageReader};
use crate::domain::usage::{
    Granularity, RequestUsage, RunTotalUsage, TokenCounts, UsageEvent,
};
use serde_json::Value;

#[derive(Default)]
pub struct CodexJsonReader {
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

impl UsageReader for CodexJsonReader {
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
            Some("turn.completed") => self.turn_completed(event),
            _ => Vec::new(),
        }
    }
}

impl CodexJsonReader {
    fn turn_completed(&mut self, event: &Value) -> Vec<UsageEvent> {
        let Some(usage) = event.get("usage") else {
            return Vec::new();
        };
        let tokens = turn_tokens(usage);
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

/// Converts Codex's inclusive input and reasoning-in-output into Coppice
/// [`TokenCounts`]. Missing keys stay `None`.
fn turn_tokens(usage: &Value) -> TokenCounts {
    let input_tokens = int(usage, "input_tokens");
    let cached = int(usage, "cached_input_tokens");
    TokenCounts {
        // `input_tokens` includes cached reads when both are present.
        input: match (input_tokens, cached) {
            (Some(input), Some(cached)) => Some((input - cached).max(0)),
            (Some(input), None) => Some(input),
            (None, _) => None,
        },
        cache_read: cached,
        cache_write: int(usage, "cache_write_input_tokens"),
        output: int(usage, "output_tokens"),
        reasoning: int(usage, "reasoning_output_tokens"),
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
            .join("../fixtures/codex")
            .join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{}: {err}", path.display()))
            .lines()
            .map(|line| serde_json::from_str(line).expect("json line"))
            .collect()
    }

    fn track(events: &[Value], resumed: bool) -> crate::domain::usage::UsageSnapshot {
        let mut tracker = UsageTracker::new();
        track_processes(
            &mut tracker,
            &mut CodexJsonReader::default(),
            resumed,
            events,
        );
        tracker.snapshot()
    }

    fn track_processes(
        tracker: &mut UsageTracker,
        reader: &mut CodexJsonReader,
        resumed: bool,
        events: &[Value],
    ) {
        let process = ProcessContext {
            resumed,
            ..ProcessContext::default()
        };
        for usage in reader.start_process(&process) {
            tracker.apply(usage);
        }
        for event in events {
            for usage in reader.read(event) {
                tracker.apply(usage);
            }
        }
    }

    fn turn(input: i64, cached: i64, output: i64, reasoning: i64) -> Value {
        json!({
            "type": "turn.completed",
            "usage": {
                "input_tokens": input,
                "cached_input_tokens": cached,
                "cache_write_input_tokens": 0,
                "output_tokens": output,
                "reasoning_output_tokens": reasoning,
            }
        })
    }

    #[test]
    fn live_capture_yields_turn_rows_and_run_total() {
        let snap = track(&fixture("codex-0.163.0-usage.jsonl"), false);
        assert_eq!(snap.requests.len(), 1);
        assert_eq!(snap.requests[0].granularity, Granularity::Turn);
        assert_eq!(snap.requests[0].id.as_deref(), Some("turn-1"));
        assert_eq!(snap.requests[0].model, None);
        // input_tokens 25711 includes cached 23552.
        assert_eq!(snap.requests[0].tokens.input, Some(2159));
        assert_eq!(snap.requests[0].tokens.cache_read, Some(23552));
        assert_eq!(snap.requests[0].tokens.cache_write, Some(0));
        assert_eq!(snap.requests[0].tokens.output, Some(41));
        assert_eq!(snap.requests[0].tokens.reasoning, Some(0));
        assert!(snap.subagents.is_empty());
        assert_eq!(snap.totals.status, UsageStatus::Reported);
        assert_eq!(snap.totals.tokens, snap.requests[0].tokens);
        assert_eq!(snap.totals.cost_usd, None);
        assert!(snap.totals.by_model.is_empty());
    }

    #[test]
    fn resumed_capture_does_not_count_earlier_turns() {
        let snap = track(&fixture("codex-0.163.0-usage-resume.jsonl"), true);
        assert_eq!(snap.requests.len(), 1);
        // Only this process's turn: input 38624 − cached 36224, output 48.
        // The earlier process's 41 output tokens are absent.
        assert_eq!(snap.requests[0].tokens.input, Some(2400));
        assert_eq!(snap.requests[0].tokens.cache_read, Some(36224));
        assert_eq!(snap.requests[0].tokens.output, Some(48));
        assert_eq!(snap.totals.tokens.output, Some(48));
        assert_ne!(snap.totals.tokens.output, Some(41 + 48));
    }

    #[test]
    fn input_includes_cached_reads() {
        let mut reader = CodexJsonReader::default();
        let UsageEvent::Request(req) = &reader.read(&turn(100, 40, 5, 1))[0] else {
            panic!("request");
        };
        assert_eq!(req.tokens.input, Some(60));
        assert_eq!(req.tokens.cache_read, Some(40));
        assert_eq!(req.tokens.output, Some(5));
        assert_eq!(req.tokens.reasoning, Some(1));
    }

    #[test]
    fn turn_without_usage_is_ignored() {
        let snap = track(&[json!({ "type": "turn.completed" })], false);
        assert!(snap.requests.is_empty());
        assert_eq!(snap.totals.status, UsageStatus::None);
    }

    #[test]
    fn empty_usage_object_reports_nothing() {
        let snap = track(&[json!({ "type": "turn.completed", "usage": {} })], false);
        assert!(snap.requests.is_empty());
        assert_eq!(snap.totals.status, UsageStatus::None);
    }

    #[test]
    fn totals_of_several_processes_are_added() {
        let mut tracker = UsageTracker::new();
        let mut reader = CodexJsonReader::default();
        track_processes(&mut tracker, &mut reader, false, &[turn(10, 2, 4, 0)]);
        assert_eq!(tracker.snapshot().totals.tokens.output, Some(4));
        assert_eq!(tracker.snapshot().requests.len(), 1);

        track_processes(&mut tracker, &mut reader, true, &[turn(20, 5, 3, 0)]);
        let snap = tracker.snapshot();
        assert_eq!(snap.requests.len(), 2);
        assert_eq!(snap.totals.status, UsageStatus::Reported);
        assert_eq!(snap.totals.tokens.input, Some(8 + 15));
        assert_eq!(snap.totals.tokens.cache_read, Some(2 + 5));
        assert_eq!(snap.totals.tokens.output, Some(7));
    }

    #[test]
    fn several_turns_in_one_process_are_summed() {
        let snap = track(&[turn(10, 0, 2, 0), turn(12, 4, 3, 1)], false);
        assert_eq!(snap.requests.len(), 2);
        assert_eq!(snap.requests[0].id.as_deref(), Some("turn-1"));
        assert_eq!(snap.requests[1].id.as_deref(), Some("turn-2"));
        assert_eq!(snap.totals.tokens.input, Some(10 + 8));
        assert_eq!(snap.totals.tokens.cache_read, Some(0 + 4));
        assert_eq!(snap.totals.tokens.output, Some(5));
        assert_eq!(snap.totals.tokens.reasoning, Some(0 + 1));
    }
}
