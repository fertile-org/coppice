use std::collections::HashMap;

use crate::domain::comment::{intent_to_str, AuthorType, Comment, CommentIntent};
use crate::util::truncate::truncate_with_ellipsis;
use uuid::Uuid;

pub const TICKET_THREAD_MAX: usize = 4000;
const COMMENT_BODY_MAX: usize = 400;

/// Build a compact chronological summary of ticket comments for agent context.
pub fn format_ticket_thread(
    comments: &[Comment],
    agent_names: &HashMap<Uuid, String>,
) -> Option<String> {
    format_ticket_thread_with_limit(comments, agent_names, TICKET_THREAD_MAX)
}

/// Build a chronological comment section bounded for the caller's token allocation.
pub fn format_ticket_thread_with_limit(
    comments: &[Comment],
    agent_names: &HashMap<Uuid, String>,
    max_chars: usize,
) -> Option<String> {
    let mut relevant: Vec<&Comment> = comments
        .iter()
        .filter(|c| c.intent != CommentIntent::SystemEvent)
        .collect();
    relevant.sort_by_key(|c| c.created_at);

    if relevant.is_empty() {
        return None;
    }

    let mut lines: Vec<String> = Vec::new();
    for comment in relevant {
        let author = author_label(comment, agent_names);
        let intent = intent_to_str(comment.intent).replace('_', " ");
        let body = truncate_with_ellipsis(comment.body.trim(), COMMENT_BODY_MAX);
        lines.push(format!("- **{author}** ({intent}): {body}"));
    }

    let header = "Recent activity on this ticket (oldest first):\n\n";
    let footer = "\n\nRead the full thread in Coppice if a detail is truncated.";
    let overhead = header.len() + footer.len();
    if max_chars <= overhead {
        return None;
    }
    let max_body = max_chars - overhead;

    let mut thread = lines.join("\n");
    if thread.len() > max_body {
        // Keep newest comments when over budget.
        while lines.len() > 1 && lines.join("\n").len() > max_body {
            lines.remove(0);
        }
        thread = lines.join("\n");
        if thread.len() > max_body {
            thread = truncate_with_ellipsis(&thread, max_body);
        }
    }

    Some(format!("{header}{thread}{footer}"))
}

/// Build the most recent durable update from the agent that is continuing the work.
pub fn format_previous_attempt_summary(
    comments: &[Comment],
    agent_id: Uuid,
    agent_names: &HashMap<Uuid, String>,
) -> Option<String> {
    let comment = comments
        .iter()
        .filter(|comment| {
            comment.author_type == AuthorType::Agent
                && comment.author_id == Some(agent_id)
                && comment.intent != CommentIntent::SystemEvent
        })
        .max_by_key(|comment| comment.created_at)?;
    let author = author_label(comment, agent_names);
    let intent = intent_to_str(comment.intent).replace('_', " ");
    let body = comment.body.trim();
    Some(format!(
        "Most recent prior update from **{author}** ({intent}):\n\n{body}"
    ))
}

/// Build a short excerpt of the most recent non-system comments for human chat context.
pub fn format_thread_excerpt(
    comments: &[Comment],
    agent_names: &HashMap<Uuid, String>,
    max_comments: usize,
    max_chars: usize,
) -> Option<String> {
    let mut relevant: Vec<&Comment> = comments
        .iter()
        .filter(|c| c.intent != CommentIntent::SystemEvent)
        .take(max_comments)
        .collect();
    if relevant.is_empty() {
        return None;
    }

    relevant.sort_by_key(|c| c.created_at);

    let mut lines: Vec<String> = Vec::new();
    for comment in relevant {
        let author = author_label(comment, agent_names);
        let intent = intent_to_str(comment.intent).replace('_', " ");
        let body = truncate_with_ellipsis(comment.body.trim(), COMMENT_BODY_MAX);
        lines.push(format!("- **{author}** ({intent}): {body}"));
    }

    let mut thread = lines.join("\n");
    if thread.len() > max_chars {
        while lines.len() > 1 && lines.join("\n").len() > max_chars {
            lines.remove(0);
        }
        thread = lines.join("\n");
        if thread.len() > max_chars {
            thread = truncate_with_ellipsis(&thread, max_chars);
        }
    }

    Some(thread)
}

