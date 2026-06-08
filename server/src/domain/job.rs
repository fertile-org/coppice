use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Processing,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct AgentJob {
    pub id: Uuid,
    pub run_id: Uuid,
    pub job_type: String,
    pub status: JobStatus,
    pub attempts: i32,
    pub max_attempts: i32,
    pub available_at: OffsetDateTime,
    pub locked_at: Option<OffsetDateTime>,
    pub locked_by: Option<String>,
    pub created_at: OffsetDateTime,
}

pub fn job_status_to_str(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Pending => "pending",
        JobStatus::Processing => "processing",
        JobStatus::Done => "done",
        JobStatus::Failed => "failed",
        JobStatus::Cancelled => "cancelled",
    }
}

pub fn job_status_from_str(s: &str) -> Option<JobStatus> {
    match s {
        "pending" => Some(JobStatus::Pending),
        "processing" => Some(JobStatus::Processing),
        "done" => Some(JobStatus::Done),
        "failed" => Some(JobStatus::Failed),
        "cancelled" => Some(JobStatus::Cancelled),
        _ => None,
    }
}
