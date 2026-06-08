use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    Ready,
    PathMissing,
    NotGitRepo,
    Error,
}

#[derive(Debug, Clone)]
pub struct Repo {
    pub id: Uuid,
    pub name: String,
    pub local_path: String,
    pub remote_url: Option<String>,
    pub default_branch: String,
    pub verification_status: VerificationStatus,
    pub verification_error: Option<String>,
    pub last_verified_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

pub fn verification_status_to_str(status: VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::Ready => "ready",
        VerificationStatus::PathMissing => "path_missing",
        VerificationStatus::NotGitRepo => "not_git_repo",
        VerificationStatus::Error => "error",
    }
}

pub fn verification_status_from_str(s: &str) -> Option<VerificationStatus> {
    match s {
        "ready" => Some(VerificationStatus::Ready),
        "path_missing" => Some(VerificationStatus::PathMissing),
        "not_git_repo" => Some(VerificationStatus::NotGitRepo),
        "error" => Some(VerificationStatus::Error),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_status_roundtrip() {
        let statuses = [
            VerificationStatus::Ready,
            VerificationStatus::PathMissing,
            VerificationStatus::NotGitRepo,
            VerificationStatus::Error,
        ];
        for status in statuses {
            assert_eq!(
                verification_status_from_str(verification_status_to_str(status)),
                Some(status)
            );
        }
    }
}
