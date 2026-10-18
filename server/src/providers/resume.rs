//! When a failed resume means the session itself is unusable.
//!
//! Starting over throws away the agent's memory of the conversation, so this is
//! positive evidence only: a message that names a session and says it is gone,
//! unknown, or tied to another account. Network errors, rate limits, quota,
//! provider outages, and sign-in problems are not matched. They keep the
//! session so a later turn can resume it.

/// Phrases that mean the session is gone, each as a CLI prints it.
const UNUSABLE_PHRASES: &[&str] = &[
    // Claude Code, when `--resume` names a session it has no transcript for.
    "no conversation found",
    "no saved session found",
    "no session found",
    "unknown session",
    "invalid session",
    // The session belongs to a different provider account.
    "bound to another account",
    "bound to a different account",
    "已绑定另外",
];

/// Said after the word "session" (with or without an id between).
const UNUSABLE_VERDICTS: &[&str] = &[
    "not found",
    "does not exist",
    "doesn't exist",
    "no longer exists",
];

pub fn session_is_unusable(message: &str) -> bool {
    let lower = message.to_lowercase();
    if UNUSABLE_PHRASES.iter().any(|phrase| lower.contains(phrase)) {
        return true;
    }
    lower.match_indices("session").any(|(at, _)| {
        // The verdict has to follow the noun closely, so "session" in one
        // sentence and "not found" in another about a file don't match.
        let after = &lower[at..];
        let sentence_end = after
            .find(['.', ';', '\n', '!', '?'])
            .unwrap_or(after.len());
        let mut end = sentence_end.min(60);
        while !after.is_char_boundary(end) {
            end -= 1;
        }
        let window = &after[..end];
        UNUSABLE_VERDICTS
            .iter()
            .any(|verdict| window.contains(verdict))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_sessions_that_are_gone() {
        for message in [
            "No conversation found with session ID: 1234",
            "Session not found",
            "session ses_abc does not exist",
            "No session found with id ses_abc",
            "Invalid session identifier \"27d8031c\"",
            "unknown session 42",
            "No saved session found with ID 9. Run `qwen --resume` without an ID",
            "400 此 session 已绑定另外的ai账号",
            "This session is bound to another account",
            "opencode session ses_1 not found",
        ] {
            assert!(session_is_unusable(message), "{message}");
        }
    }

    #[test]
    fn leaves_failures_a_new_session_does_not_cure() {
        for message in [
            "connection refused",
            "429 Too Many Requests",
            "rate limit exceeded",
            "Your session has expired. Please log in again.",
            "MCP server coppice failed to connect",
            "file not found: src/main.rs",
            "Session started. Later: config file not found in /etc/app/settings/very/long/path/that/goes/on/and/on",
            "provider returned 503",
        ] {
            assert!(!session_is_unusable(message), "{message}");
        }
    }
}