pub fn author_label(comment: &Comment, agent_names: &HashMap<Uuid, String>) -> String {
    match comment.author_type {
        AuthorType::Human => "Human".into(),
        AuthorType::System => "System".into(),
        AuthorType::Agent => comment
            .author_id
            .and_then(|id| agent_names.get(&id).cloned())
            .unwrap_or_else(|| "Agent".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn sample_comment(body: &str, intent: CommentIntent) -> Comment {
        Comment {
            id: Uuid::new_v4(),
            ticket_id: Uuid::new_v4(),
            author_type: AuthorType::Agent,
            author_id: Some(Uuid::new_v4()),
            body: body.into(),
            intent,
            mentions: serde_json::json!([]),
            attachment_ids: vec![],
            created_at: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn format_ticket_thread_includes_comments() {
        let agent_id = Uuid::new_v4();
        let mut comment = sample_comment("Implemented polling retry.", CommentIntent::ImplementationDone);
        comment.author_id = Some(agent_id);
        let mut names = HashMap::new();
        names.insert(agent_id, "Backend Engineer".into());

        let thread = format_ticket_thread(&[comment], &names).expect("thread");
        assert!(thread.contains("Backend Engineer"));
        assert!(thread.contains("implementation done"));
        assert!(thread.contains("Implemented polling retry"));
    }

    #[test]
    fn format_ticket_thread_skips_system_events() {
        let comment = sample_comment("Run started", CommentIntent::SystemEvent);
        assert!(format_ticket_thread(&[comment], &HashMap::new()).is_none());
    }

    #[test]
    fn previous_attempt_summary_uses_newest_update_from_same_agent() {
        let agent_id = Uuid::new_v4();
        let other_agent_id = Uuid::new_v4();
        let base = OffsetDateTime::now_utc();
        let mut older = sample_comment("older matching update", CommentIntent::ProgressUpdate);
        older.author_id = Some(agent_id);
        older.created_at = base;
        let mut newer = sample_comment("newer matching update", CommentIntent::ProgressUpdate);
        newer.author_id = Some(agent_id);
        newer.created_at = base + time::Duration::seconds(1);
        let mut other = sample_comment("other agent update", CommentIntent::ReviewFeedback);
        other.author_id = Some(other_agent_id);
        other.created_at = base + time::Duration::seconds(2);

        let mut names = HashMap::new();
        names.insert(agent_id, "Backend Engineer".into());
        let summary = format_previous_attempt_summary(
            &[older, newer, other],
            agent_id,
            &names,
        )
        .expect("previous attempt summary");

        assert!(summary.contains("Backend Engineer"));
        assert!(summary.contains("newer matching update"));
        assert!(!summary.contains("older matching update"));
        assert!(!summary.contains("other agent update"));
    }

    #[test]
    fn format_thread_excerpt_limits_comments_and_chars() {
        let agent_id = Uuid::new_v4();
        let mut names = HashMap::new();
        names.insert(agent_id, "Engineer".into());
        let base = OffsetDateTime::now_utc();

        let mut comments: Vec<Comment> = (0..5)
            .map(|i| {
                let mut c = sample_comment(
                    &format!("Chat message #{i} with some padding."),
                    CommentIntent::ProgressUpdate,
                );
                c.author_id = Some(agent_id);
                c.created_at = base + time::Duration::seconds(i);
                c
            })
            .collect();
        comments.reverse();

        let excerpt = format_thread_excerpt(&comments, &names, 3, 800).expect("excerpt");
        assert!(excerpt.contains("Chat message #4"));
        assert!(excerpt.contains("Chat message #2"));
        assert!(!excerpt.contains("Chat message #1"));
        assert!(excerpt.len() <= 800);
    }

    #[test]
    fn format_thread_excerpt_skips_system_events() {
        let comment = sample_comment("System notice", CommentIntent::SystemEvent);
        assert!(format_thread_excerpt(&[comment], &HashMap::new(), 3, 800).is_none());
    }

    #[test]
    fn format_ticket_thread_drops_oldest_when_over_budget() {
        let agent_id = Uuid::new_v4();
        let mut names = HashMap::new();
        names.insert(agent_id, "Engineer".into());

        let comments: Vec<Comment> = (0..50)
            .map(|i| {
                let mut c = sample_comment(
                    &format!("Thread entry #{i:02} with padding text for length."),
                    CommentIntent::ProgressUpdate,
                );
                c.author_id = Some(agent_id);
                c
            })
            .collect();

        let thread = format_ticket_thread(&comments, &names).expect("thread");
        assert!(thread.len() <= TICKET_THREAD_MAX);
        assert!(thread.contains("Thread entry #49"));
        assert!(!thread.contains("Thread entry #00"));
    }

    #[test]
    fn configurable_ticket_thread_limit_can_use_a_larger_context_allocation() {
        let agent_id = Uuid::new_v4();
        let mut names = HashMap::new();
        names.insert(agent_id, "Engineer".into());
        let comments: Vec<Comment> = (0..50)
            .map(|i| {
                let mut comment = sample_comment(
                    &format!(
                        "Expanded thread entry #{i:02} with {}",
                        "padding ".repeat(20)
                    ),
                    CommentIntent::ProgressUpdate,
                );
                comment.author_id = Some(agent_id);
                comment.created_at += time::Duration::seconds(i);
                comment
            })
            .collect();

        let legacy = format_ticket_thread(&comments, &names).expect("legacy thread");
        let expanded = format_ticket_thread_with_limit(&comments, &names, 16_000)
            .expect("expanded thread");
        assert!(legacy.len() <= TICKET_THREAD_MAX);
        assert!(expanded.len() <= 16_000);
        assert!(expanded.len() > legacy.len());
        assert!(expanded.contains("Expanded thread entry #00"));
    }
}
