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
    let max_body = TICKET_THREAD_MAX.saturating_sub(overhead);

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
}
