//! How a CLI run ends, shared by the stream-JSON connectors.
//!
//! A clean exit is not proof of success: a run needs a terminal event, and an
//! error flag or failure reason on that event fails it. Every failure carries
//! the CLI's stderr, which is often the only place the cause is. Failures that
//! may still have a result from `result_submit` are `MissingResult`, so that
//! result wins (`prefer_submitted_result`).

use super::{AgentRunResult, ProviderError};
use crate::domain::context_profile::ContextProfile;
use crate::sessions::opencode_events::extract_result_from_text;

/// Longest stderr text added to one failure message.
const STDERR_MAX: usize = 600;

/// ` (stderr: a | b)`, or empty when stderr had nothing.
pub fn stderr_suffix(stderr: &[String]) -> String {
    let joined = stderr
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    if joined.is_empty() {
        String::new()
    } else {
        format!(
            " (stderr: {})",
            crate::util::truncate::truncate_with_ellipsis(&joined, STDERR_MAX)
        )
    }
}

pub fn exited(name: &str, status: &std::process::ExitStatus, stderr: &[String]) -> ProviderError {
    ProviderError::InvalidFixture(format!(
        "{name} exited with {status} before finishing{}",
        stderr_suffix(stderr)
    ))
}

pub fn timed_out(name: &str, secs: u64, stderr: &[String]) -> ProviderError {
    ProviderError::InvalidFixture(format!(
        "{name} run timed out after {secs}s{}",
        stderr_suffix(stderr)
    ))
}

/// The terminal event said the turn failed.
pub fn error_result(name: &str, detail: Option<&str>, stderr: &[String]) -> ProviderError {
    let detail = detail.map(str::trim).filter(|d| !d.is_empty());
    ProviderError::MissingResult(match detail {
        Some(detail) => format!(
            "{name} ended with an error: {detail}{}",
            stderr_suffix(stderr)
        ),
        None => format!(
            "{name} ended with an error and gave no details{}",
            stderr_suffix(stderr)
        ),
    })
}

/// The result from the final text. A chat turn whose agent answered in plain
/// text without calling `result_submit` keeps that text as the reply; a chat
/// reply needs no structure, so losing it helps nobody. Other runs need the
/// structured fields and fail with `why_missing`.
pub fn result_from_text(
    final_text: &str,
    profile: ContextProfile,
    name: &str,
    why_missing: impl FnOnce() -> String,
) -> Result<AgentRunResult, ProviderError> {
    if let Some(result) = extract_result_from_text(final_text) {
        return Ok(result);
    }
    let reply = final_text.trim();
    if profile == ContextProfile::Conversation && !reply.is_empty() {
        tracing::info!(
            connector = name,
            "chat reply arrived as plain text without result_submit"
        );
        return Ok(plain_reply(reply));
    }
    Err(ProviderError::MissingResult(why_missing()))
}

fn plain_reply(text: &str) -> AgentRunResult {
    serde_json::from_value(serde_json::json!({ "status": "done", "summary": text }))
        .expect("a done result with a summary deserializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_is_joined_trimmed_and_capped() {
        assert_eq!(stderr_suffix(&[]), "");
        assert_eq!(stderr_suffix(&["  ".into()]), "");
        assert_eq!(
            stderr_suffix(&["first ".into(), "".into(), "second".into()]),
            " (stderr: first | second)"
        );
        let long = stderr_suffix(&["x".repeat(2000)]);
        assert!(long.len() < 700, "{}", long.len());
    }

    #[test]
    fn error_results_are_missing_result_so_a_submission_still_wins() {
        let err = error_result(
            "Claude Code",
            Some("API Error: 529 overloaded"),
            &["boom".into()],
        );
        match err {
            ProviderError::MissingResult(msg) => {
                assert_eq!(
                    msg,
                    "Claude Code ended with an error: API Error: 529 overloaded (stderr: boom)"
                )
            }
            other => panic!("{other:?}"),
        }
        let err = error_result("Codex", None, &[]);
        assert_eq!(
            err.to_string(),
            "no result submitted: Codex ended with an error and gave no details"
        );
    }

    #[test]
    fn chat_keeps_a_plain_text_reply_and_other_runs_do_not() {
        let reply = result_from_text(
            "Hi! How can I help?",
            ContextProfile::Conversation,
            "x",
            || "none".into(),
        )
        .expect("chat reply");
        match reply {
            AgentRunResult::Done { summary, .. } => assert_eq!(summary, "Hi! How can I help?"),
            other => panic!("{other:?}"),
        }
        let err = result_from_text("Hi!", ContextProfile::Full, "x", || "no contract".into())
            .unwrap_err();
        assert_eq!(err.to_string(), "no result submitted: no contract");
        let err = result_from_text("   ", ContextProfile::Conversation, "x", || "empty".into())
            .unwrap_err();
        assert_eq!(err.to_string(), "no result submitted: empty");
    }

    #[test]
    fn a_contract_in_the_text_wins_over_the_plain_reply() {
        let result = result_from_text(
            r#"Here you go: {"status":"done","summary":"from the contract","changedFiles":["a.rs"]}"#,
            ContextProfile::Conversation,
            "x",
            || "none".into(),
        )
        .expect("parsed");
        match result {
            AgentRunResult::Done { summary, changed_files, .. } => {
                assert_eq!(summary, "from the contract");
                assert_eq!(changed_files, ["a.rs"]);
            }
            other => panic!("{other:?}"),
        }
    }
}
