use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

pub const MAX_KNOWLEDGE_TITLE_CHARS: usize = 160;
pub const MAX_KNOWLEDGE_CONTENT_CHARS: usize = 12_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeScope {
    Workspace,
    Project,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeType {
    CodingConvention,
    ArchitectureRule,
    BugPattern,
    TestCommand,
    ReviewFeedback,
    DependencyNote,
    ApiContract,
    WorkflowRule,
    HumanPreference,
    OperationalRunbook,
    SecurityRule,
    PerformanceNote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeSourceType {
    Ticket,
    Comment,
    Review,
    HumanNote,
    AgentSummary,
    WorkspaceSignal,
    ObservationRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeConfidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeStatus {
    Pending,
    Approved,
    Rejected,
    Stale,
}

#[derive(Debug, Clone)]
pub struct KnowledgeRevisionInput {
    pub scope: KnowledgeScope,
    pub project_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub knowledge_type: KnowledgeType,
    pub title: String,
    pub content: String,
    pub source_type: KnowledgeSourceType,
    pub source_id: Option<Uuid>,
    pub source_run_id: Option<Uuid>,
    pub confidence: KnowledgeConfidence,
}

#[derive(Debug, Clone)]
pub struct KnowledgeItemView {
    pub id: Uuid,
    pub version: i32,
    pub status: KnowledgeStatus,
    pub revision_id: Uuid,
    pub revision_number: i32,
    pub active_revision_id: Option<Uuid>,
    pub scope: KnowledgeScope,
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
    pub agent_id: Option<Uuid>,
    pub agent_name: Option<String>,
    pub knowledge_type: KnowledgeType,
    pub title: String,
    pub content: String,
    pub source_type: KnowledgeSourceType,
    pub source_id: Option<Uuid>,
    pub source_run_id: Option<Uuid>,
    pub confidence: KnowledgeConfidence,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<OffsetDateTime>,
    pub approval_mode: Option<String>,
    pub policy_decision: Option<String>,
    pub policy_reason: Option<String>,
    pub rejection_reason: Option<String>,
    pub expires_at: Option<OffsetDateTime>,
    pub supersedes_item_id: Option<Uuid>,
    pub superseded_by: Option<Uuid>,
    pub stale_at: Option<OffsetDateTime>,
    pub embedding_status: String,
    pub embedding_error: Option<String>,
    pub usage_count: i64,
    pub last_used_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

macro_rules! string_enum {
    ($to_fn:ident, $from_fn:ident, $ty:ty, { $($variant:path => $value:literal),+ $(,)? }) => {
        pub fn $to_fn(value: $ty) -> &'static str {
            match value { $($variant => $value),+ }
        }
        pub fn $from_fn(value: &str) -> Option<$ty> {
            match value { $($value => Some($variant),)+ _ => None }
        }
    };
}

string_enum!(scope_to_str, scope_from_str, KnowledgeScope, {
    KnowledgeScope::Workspace => "workspace",
    KnowledgeScope::Project => "project",
    KnowledgeScope::Agent => "agent",
});
string_enum!(type_to_str, type_from_str, KnowledgeType, {
    KnowledgeType::CodingConvention => "coding_convention",
    KnowledgeType::ArchitectureRule => "architecture_rule",
    KnowledgeType::BugPattern => "bug_pattern",
    KnowledgeType::TestCommand => "test_command",
    KnowledgeType::ReviewFeedback => "review_feedback",
    KnowledgeType::DependencyNote => "dependency_note",
    KnowledgeType::ApiContract => "api_contract",
    KnowledgeType::WorkflowRule => "workflow_rule",
    KnowledgeType::HumanPreference => "human_preference",
    KnowledgeType::OperationalRunbook => "operational_runbook",
    KnowledgeType::SecurityRule => "security_rule",
    KnowledgeType::PerformanceNote => "performance_note",
});
string_enum!(source_type_to_str, source_type_from_str, KnowledgeSourceType, {
    KnowledgeSourceType::Ticket => "ticket",
    KnowledgeSourceType::Comment => "comment",
    KnowledgeSourceType::Review => "review",
    KnowledgeSourceType::HumanNote => "human_note",
    KnowledgeSourceType::AgentSummary => "agent_summary",
    KnowledgeSourceType::WorkspaceSignal => "workspace_signal",
    KnowledgeSourceType::ObservationRun => "observation_run",
});
string_enum!(confidence_to_str, confidence_from_str, KnowledgeConfidence, {
    KnowledgeConfidence::Low => "low",
    KnowledgeConfidence::Medium => "medium",
    KnowledgeConfidence::High => "high",
});
string_enum!(status_to_str, status_from_str, KnowledgeStatus, {
    KnowledgeStatus::Pending => "pending",
    KnowledgeStatus::Approved => "approved",
    KnowledgeStatus::Rejected => "rejected",
    KnowledgeStatus::Stale => "stale",
});

pub fn validate_revision(input: &mut KnowledgeRevisionInput) -> Result<(), String> {
    input.title = input.title.trim().to_string();
    input.content = input.content.trim().to_string();
    let title_len = input.title.chars().count();
    if !(1..=MAX_KNOWLEDGE_TITLE_CHARS).contains(&title_len) {
        return Err(format!(
            "title must contain 1 to {MAX_KNOWLEDGE_TITLE_CHARS} characters"
        ));
    }
    let content_len = input.content.chars().count();
    if !(1..=MAX_KNOWLEDGE_CONTENT_CHARS).contains(&content_len) {
        return Err(format!(
            "content must contain 1 to {MAX_KNOWLEDGE_CONTENT_CHARS} characters"
        ));
    }
    match input.scope {
        KnowledgeScope::Workspace if input.project_id.is_none() && input.agent_id.is_none() => {}
        KnowledgeScope::Project if input.project_id.is_some() && input.agent_id.is_none() => {}
        KnowledgeScope::Agent if input.project_id.is_some() && input.agent_id.is_some() => {}
        KnowledgeScope::Workspace => {
            return Err("workspace scope cannot set projectId or agentId".into())
        }
        KnowledgeScope::Project => {
            return Err("project scope requires projectId and forbids agentId".into())
        }
        KnowledgeScope::Agent => {
            return Err("agent scope requires both projectId and agentId".into())
        }
    }
    Ok(())
}

pub fn is_high_impact(knowledge_type: KnowledgeType) -> bool {
    matches!(
        knowledge_type,
        KnowledgeType::ArchitectureRule
            | KnowledgeType::ApiContract
            | KnowledgeType::HumanPreference
            | KnowledgeType::OperationalRunbook
            | KnowledgeType::SecurityRule
            | KnowledgeType::WorkflowRule
    )
}

pub fn confidence_rank(confidence: KnowledgeConfidence) -> u8 {
    match confidence {
        KnowledgeConfidence::Low => 0,
        KnowledgeConfidence::Medium => 1,
        KnowledgeConfidence::High => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input() -> KnowledgeRevisionInput {
        KnowledgeRevisionInput {
            scope: KnowledgeScope::Project,
            project_id: Some(Uuid::new_v4()),
            agent_id: None,
            knowledge_type: KnowledgeType::TestCommand,
            title: " Run tests ".into(),
            content: " make test-unit ".into(),
            source_type: KnowledgeSourceType::HumanNote,
            source_id: None,
            source_run_id: None,
            confidence: KnowledgeConfidence::High,
        }
    }

    #[test]
    fn validates_and_normalizes_revision() {
        let mut input = valid_input();
        validate_revision(&mut input).unwrap();
        assert_eq!(input.title, "Run tests");
        assert_eq!(input.content, "make test-unit");
    }

    #[test]
    fn rejects_scope_mismatch() {
        let mut input = valid_input();
        input.scope = KnowledgeScope::Workspace;
        assert!(validate_revision(&mut input).is_err());
    }

    #[test]
    fn security_rules_are_always_high_impact() {
        assert!(is_high_impact(KnowledgeType::SecurityRule));
        assert!(!is_high_impact(KnowledgeType::TestCommand));
    }
}
