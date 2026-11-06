//! Token usage, independent of any connector.
//!
//! Readers in `providers::usage` turn a CLI's own output into [`UsageEvent`]s;
//! a [`UsageTracker`] folds them into the run's requests, subagents and totals.
//! Nothing here knows which connector produced an event. A missing count is
//! `None` ("not reported"), never 0.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Token counts as one source reported them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenCounts {
    /// Input that was neither read from nor written to the prompt cache.
    pub input: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_write: Option<i64>,
    /// All output, reasoning included.
    pub output: Option<i64>,
    /// The reasoning part of `output`.
    pub reasoning: Option<i64>,
}

impl TokenCounts {
    pub fn is_empty(&self) -> bool {
        self.input.is_none()
            && self.cache_read.is_none()
            && self.cache_write.is_none()
            && self.output.is_none()
            && self.reasoning.is_none()
    }

    /// Adds field by field. A field stays `None` only when both sides lack it.
    pub fn add(&mut self, other: &TokenCounts) {
        fn sum(a: Option<i64>, b: Option<i64>) -> Option<i64> {
            match (a, b) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
            }
        }
        self.input = sum(self.input, other.input);
        self.cache_read = sum(self.cache_read, other.cache_read);
        self.cache_write = sum(self.cache_write, other.cache_write);
        self.output = sum(self.output, other.output);
        self.reasoning = sum(self.reasoning, other.reasoning);
    }

    /// Fields `other` reports replace this one's; the rest are kept.
    pub fn overlay(&mut self, other: &TokenCounts) {
        self.input = other.input.or(self.input);
        self.cache_read = other.cache_read.or(self.cache_read);
        self.cache_write = other.cache_write.or(self.cache_write);
        self.output = other.output.or(self.output);
        self.reasoning = other.reasoning.or(self.reasoning);
    }

    /// Every token processed: input, both cache directions, and output.
    pub fn total(&self) -> Option<i64> {
        let parts = [self.input, self.cache_read, self.cache_write, self.output];
        parts
            .iter()
            .any(Option::is_some)
            .then(|| parts.iter().map(|p| p.unwrap_or(0)).sum())
    }
}

/// What one usage row covers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Granularity {
    /// One LLM request.
    #[default]
    Request,
    /// One agent turn, for CLIs that do not report single requests.
    Turn,
}

impl Granularity {
    pub fn as_str(self) -> &'static str {
        match self {
            Granularity::Request => "request",
            Granularity::Turn => "turn",
        }
    }
}

/// One LLM request (or turn) on the run's own thread.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestUsage {
    /// The CLI's id for the request. A later event with the same id updates
    /// the row; `None` always adds a row.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub granularity: Granularity,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tokens: TokenCounts,
}

/// A subagent the connector spawned inside the run, with its own totals.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentUsage {
    /// The CLI's id for the subagent. Later events with the same id update it.
    pub id: String,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tokens: TokenCounts,
    /// The CLI's own total, when it reports one without a breakdown.
    #[serde(default)]
    pub total_tokens: Option<i64>,
    #[serde(default)]
    pub tool_uses: Option<i64>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

impl SubagentUsage {
    fn overlay(&mut self, other: &SubagentUsage) {
        self.agent_type = other.agent_type.clone().or(self.agent_type.take());
        self.description = other.description.clone().or(self.description.take());
        self.model = other.model.clone().or(self.model.take());
        self.status = other.status.clone().or(self.status.take());
        self.tokens.overlay(&other.tokens);
        self.total_tokens = other.total_tokens.or(self.total_tokens);
        self.tool_uses = other.tool_uses.or(self.tool_uses);
        self.duration_ms = other.duration_ms.or(self.duration_ms);
    }

    /// The breakdown's total, or the CLI's own when there is no breakdown.
    pub fn total(&self) -> Option<i64> {
        self.tokens.total().or(self.total_tokens)
    }
}

/// The CLI's own total for the run so far, subagents included. Each one
/// replaces the previous.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunTotalUsage {
    #[serde(default)]
    pub tokens: TokenCounts,
    /// Stored for later; not shown.
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

