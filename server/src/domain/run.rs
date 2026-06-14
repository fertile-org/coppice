use crate::domain::context_profile::ContextProfile;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct AgentRun {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub agent_id: Uuid,
    pub job_type: String,
    pub status: RunStatus,
    pub sandbox_profile_id: String,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    pub error_message: Option<String>,
    pub session_id: Option<String>,
    pub context_profile: ContextProfile,
    pub trigger_comment_id: Option<Uuid>,
    pub started_at: Option<OffsetDateTime>,
    pub ended_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

pub fn run_status_to_str(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::Succeeded => "succeeded",
        RunStatus::Failed => "failed",
        RunStatus::Blocked => "blocked",
        RunStatus::Cancelled => "cancelled",
    }
}

pub fn run_status_from_str(s: &str) -> Option<RunStatus> {
    match s {
        "queued" => Some(RunStatus::Queued),
        "running" => Some(RunStatus::Running),
        "succeeded" => Some(RunStatus::Succeeded),
        "failed" => Some(RunStatus::Failed),
        "blocked" => Some(RunStatus::Blocked),
        "cancelled" => Some(RunStatus::Cancelled),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_status_roundtrip() {
        let statuses = [
            RunStatus::Queued,
            RunStatus::Running,
            RunStatus::Succeeded,
            RunStatus::Failed,
            RunStatus::Blocked,
            RunStatus::Cancelled,
        ];
        for status in statuses {
            assert_eq!(
                run_status_from_str(run_status_to_str(status)),
                Some(status)
            );
        }
    }
}
