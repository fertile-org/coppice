//! One test harness for every usage reader.
//!
//! Each capture `fixtures/<dir>/<name>.jsonl` of a directory listed in
//! [`CAPTURE_DIRS`] has `<name>.usage.json` beside it: the process the capture
//! came from (`process`) and the normalized result (`requests`, `subagents`,
//! `totals`). The harness replays the capture through the format's reader and
//! a [`UsageTracker`], compares the result, and checks the rules every reader
//! follows (see [`invariant_errors`]).
//!
//! To write or refresh the expected files after a deliberate reader change:
//! `UPDATE_USAGE_FIXTURES=1 cargo test -p coppice-server --lib usage::harness`,
//! then review the diff. A missing `process` is written as a fresh process.

use super::{reader_for, ProcessContext};
use crate::domain::usage::{Granularity, TokenCounts, UsageSnapshot, UsageStatus, UsageTracker};
use coppice_connectors::{
    CoverageGranularity, SubagentSource, TokenFields, UsageCoverage, UsageFormat,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Capture directories under `fixtures/` and the format their captures are in.
const CAPTURE_DIRS: &[(&str, UsageFormat)] = &[("claude-code", UsageFormat::ClaudeStreamJson)];

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures")
}

fn captures(dir: &str) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(fixtures_root().join(dir))
        .unwrap_or_else(|err| panic!("fixtures/{dir}: {err}"))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    paths.sort();
    paths
}

fn expected_path(capture: &Path) -> PathBuf {
    capture.with_extension("usage.json")
}

