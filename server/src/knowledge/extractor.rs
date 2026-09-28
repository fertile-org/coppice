//! Deterministic M06 knowledge extraction and fail-closed policy.
//!
//! # Reuse litmus
//!
//! Keep only candidates that look reusable across tickets, still-true, and scoped.
//! Reject vague advice and ticket-outcome logs (`Outcome: …`, `Fixed ticket #N`,
//! `Completed ticket: …`, pure LGTM/run summaries).
//!
//! Prefer omit over demote for clear one-offs. Borderline text may emit with
//! medium/low confidence, `should_require_human_approval`, and a stable
//! `reuse_hint` token folded into `policy_reason` (no schema migration).
//!
//! Human-facing source of truth: `web/src/features/knowledge/curationGuide.ts`
//! (`CURATION_LITMUS` and reject presets). Do not duplicate long UI copy here.

use crate::config::KnowledgeConfig;
use crate::domain::knowledge::{
    is_high_impact, type_to_str, KnowledgeConfidence, KnowledgeSourceType, KnowledgeStatus,
    KnowledgeType,
};
use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

/// Stable machine-readable reuse signals for borderline candidates.
pub const REUSE_HINT_BORDERLINE_ONE_OFF: &str = "borderline_one_off_risk";
pub const REUSE_HINT_BORDERLINE_VAGUE: &str = "borderline_vague";
pub const REUSE_HINT_BORDERLINE_WRONG_SCOPE: &str = "borderline_wrong_scope";

/// Prefer sharpness: mock emits at most this many candidates before worker
/// `max_candidates` truncation.
const MOCK_SHARP_CAP: usize = 2;

#[derive(Debug, Clone)]
pub struct ExtractionComment {
    pub id: Uuid,
    pub body: String,
    pub source_type: KnowledgeSourceType,
}

#[derive(Debug, Clone)]
pub struct ExtractionInput {
    pub ticket_id: Uuid,
    pub board_id: Uuid,
    pub title: String,
    pub description: String,
    pub comments: Vec<ExtractionComment>,
}

#[derive(Debug, Clone)]
pub struct ExtractedCandidate {
    pub knowledge_type: KnowledgeType,
    pub title: String,
    pub content: String,
    pub confidence: KnowledgeConfidence,
    pub should_require_human_approval: bool,
    pub source_type: KnowledgeSourceType,
    pub source_id: Option<Uuid>,
    /// Optional borderline reuse signal; folded into `policy_reason` as
    /// `reuse_hint=<token>` when human review is required.
    pub reuse_hint: Option<String>,
}

#[derive(Debug, Error)]
pub enum ExtractionError {
    #[error("invalid extraction input: {0}")]
    InvalidInput(String),
}

#[async_trait]
pub trait ExtractionProvider: Send + Sync {
    async fn extract(
        &self,
        input: &ExtractionInput,
    ) -> Result<Vec<ExtractedCandidate>, ExtractionError>;
}

#[derive(Default)]
pub struct MockExtractionProvider;

#[async_trait]
impl ExtractionProvider for MockExtractionProvider {
    async fn extract(
        &self,
        input: &ExtractionInput,
    ) -> Result<Vec<ExtractedCandidate>, ExtractionError> {
        let title = input.title.trim();
        if title.is_empty() {
            return Err(ExtractionError::InvalidInput(
                "ticket title is empty".into(),
            ));
        }

        let mut candidates = Vec::new();
        let mut seen_types = Vec::new();

        let mut consider = |snippet: EvidenceSnippet<'_>| {
            if candidates.len() >= MOCK_SHARP_CAP {
                return;
            }
            if let Some(candidate) = classify_snippet(snippet) {
                if seen_types.contains(&candidate.knowledge_type) {
                    return;
                }
                seen_types.push(candidate.knowledge_type);
                candidates.push(candidate);
            }
        };

        for comment in input.comments.iter().rev() {
            let body = comment.body.trim();
            if body.is_empty() {
                continue;
            }
            consider(EvidenceSnippet {
                text: body,
                source_type: comment.source_type,
                source_id: Some(comment.id),
            });
        }

