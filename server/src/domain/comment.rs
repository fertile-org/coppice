use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorType {
    Human,
    Agent,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentIntent {
    ProgressUpdate,
    ClarificationRequest,
    ClarificationAnswer,
    ReviewFeedback,
    BugReport,
    ImplementationDone,
    QaFailed,
    QaPassed,
    Blocked,
    SystemEvent,
}

#[derive(Debug, Clone)]
pub struct Comment {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub author_type: AuthorType,
    pub author_id: Option<Uuid>,
    pub body: String,
    pub intent: CommentIntent,
    pub mentions: serde_json::Value,
    pub attachment_ids: Vec<Uuid>,
    pub created_at: OffsetDateTime,
}

pub fn author_type_to_str(author_type: AuthorType) -> &'static str {
    match author_type {
        AuthorType::Human => "human",
        AuthorType::Agent => "agent",
        AuthorType::System => "system",
    }
}

pub fn author_type_from_str(s: &str) -> Option<AuthorType> {
    match s {
        "human" => Some(AuthorType::Human),
        "agent" => Some(AuthorType::Agent),
        "system" => Some(AuthorType::System),
        _ => None,
    }
}

pub fn intent_to_str(intent: CommentIntent) -> &'static str {
    match intent {
        CommentIntent::ProgressUpdate => "progress_update",
        CommentIntent::ClarificationRequest => "clarification_request",
        CommentIntent::ClarificationAnswer => "clarification_answer",
        CommentIntent::ReviewFeedback => "review_feedback",
        CommentIntent::BugReport => "bug_report",
        CommentIntent::ImplementationDone => "implementation_done",
        CommentIntent::QaFailed => "qa_failed",
        CommentIntent::QaPassed => "qa_passed",
        CommentIntent::Blocked => "blocked",
        CommentIntent::SystemEvent => "system_event",
    }
}

pub fn intent_from_str(s: &str) -> Option<CommentIntent> {
    match s {
        "progress_update" => Some(CommentIntent::ProgressUpdate),
        "clarification_request" => Some(CommentIntent::ClarificationRequest),
        "clarification_answer" => Some(CommentIntent::ClarificationAnswer),
        "review_feedback" => Some(CommentIntent::ReviewFeedback),
        "bug_report" => Some(CommentIntent::BugReport),
        "implementation_done" => Some(CommentIntent::ImplementationDone),
        "qa_failed" => Some(CommentIntent::QaFailed),
        "qa_passed" => Some(CommentIntent::QaPassed),
        "blocked" => Some(CommentIntent::Blocked),
        "system_event" => Some(CommentIntent::SystemEvent),
        _ => None,
    }
}
