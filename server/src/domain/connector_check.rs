pub const JOB_TYPE_CONNECTOR_CHECK: &str = "connector_check";

/// Longest stored failure reason, in characters.
pub const MAX_FAILURE_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Queued,
    Running,
    Passed,
    Failed,
}

impl CheckStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "passed" => Some(Self::Passed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

/// Pass rule for a check run that finished without error: both gateway tools
/// were called successfully and the final outcome is `done`. Returns the first
/// applicable failure reason otherwise.
pub fn evaluate_check(
    ticket_get_ok: bool,
    result_submit_ok: bool,
    outcome: &str,
) -> Result<(), String> {
    if !ticket_get_ok {
        return Err("ticket_get was not called".into());
    }
    if !result_submit_ok {
        return Err("result_submit was not called".into());
    }
    if outcome != "done" {
        return Err(format!("result was {outcome}"));
    }
    Ok(())
}

/// Failure text safe to store and return: every `secrets` occurrence is
/// replaced, and the result is capped at `MAX_FAILURE_CHARS`.
pub fn sanitize_failure(text: &str, secrets: &[&str]) -> String {
    let mut out = text.to_string();
    for secret in secrets.iter().filter(|s| !s.is_empty()) {
        out = out.replace(secret, "[redacted]");
    }
    let out = out.trim();
    if out.chars().count() <= MAX_FAILURE_CHARS {
        return out.to_string();
    }
    let mut capped: String = out.chars().take(MAX_FAILURE_CHARS - 1).collect();
    capped.push('…');
    capped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip() {
        for status in [
            CheckStatus::Queued,
            CheckStatus::Running,
            CheckStatus::Passed,
            CheckStatus::Failed,
        ] {
            assert_eq!(CheckStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(CheckStatus::parse("nope"), None);
    }

    #[test]
    fn evaluate_reports_first_failure() {
        assert_eq!(evaluate_check(true, true, "done"), Ok(()));
        assert_eq!(
            evaluate_check(false, false, "blocked"),
            Err("ticket_get was not called".into())
        );
        assert_eq!(
            evaluate_check(true, false, "done"),
            Err("result_submit was not called".into())
        );
        assert_eq!(
            evaluate_check(true, true, "blocked"),
            Err("result was blocked".into())
        );
    }

    #[test]
    fn sanitize_redacts_and_caps() {
        assert_eq!(
            sanitize_failure("token abc123 leaked", &["abc123", ""]),
            "token [redacted] leaked"
        );
        let long = "x".repeat(2000);
        let capped = sanitize_failure(&long, &[]);
        assert_eq!(capped.chars().count(), MAX_FAILURE_CHARS);
        assert!(capped.ends_with('…'));
    }
}