        let description = input.description.trim();
        if !description.is_empty() {
            consider(EvidenceSnippet {
                text: description,
                source_type: KnowledgeSourceType::AgentSummary,
                source_id: None,
            });
        }

        Ok(candidates)
    }
}

#[derive(Clone, Copy)]
struct EvidenceSnippet<'a> {
    text: &'a str,
    source_type: KnowledgeSourceType,
    source_id: Option<Uuid>,
}

fn classify_snippet(snippet: EvidenceSnippet<'_>) -> Option<ExtractedCandidate> {
    let text = snippet.text.trim();
    if text.is_empty() || is_clear_outcome_or_one_off(text) {
        return None;
    }

    if let Some(hint) = borderline_hint(text) {
        let knowledge_type = guess_borderline_type(text);
        return Some(ExtractedCandidate {
            knowledge_type,
            title: truncate_chars(format!("Review: {}", first_line(text)), 160),
            content: truncate_chars(text.to_string(), 4_000),
            confidence: KnowledgeConfidence::Medium,
            should_require_human_approval: true,
            source_type: snippet.source_type,
            source_id: snippet.source_id,
            reuse_hint: Some(hint.to_string()),
        });
    }

    if let Some((knowledge_type, title)) = match_reusable_shape(text) {
        return Some(ExtractedCandidate {
            knowledge_type,
            title: truncate_chars(title, 160),
            content: truncate_chars(text.to_string(), 4_000),
            confidence: KnowledgeConfidence::High,
            should_require_human_approval: false,
            source_type: snippet.source_type,
            source_id: snippet.source_id,
            reuse_hint: None,
        });
    }

    None
}

fn is_clear_outcome_or_one_off(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let trimmed = lower.trim();

    if trimmed.starts_with("outcome:")
        || trimmed.contains("outcome:")
        || trimmed.starts_with("completed ticket:")
        || trimmed.contains("fixed ticket #")
        || trimmed == "lgtm"
        || trimmed.starts_with("lgtm ")
        || trimmed == "looks good to me"
        || trimmed.starts_with("looks good to me")
        || trimmed.contains("ticket moved to **done**")
        || trimmed.contains("ticket moved to done")
        || trimmed.starts_with("final approval:")
        || trimmed.contains("rename this helper")
        || trimmed.contains("re-run the failing test from this run")
        || trimmed.contains("this ticket had a null pointer")
        || trimmed.contains("i liked this diff today")
        || trimmed.contains("move this card to in review")
        || trimmed.contains("restart the container for this outage")
        || trimmed.contains("rotate this leaked token")
        || trimmed.contains("bump this crate for the ticket")
        || trimmed.contains("fix this endpoint for the bug")
        || trimmed.contains("this query was slow once")
    {
        return true;
    }

    // Bare progress / empty-ish evidence with no reusable cue.
    if text.chars().count() < 12 {
        return true;
    }

    false
}

fn borderline_hint(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();

    // Ticket-local framing with rule-ish language → one-off risk.
    let has_ticket_local = lower.contains("this ticket")
        || lower.contains("this pr")
        || lower.contains("in this pr")
        || lower.contains("for now")
        || lower.contains("for this run");
    let has_rule_ish = lower.contains("prefer")
        || lower.contains("always")
        || lower.contains("never")
        || lower.contains("should ")
        || lower.contains("must ");
    if has_ticket_local && has_rule_ish {
        return Some(REUSE_HINT_BORDERLINE_ONE_OFF);
    }

    if lower.contains("everywhere")
        || lower.contains("all boards")
        || lower.contains("all repos")
        || lower.contains("across the entire company")
    {
        return Some(REUSE_HINT_BORDERLINE_WRONG_SCOPE);
    }

    // Vague advice without a concrete reusable fact.
    let vague_markers = [
        "be careful",
        "improve quality",
        "write better code",
        "clean this up",
        "do it properly",
        "make it nicer",
        "prefer better names",
        "use good practices",
    ];
    if vague_markers.iter().any(|marker| lower.contains(marker)) {
        return Some(REUSE_HINT_BORDERLINE_VAGUE);
    }

    // Short imperative with no typed shape → vague borderline rather than drop
    // when it looks like advice.
    if text.chars().count() < 40
        && has_rule_ish
        && match_reusable_shape(text).is_none()
        && !is_clear_outcome_or_one_off(text)
    {
        return Some(REUSE_HINT_BORDERLINE_VAGUE);
    }

    None
}