/// One normalized usage report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UsageEvent {
    Request(RequestUsage),
    Subagent(SubagentUsage),
    RunTotal(RunTotalUsage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageStatus {
    /// The CLI reported its own total.
    Reported,
    /// Only per-request or subagent rows arrived (still running, or the run
    /// ended before the CLI's total).
    Partial,
}

impl UsageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            UsageStatus::Reported => "reported",
            UsageStatus::Partial => "partial",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "reported" => Some(UsageStatus::Reported),
            "partial" => Some(UsageStatus::Partial),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunUsageTotals {
    pub status: UsageStatus,
    pub tokens: TokenCounts,
    pub request_count: i64,
    pub subagent_count: i64,
    pub cost_usd: Option<f64>,
}

/// Everything known about a run's usage at one moment. `version` grows with
/// every change, so a writer can refuse to overwrite a newer snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageSnapshot {
    pub version: i64,
    pub requests: Vec<RequestUsage>,
    /// The version at which each request last changed, by index.
    pub request_versions: Vec<i64>,
    pub subagents: Vec<SubagentUsage>,
    /// The version at which each subagent last changed, by index.
    pub subagent_versions: Vec<i64>,
    pub totals: RunUsageTotals,
}

/// Folds usage events into one run's state.
#[derive(Debug, Default)]
pub struct UsageTracker {
    requests: Vec<RequestUsage>,
    request_versions: Vec<i64>,
    request_index: HashMap<String, usize>,
    subagents: Vec<SubagentUsage>,
    subagent_versions: Vec<i64>,
    subagent_index: HashMap<String, usize>,
    reported: Option<RunTotalUsage>,
    version: i64,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, event: UsageEvent) {
        // An all-empty run total (e.g. `modelUsage: {}`) reports nothing. Taking
        // it as "reported" would hide the per-request sum behind null counts.
        if matches!(&event, UsageEvent::RunTotal(total) if total.tokens.is_empty()) {
            return;
        }
        self.version += 1;
        match event {
            UsageEvent::Request(request) => self.apply_request(request),
            UsageEvent::Subagent(subagent) => self.apply_subagent(subagent),
            UsageEvent::RunTotal(total) => self.reported = Some(total),
        }
    }

    fn apply_request(&mut self, request: RequestUsage) {
        let existing = request
            .id
            .as_ref()
            .and_then(|id| self.request_index.get(id).copied());
        match existing {
            Some(i) => {
                let row = &mut self.requests[i];
                row.tokens.overlay(&request.tokens);
                row.model = request.model.or(row.model.take());
                self.request_versions[i] = self.version;
            }
            None => {
                if let Some(id) = &request.id {
                    self.request_index.insert(id.clone(), self.requests.len());
                }
                self.requests.push(request);
                self.request_versions.push(self.version);
            }
        }
    }

    fn apply_subagent(&mut self, subagent: SubagentUsage) {
        match self.subagent_index.get(&subagent.id).copied() {
            Some(i) => {
                self.subagents[i].overlay(&subagent);
                self.subagent_versions[i] = self.version;
            }
            None => {
                self.subagent_index
                    .insert(subagent.id.clone(), self.subagents.len());
                self.subagents.push(subagent);
                self.subagent_versions.push(self.version);
            }
        }
    }

    /// `None` until the first event.
    pub fn snapshot(&self) -> Option<UsageSnapshot> {
        if self.version == 0 {
            return None;
        }
        Some(UsageSnapshot {
            version: self.version,
            requests: self.requests.clone(),
            request_versions: self.request_versions.clone(),
            subagents: self.subagents.clone(),
            subagent_versions: self.subagent_versions.clone(),
            totals: self.totals(),
        })
    }

    /// The CLI's own total when it gave one; otherwise the sum of the rows.
    fn totals(&self) -> RunUsageTotals {
        let (status, tokens, cost_usd) = match &self.reported {
            Some(total) => (UsageStatus::Reported, total.tokens, total.cost_usd),
            None => {
                let mut sum = TokenCounts::default();
                for request in &self.requests {
                    sum.add(&request.tokens);
                }
                for subagent in &self.subagents {
                    sum.add(&subagent.tokens);
                }
                (UsageStatus::Partial, sum, None)
            }
        };
        RunUsageTotals {
            status,
            tokens,
            request_count: self.requests.len() as i64,
            subagent_count: self.subagents.len() as i64,
            cost_usd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(input: i64, output: i64) -> TokenCounts {
        TokenCounts {
            input: Some(input),
            output: Some(output),
            ..TokenCounts::default()
        }
    }

    fn request(id: Option<&str>, tokens: TokenCounts) -> UsageEvent {
        UsageEvent::Request(RequestUsage {
            id: id.map(str::to_string),
            tokens,
            ..RequestUsage::default()
        })
    }

    #[test]
    fn add_keeps_missing_fields_missing() {
        let mut a = TokenCounts {
            input: Some(1),
            ..TokenCounts::default()
        };
        a.add(&TokenCounts {
            input: Some(2),
            output: Some(5),
            ..TokenCounts::default()
        });
        assert_eq!(a.input, Some(3));
        assert_eq!(a.output, Some(5));
        assert_eq!(a.cache_read, None);
        assert_eq!(TokenCounts::default().total(), None);
        assert_eq!(a.total(), Some(8));
    }

    #[test]
    fn empty_tracker_has_no_snapshot() {
        assert!(UsageTracker::new().snapshot().is_none());
    }

    #[test]
    fn same_request_id_updates_one_row() {
        let mut tracker = UsageTracker::new();
        tracker.apply(request(Some("m1"), counts(10, 1)));
        tracker.apply(request(
            Some("m1"),
            TokenCounts {
                output: Some(40),
                ..TokenCounts::default()
            },
        ));
        tracker.apply(request(None, counts(3, 3)));
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.requests.len(), 2);
        assert_eq!(snap.requests[0].tokens, counts(10, 40));
        assert_eq!(snap.totals.status, UsageStatus::Partial);
        assert_eq!(snap.totals.tokens, counts(13, 43));
        assert_eq!(snap.totals.request_count, 2);
        assert_eq!(snap.version, 3);
        // m1 last changed at version 2, the id-less row at 3.
        assert_eq!(snap.request_versions, [2, 3]);
    }

    #[test]
    fn partial_totals_include_subagents() {
        let mut tracker = UsageTracker::new();
        tracker.apply(request(Some("m1"), counts(10, 10)));
        tracker.apply(UsageEvent::Subagent(SubagentUsage {
            id: "t1".into(),
            status: Some("running".into()),
            ..SubagentUsage::default()
        }));
        tracker.apply(UsageEvent::Subagent(SubagentUsage {
            id: "t1".into(),
            tokens: counts(5, 5),
            total_tokens: Some(10),
            ..SubagentUsage::default()
        }));
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.subagents.len(), 1);
        assert_eq!(snap.subagents[0].status.as_deref(), Some("running"));
        assert_eq!(snap.totals.tokens, counts(15, 15));
        assert_eq!(snap.totals.subagent_count, 1);
    }

    #[test]
    fn reported_total_wins_over_row_sum() {
        let mut tracker = UsageTracker::new();
        tracker.apply(request(Some("m1"), counts(10, 10)));
        tracker.apply(UsageEvent::RunTotal(RunTotalUsage {
            tokens: counts(100, 100),
            cost_usd: Some(0.5),
        }));
        let totals = tracker.snapshot().unwrap().totals;
        assert_eq!(totals.status, UsageStatus::Reported);
        assert_eq!(totals.tokens, counts(100, 100));
        assert_eq!(totals.cost_usd, Some(0.5));
        assert_eq!(totals.request_count, 1);
    }

    #[test]
    fn empty_run_total_is_ignored() {
        let mut tracker = UsageTracker::new();
        tracker.apply(request(Some("m1"), counts(10, 10)));
        tracker.apply(request(Some("m2"), counts(1, 2)));
        tracker.apply(UsageEvent::RunTotal(RunTotalUsage {
            tokens: TokenCounts::default(),
            cost_usd: Some(0.5),
        }));
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.totals.status, UsageStatus::Partial);
        assert_eq!(snap.totals.tokens, counts(11, 12));
        assert_eq!(snap.totals.cost_usd, None);
        assert_eq!(snap.version, 2);
    }

    #[test]
    fn normalized_events_round_trip_as_json() {
        let event: UsageEvent = serde_json::from_value(serde_json::json!({
            "kind": "request",
            "id": "r1",
            "model": "m",
            "tokens": { "input": 1, "output": 2 }
        }))
        .unwrap();
        assert_eq!(
            event,
            UsageEvent::Request(RequestUsage {
                id: Some("r1".into()),
                granularity: Granularity::Request,
                model: Some("m".into()),
                tokens: counts(1, 2),
            })
        );
    }
}