fn process_from(expected: Option<&Value>) -> ProcessContext {
    let process = expected.and_then(|e| e.get("process"));
    ProcessContext {
        resumed: process
            .and_then(|p| p.get("resumed"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        cli_version: process
            .and_then(|p| p.get("cliVersion"))
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn process_json(process: &ProcessContext) -> Value {
    json!({ "resumed": process.resumed, "cliVersion": process.cli_version })
}

/// One CLI process: `start_process`, then every line of the capture.
fn replay(format: UsageFormat, capture: &Path, process: &ProcessContext) -> UsageSnapshot {
    let mut reader = reader_for(format).expect("a format with a reader");
    let mut tracker = UsageTracker::new();
    for event in reader.start_process(process) {
        tracker.apply(event);
    }
    let text = std::fs::read_to_string(capture)
        .unwrap_or_else(|err| panic!("{}: {err}", capture.display()));
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let value: Value = serde_json::from_str(line)
            .unwrap_or_else(|err| panic!("{}: {err}", capture.display()));
        for event in reader.read(&value) {
            tracker.apply(event);
        }
    }
    tracker.snapshot()
}

/// The part of a snapshot an expected file pins down.
fn normalized(snapshot: &UsageSnapshot) -> Value {
    json!({
        "requests": snapshot.requests,
        "subagents": snapshot.subagents,
        "totals": snapshot.totals,
    })
}

fn counts_errors(what: &str, tokens: &TokenCounts) -> Vec<String> {
    let mut errors = Vec::new();
    let fields = [
        ("input", tokens.input),
        ("cacheRead", tokens.cache_read),
        ("cacheWrite", tokens.cache_write),
        ("output", tokens.output),
        ("reasoning", tokens.reasoning),
    ];
    for (name, value) in fields {
        if value.is_some_and(|v| v < 0) {
            errors.push(format!("{what}: {name} is negative ({value:?})"));
        }
    }
    if let (Some(reasoning), Some(output)) = (tokens.reasoning, tokens.output) {
        if reasoning > output {
            errors.push(format!(
                "{what}: reasoning ({reasoning}) is more than output ({output}); \
                 output must include reasoning"
            ));
        }
    }
    errors
}

/// The rules every reader's result follows, whatever its format.
pub(super) fn invariant_errors(snapshot: &UsageSnapshot) -> Vec<String> {
    let mut errors = Vec::new();
    for (i, request) in snapshot.requests.iter().enumerate() {
        errors.extend(counts_errors(&format!("request {i}"), &request.tokens));
    }
    for subagent in &snapshot.subagents {
        let what = format!("subagent {}", subagent.id);
        errors.extend(counts_errors(&what, &subagent.tokens));
        if subagent.total_tokens.is_some_and(|t| t < 0) {
            errors.push(format!("{what}: total is negative"));
        }
    }
    let totals = &snapshot.totals;
    errors.extend(counts_errors("run total", &totals.tokens));
    if totals.subagent_tokens.is_some_and(|t| t < 0) {
        errors.push("run total: subagent tokens are negative".into());
    }
    if totals.status == UsageStatus::Reported {
        // The CLI's total covers every row: requests, subagent breakdowns and
        // subagent totals without a breakdown.
        let mut rows = TokenCounts::default();
        let mut unsplit = 0;
        for request in &snapshot.requests {
            rows.add(&request.tokens);
        }
        for subagent in &snapshot.subagents {
            if subagent.tokens.is_empty() {
                unsplit += subagent.total_tokens.unwrap_or(0);
            } else {
                rows.add(&subagent.tokens);
            }
        }
        let pairs = [
            ("input", rows.input, totals.tokens.input),
            ("cacheRead", rows.cache_read, totals.tokens.cache_read),
            ("cacheWrite", rows.cache_write, totals.tokens.cache_write),
            ("output", rows.output, totals.tokens.output),
        ];
        for (name, rows, total) in pairs {
            if let (Some(rows), Some(total)) = (rows, total) {
                if rows > total {
                    errors.push(format!(
                        "rows add up to more {name} ({rows}) than the run total ({total})"
                    ));
                }
            }
        }
        let rows_total = rows.total().unwrap_or(0) + unsplit;
        if let Some(total) = totals.tokens.total() {
            if rows_total > total {
                errors.push(format!(
                    "rows and subagent totals ({rows_total}) are more than the run total ({total})"
                ));
            }
        }
    }
    errors
}

fn fields_errors(what: &str, tokens: &TokenCounts, fields: &TokenFields) -> Vec<String> {
    let checks = [
        ("input", tokens.input, fields.input),
        ("cacheRead", tokens.cache_read, fields.cache_read),
        ("cacheWrite", tokens.cache_write, fields.cache_write),
        ("output", tokens.output, fields.output),
        ("reasoning", tokens.reasoning, fields.reasoning),
    ];
    checks
        .iter()
        .filter(|(_, value, declared)| value.is_some() && !declared)
        .map(|(name, _, _)| format!("{what}: reports {name}, which the coverage leaves out"))
        .collect()
}

/// What the result reports beyond the format's declared coverage.
pub(super) fn coverage_errors(snapshot: &UsageSnapshot, coverage: &UsageCoverage) -> Vec<String> {
    let mut errors = Vec::new();
    for (i, request) in snapshot.requests.iter().enumerate() {
        let what = format!("request {i}");
        errors.extend(fields_errors(&what, &request.tokens, &coverage.fields));
        let allowed = match coverage.granularity {
            CoverageGranularity::Request => true,
            CoverageGranularity::Turn => request.granularity == Granularity::Turn,
            CoverageGranularity::RunTotalOnly => false,
        };
        if !allowed {
            errors.push(format!(
                "{what}: a {:?} row from a format whose coverage is {:?}",
                request.granularity, coverage.granularity
            ));
        }
    }
    for subagent in &snapshot.subagents {
        let what = format!("subagent {}", subagent.id);
        errors.extend(fields_errors(&what, &subagent.tokens, &coverage.fields));
        match coverage.subagents {
            SubagentSource::Breakdown => {}
            SubagentSource::TotalOnly if subagent.tokens.is_empty() => {}
            SubagentSource::TotalOnly => {
                errors.push(format!("{what}: a breakdown from a total-only format"))
            }
            SubagentSource::None => {
                errors.push(format!("{what}: a subagent from a format without subagents"))
            }
        }
    }
    errors.extend(fields_errors("run total", &snapshot.totals.tokens, &coverage.fields));
    errors
}

/// Differences between an expected file's result and the replayed one.
pub(super) fn mismatch(expected: &Value, actual: &Value) -> Option<String> {
    let keys = ["requests", "subagents", "totals"];
    let wrong: Vec<_> = keys
        .iter()
        .filter(|key| expected.get(**key) != actual.get(**key))
        .collect();
    if wrong.is_empty() {
        return None;
    }
    let actual = serde_json::to_string_pretty(actual).unwrap_or_default();
    Some(format!("{wrong:?} differ; replayed result:\n{actual}"))
}

#[test]
fn every_capture_matches_its_expected_usage() {
    let update = std::env::var_os("UPDATE_USAGE_FIXTURES").is_some();
    let mut failures = Vec::new();
    let mut checked = 0;
    for (dir, format) in CAPTURE_DIRS {
        for capture in captures(dir) {
            let name = format!("{dir}/{}", capture.file_name().unwrap().to_string_lossy());
            let expected_file = expected_path(&capture);
            let expected: Option<Value> = std::fs::read_to_string(&expected_file)
                .ok()
                .map(|text| serde_json::from_str(&text).expect("expected file is JSON"));
            let process = process_from(expected.as_ref());
            let snapshot = replay(*format, &capture, &process);
            let coverage = format.coverage().expect("a format with a reader has coverage");
            let errors = invariant_errors(&snapshot)
                .into_iter()
                .chain(coverage_errors(&snapshot, &coverage));
            for error in errors {
                failures.push(format!("{name}: {error}"));
            }
            let mut actual = normalized(&snapshot);
            if update {
                actual["process"] = process_json(&process);
                let text = serde_json::to_string_pretty(&actual).expect("json") + "\n";
                std::fs::write(&expected_file, text).expect("write expected file");
                checked += 1;
                continue;
            }
            match expected {
                None => failures.push(format!(
                    "{name}: no {} (run with UPDATE_USAGE_FIXTURES=1 to write it)",
                    expected_file.file_name().unwrap().to_string_lossy()
                )),
                Some(expected) => {
                    if let Some(diff) = mismatch(&expected, &actual) {
                        failures.push(format!("{name}: {diff}"));
                    }
                }
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no captures found");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn a_wrong_expected_result_is_reported() {
    let capture = fixtures_root().join("claude-code/claude-code-2.1.318-usage-partial.jsonl");
    let snapshot = replay(
        UsageFormat::ClaudeStreamJson,
        &capture,
        &ProcessContext::default(),
    );
    let actual = normalized(&snapshot);
    assert!(mismatch(&actual, &actual).is_none());
    let mut wrong = actual.clone();
    wrong["totals"]["tokens"]["output"] = json!(1);
    let diff = mismatch(&wrong, &actual).expect("a mismatch");
    assert!(diff.contains("totals"), "{diff}");
}

#[test]
fn broken_counts_are_reported() {
    use crate::domain::usage::{RequestUsage, RunTotalUsage, UsageEvent};
    let mut tracker = UsageTracker::new();
    tracker.apply(UsageEvent::Request(RequestUsage {
        id: Some("r1".into()),
        tokens: TokenCounts {
            input: Some(-1),
            output: Some(5),
            reasoning: Some(9),
            ..TokenCounts::default()
        },
        ..RequestUsage::default()
    }));
    let errors = invariant_errors(&tracker.snapshot());
    assert!(errors.iter().any(|e| e.contains("input is negative")), "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("reasoning (9)")), "{errors:?}");

    // A reported total smaller than its rows.
    let mut tracker = UsageTracker::new();
    tracker.apply(UsageEvent::Request(RequestUsage {
        id: Some("r1".into()),
        tokens: TokenCounts {
            output: Some(50),
            ..TokenCounts::default()
        },
        ..RequestUsage::default()
    }));
    tracker.apply(UsageEvent::RunTotal(RunTotalUsage {
        tokens: TokenCounts {
            output: Some(10),
            ..TokenCounts::default()
        },
        cost_usd: None,
    }));
    let errors = invariant_errors(&tracker.snapshot());
    assert!(errors.iter().any(|e| e.contains("more output")), "{errors:?}");
}

#[test]
fn results_outside_the_coverage_are_reported() {
    use crate::domain::usage::{RequestUsage, SubagentUsage, UsageEvent};
    let mut tracker = UsageTracker::new();
    tracker.apply(UsageEvent::Request(RequestUsage {
        id: Some("r1".into()),
        granularity: Granularity::Request,
        tokens: TokenCounts {
            input: Some(5),
            cache_write: Some(2),
            ..TokenCounts::default()
        },
        ..RequestUsage::default()
    }));
    tracker.apply(UsageEvent::Subagent(SubagentUsage {
        id: "s1".into(),
        tokens: TokenCounts {
            output: Some(1),
            ..TokenCounts::default()
        },
        ..SubagentUsage::default()
    }));
    let snapshot = tracker.snapshot();
    // Like a per-turn CLI without cache writes or subagents.
    let turn_only = UsageCoverage {
        granularity: CoverageGranularity::Turn,
        model: coppice_connectors::ModelSource::RunOnly,
        fields: TokenFields {
            cache_write: false,
            ..TokenFields::ALL
        },
        subagents: SubagentSource::None,
        context_window: coppice_connectors::ContextWindowSource::None,
    };
    let errors = coverage_errors(&snapshot, &turn_only);
    assert!(errors.iter().any(|e| e.contains("request 0: reports cacheWrite")), "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("Request row")), "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("without subagents")), "{errors:?}");
    let total_only = UsageCoverage {
        subagents: SubagentSource::TotalOnly,
        ..turn_only
    };
    let errors = coverage_errors(&snapshot, &total_only);
    assert!(errors.iter().any(|e| e.contains("breakdown from a total-only")), "{errors:?}");
}