fn guess_borderline_type(text: &str) -> KnowledgeType {
    if let Some((kind, _)) = match_reusable_shape(text) {
        return kind;
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("test") || lower.contains("make ") || lower.contains("cargo ") {
        KnowledgeType::TestCommand
    } else if lower.contains("bug") || lower.contains("null") || lower.contains("causes") {
        KnowledgeType::BugPattern
    } else if lower.contains("review") || lower.contains("cite") {
        KnowledgeType::ReviewFeedback
    } else {
        KnowledgeType::CodingConvention
    }
}

fn match_reusable_shape(text: &str) -> Option<(KnowledgeType, String)> {
    let lower = text.to_ascii_lowercase();

    // test_command — mirror Inbox approve example.
    if lower.contains("make test-unit")
        || lower.contains("make test ")
        || lower.contains("make test\n")
        || lower.contains("cargo test")
        || lower.contains("npm test")
        || lower.contains("pnpm test")
        || (lower.contains("while iterating")
            && (lower.contains("test-unit") || lower.contains("make test")))
        || (lower.starts_with("use make ") && lower.contains("test"))
    {
        return Some((
            KnowledgeType::TestCommand,
            title_from_text(text, "Test command"),
        ));
    }

    // bug_pattern
    if (lower.contains("causes")
        && (lower.contains("403")
            || lower.contains("null")
            || lower.contains("panic")
            || lower.contains("error")
            || lower.contains("bug")))
        || lower.contains("bug pattern:")
        || lower.contains("null csrf")
        || (lower.contains("n+1") && lower.contains("knowledge"))
        || (lower.contains("leads to")
            && (lower.contains("403") || lower.contains("failure") || lower.contains("error")))
    {
        return Some((
            KnowledgeType::BugPattern,
            title_from_text(text, "Bug pattern"),
        ));
    }

    // Durable review rule (not LGTM / ticket outcome).
    if lower.contains("cite line")
        || lower.contains("line ranges in review")
        || lower.contains("review comments should")
        || (lower.contains("in review comments") && lower.contains("cite"))
    {
        return Some((
            KnowledgeType::ReviewFeedback,
            title_from_text(text, "Review rule"),
        ));
    }

    // coding_convention
    if (lower.contains("prefer")
        && (lower.contains(" over ") || lower.contains(" rather than ")))
        || lower.contains("coding convention:")
        || lower.contains("handlers stay thin")
        || (lower.contains("never ")
            && (lower.contains("panic") || lower.contains("unwrap") || lower.contains("expect(")))
        || (lower.contains("always ")
            && (lower.contains("result") || lower.contains("csrf") || lower.contains("validate")))
        || lower.contains("prefer result over panic")
    {
        return Some((
            KnowledgeType::CodingConvention,
            title_from_text(text, "Coding convention"),
        ));
    }

    None
}

fn title_from_text(text: &str, fallback_prefix: &str) -> String {
    let line = first_line(text);
    if line.chars().count() <= 160 {
        line
    } else {
        format!("{fallback_prefix}: {}", truncate_chars(line, 140))
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or(text)
        .trim()
        .trim_start_matches(['-', '*', '#'])
        .trim()
        .to_string()
}

fn truncate_chars(value: String, max: usize) -> String {
    value.chars().take(max).collect()
}

pub fn policy_decision(
    config: &KnowledgeConfig,
    candidate: &ExtractedCandidate,
) -> (KnowledgeStatus, &'static str, String) {
    if is_high_impact(candidate.knowledge_type) {
        return (
            KnowledgeStatus::Pending,
            "human_review",
            fold_reuse_hint(
                "high-impact type always requires human approval",
                candidate.reuse_hint.as_deref(),
            ),
        );
    }
    if candidate.should_require_human_approval {
        return (
            KnowledgeStatus::Pending,
            "human_review",
            fold_reuse_hint(
                "extractor requested human approval",
                candidate.reuse_hint.as_deref(),
            ),
        );
    }
    let allowed = config.auto_save.enabled
        && candidate.confidence == KnowledgeConfidence::High
        && config
            .auto_save
            .allowed_types
            .iter()
            .any(|value| value == type_to_str(candidate.knowledge_type));
    if allowed {
        (
            KnowledgeStatus::Approved,
            "auto_saved",
            "explicit low-risk allowlist and high confidence matched".into(),
        )
    } else {
        (
            KnowledgeStatus::Pending,
            "human_review",
            fold_reuse_hint(
                "auto-save policy did not explicitly allow this candidate",
                candidate.reuse_hint.as_deref(),
            ),
        )
    }
}

fn fold_reuse_hint(base: &str, reuse_hint: Option<&str>) -> String {
    match reuse_hint {
        Some(hint) if !hint.is_empty() => format!("{base}; reuse_hint={hint}"),
        _ => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(kind: KnowledgeType) -> ExtractedCandidate {
        ExtractedCandidate {
            knowledge_type: kind,
            title: "title".into(),
            content: "content".into(),
            confidence: KnowledgeConfidence::High,
            should_require_human_approval: false,
            source_type: KnowledgeSourceType::AgentSummary,
            source_id: None,
            reuse_hint: None,
        }
    }

    fn input_with(title: &str, description: &str, comments: &[&str]) -> ExtractionInput {
        ExtractionInput {
            ticket_id: Uuid::new_v4(),
            board_id: Uuid::new_v4(),
            title: title.into(),
            description: description.into(),
            comments: comments
                .iter()
                .map(|body| ExtractionComment {
                    id: Uuid::new_v4(),
                    body: (*body).into(),
                    source_type: KnowledgeSourceType::Comment,
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn outcome_only_and_one_off_inputs_emit_nothing() {
        let provider = MockExtractionProvider;
        let cases = [
            ("Fix login bug", "", &[] as &[&str]),
            ("Fix login bug", "details", &[]),
            (
                "Ship feature",
                "",
                &["Outcome: Ship feature"],
            ),
            ("Ship feature", "", &["Fixed ticket #42"]),
            ("Ship feature", "", &["Completed ticket: Ship feature"]),
            ("Review PR", "", &["LGTM"]),
            ("Cleanup", "", &["Rename this helper in the PR."]),
        ];
        for (title, description, comments) in cases {
            let extracted = provider
                .extract(&input_with(title, description, comments))
                .await
                .unwrap();
            assert!(
                extracted.is_empty(),
                "expected empty for title={title:?} description={description:?} comments={comments:?}, got {extracted:?}"
            );
            assert!(
                extracted.iter().all(|c| !c.title.starts_with("Outcome:")),
                "must not emit Outcome: junk"
            );
        }
    }

    #[tokio::test]
    async fn vague_borderline_emits_medium_with_reuse_hint() {
        let provider = MockExtractionProvider;
        let extracted = provider
            .extract(&input_with(
                "Polish UI",
                "",
                &["Be careful with edge cases."],
            ))
            .await
            .unwrap();
        assert_eq!(extracted.len(), 1);
        let candidate = &extracted[0];
        assert_eq!(candidate.confidence, KnowledgeConfidence::Medium);
        assert!(candidate.should_require_human_approval);
        assert_eq!(
            candidate.reuse_hint.as_deref(),
            Some(REUSE_HINT_BORDERLINE_VAGUE)
        );
        let config = KnowledgeConfig::default();
        let (status, decision, reason) = policy_decision(&config, candidate);
        assert_eq!(status, KnowledgeStatus::Pending);
        assert_eq!(decision, "human_review");
        assert!(
            reason.contains("reuse_hint=borderline_vague"),
            "reason={reason}"
        );
        assert_ne!(candidate.confidence, KnowledgeConfidence::High);
    }

    #[tokio::test]
    async fn seeded_reusable_shapes_extract_as_typed_candidates() {
        let provider = MockExtractionProvider;
        let extracted = provider
            .extract(&input_with(
                "Hardening APIs",
                "Prefer Result over panic in public APIs.",
                &[
                    "Null CSRF on mutations causes 403s.",
                    "Use make test-unit while iterating.",
                ],
            ))
            .await
            .unwrap();
        assert!(extracted.len() <= MOCK_SHARP_CAP);
        assert_eq!(extracted.len(), 2, "sharp cap keeps newest comment matches first");
        assert_eq!(extracted[0].knowledge_type, KnowledgeType::TestCommand);
        assert_eq!(extracted[0].confidence, KnowledgeConfidence::High);
        assert!(extracted[0].reuse_hint.is_none());
        assert_eq!(extracted[1].knowledge_type, KnowledgeType::BugPattern);
        assert_eq!(extracted[1].confidence, KnowledgeConfidence::High);

        let convention_only = provider
            .extract(&input_with(
                "API style",
                "Prefer Result over panic in public APIs.",
                &[],
            ))
            .await
            .unwrap();
        assert_eq!(convention_only.len(), 1);
        assert_eq!(
            convention_only[0].knowledge_type,
            KnowledgeType::CodingConvention
        );

        let review = provider
            .extract(&input_with(
                "Review hygiene",
                "",
                &["Cite line ranges in review comments."],
            ))
            .await
            .unwrap();
        assert_eq!(review.len(), 1);
        assert_eq!(review[0].knowledge_type, KnowledgeType::ReviewFeedback);
        assert_eq!(review[0].confidence, KnowledgeConfidence::High);
    }

    #[tokio::test]
    async fn empty_title_is_invalid() {
        let provider = MockExtractionProvider;
        let err = provider
            .extract(&input_with("  ", "Prefer Result over panic.", &[]))
            .await
            .unwrap_err();
        assert!(matches!(err, ExtractionError::InvalidInput(_)));
    }

    #[test]
    fn default_policy_is_fail_closed() {
        let config = KnowledgeConfig::default();
        assert_eq!(
            policy_decision(&config, &candidate(KnowledgeType::TestCommand)).0,
            KnowledgeStatus::Pending
        );
    }

    #[test]
    fn explicit_low_risk_allowlist_can_auto_save_but_security_never_does() {
        let mut config = KnowledgeConfig::default();
        config.auto_save.enabled = true;
        config.auto_save.allowed_types = vec!["test_command".into(), "security_rule".into()];
        assert_eq!(
            policy_decision(&config, &candidate(KnowledgeType::TestCommand)).0,
            KnowledgeStatus::Approved
        );
        assert_eq!(
            policy_decision(&config, &candidate(KnowledgeType::SecurityRule)).0,
            KnowledgeStatus::Pending
        );
    }

    #[test]
    fn high_impact_stays_pending_even_with_hint_and_allowlist() {
        let mut config = KnowledgeConfig::default();
        config.auto_save.enabled = true;
        config.auto_save.allowed_types = vec!["security_rule".into(), "test_command".into()];
        let mut high = candidate(KnowledgeType::SecurityRule);
        high.confidence = KnowledgeConfidence::High;
        high.should_require_human_approval = false;
        high.reuse_hint = Some(REUSE_HINT_BORDERLINE_VAGUE.into());
        let (status, decision, reason) = policy_decision(&config, &high);
        assert_eq!(status, KnowledgeStatus::Pending);
        assert_eq!(decision, "human_review");
        assert!(reason.contains("high-impact"));
        assert!(reason.contains("reuse_hint=borderline_vague"));
    }

    #[test]
    fn borderline_never_auto_saves() {
        let mut config = KnowledgeConfig::default();
        config.auto_save.enabled = true;
        config.auto_save.allowed_types = vec!["coding_convention".into()];
        let borderline = ExtractedCandidate {
            knowledge_type: KnowledgeType::CodingConvention,
            title: "Prefer better names".into(),
            content: "Prefer better names".into(),
            confidence: KnowledgeConfidence::Medium,
            should_require_human_approval: true,
            source_type: KnowledgeSourceType::Comment,
            source_id: None,
            reuse_hint: Some(REUSE_HINT_BORDERLINE_VAGUE.into()),
        };
        let (status, decision, reason) = policy_decision(&config, &borderline);
        assert_eq!(status, KnowledgeStatus::Pending);
        assert_eq!(decision, "human_review");
        assert!(reason.contains("reuse_hint=borderline_vague"));
    }
}
