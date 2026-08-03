use crate::config::KnowledgeConfig;
use crate::domain::knowledge::{
    is_high_impact, type_to_str, KnowledgeConfidence, KnowledgeSourceType, KnowledgeStatus,
    KnowledgeType,
};
use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ExtractionInput {
    pub ticket_id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub description: String,
    pub comments: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ExtractedCandidate {
    pub knowledge_type: KnowledgeType,
    pub title: String,
    pub content: String,
    pub confidence: KnowledgeConfidence,
    pub should_require_human_approval: bool,
    pub source_type: KnowledgeSourceType,
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
        let evidence = input
            .comments
            .iter()
            .rev()
            .find(|comment| !comment.trim().is_empty())
            .map(String::as_str)
            .unwrap_or(&input.description);
        let content = if evidence.trim().is_empty() {
            format!("Completed ticket: {title}")
        } else {
            evidence.trim().chars().take(4_000).collect()
        };
        Ok(vec![ExtractedCandidate {
            knowledge_type: KnowledgeType::ReviewFeedback,
            title: format!("Outcome: {title}").chars().take(160).collect(),
            content,
            confidence: KnowledgeConfidence::High,
            should_require_human_approval: false,
            source_type: KnowledgeSourceType::AgentSummary,
        }])
    }
}

pub fn policy_decision(
    config: &KnowledgeConfig,
    candidate: &ExtractedCandidate,
) -> (KnowledgeStatus, &'static str, String) {
    if is_high_impact(candidate.knowledge_type) {
        return (
            KnowledgeStatus::Pending,
            "human_review",
            "high-impact type always requires human approval".into(),
        );
    }
    if candidate.should_require_human_approval {
        return (
            KnowledgeStatus::Pending,
            "human_review",
            "extractor requested human approval".into(),
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
            "auto-save policy did not explicitly allow this candidate".into(),
        )
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
        }
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
}
