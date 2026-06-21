use crate::domain::notification::{Notification, NotificationType};
use sqlx::PgPool;
use sqlx::Row;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;

pub struct NotificationService<'a> {
    pool: &'a PgPool,
}

struct FanOutInput<'a> {
    kind: NotificationType,
    title: &'a str,
    body: Option<&'a str>,
    ticket_id: Option<Uuid>,
    run_id: Option<Uuid>,
    agent_id: Option<Uuid>,
    comment_id: Option<Uuid>,
    mention_id: Option<Uuid>,
    source_key: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("notification not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Oldest-first page of notifications for a recipient, plus the opaque cursor
/// the client should pass to fetch the next page (or `None` at the end).
pub struct NotificationPage {
    pub items: Vec<Notification>,
    pub next_cursor: Option<String>,
}

impl<'a> NotificationService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Fan out a run-finished notification to every workspace user. Idempotent
    /// per `(recipient, run_id)`: re-publishing the same finished run will not
    /// create duplicate rows. Only the four terminal statuses produce rows.
    pub async fn create_for_run_finished(
        &self,
        run_id: Uuid,
        ticket_id: Uuid,
        agent_id: Uuid,
        status: &str,
    ) -> Result<Vec<Uuid>, NotificationError> {
        if !matches!(
            status,
            "succeeded" | "blocked" | "failed" | "cancelled"
        ) {
            return Ok(Vec::new());
        }

        let agent_name: Option<String> = sqlx::query_scalar(
            "SELECT name FROM agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_optional(self.pool)
        .await?;

        let ticket_title: Option<String> = sqlx::query_scalar(
            "SELECT title FROM tickets WHERE id = $1",
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;

        let agent_label = agent_name
            .as_deref()
            .unwrap_or("Agent")
            .to_string();
        let title = format!("{agent_label} run {status}");
        let body = ticket_title;

        let source_key = format!("agent_run_finished:{run_id}");

        self.fan_out(FanOutInput {
            kind: NotificationType::AgentRunFinished,
            title: &title,
            body: body.as_deref(),
            ticket_id: Some(ticket_id),
            run_id: Some(run_id),
            agent_id: Some(agent_id),
            comment_id: None,
            mention_id: None,
            source_key: &source_key,
        })
        .await
    }

    /// Fan out an agent-mentioned notification to every workspace user.
    /// Idempotent per `(recipient, mention_id)`.
    pub async fn create_for_agent_mentioned(
        &self,
        mention_id: Uuid,
        ticket_id: Uuid,
        comment_id: Uuid,
        mentioned_agent_id: Uuid,
    ) -> Result<Vec<Uuid>, NotificationError> {
        let agent_name: Option<String> = sqlx::query_scalar(
            "SELECT name FROM agents WHERE id = $1",
        )
        .bind(mentioned_agent_id)
        .fetch_optional(self.pool)
        .await?;

        let ticket_title: Option<String> = sqlx::query_scalar(
            "SELECT title FROM tickets WHERE id = $1",
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;

        let agent_label = agent_name
            .as_deref()
            .unwrap_or("Agent")
            .to_string();
        let title = format!("{agent_label} mentioned on ticket");
        let body = ticket_title;

        let source_key = format!("agent_mentioned:{mention_id}");

        self.fan_out(FanOutInput {
            kind: NotificationType::AgentMentioned,
            title: &title,
            body: body.as_deref(),
            ticket_id: Some(ticket_id),
            run_id: None,
            agent_id: Some(mentioned_agent_id),
            comment_id: Some(comment_id),
            mention_id: Some(mention_id),
            source_key: &source_key,
        })
        .await
    }

    async fn fan_out(&self, input: FanOutInput<'_>) -> Result<Vec<Uuid>, NotificationError> {
        let user_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM users ORDER BY created_at")
                .fetch_all(self.pool)
                .await?;

        let mut created = Vec::new();
        for user_id in user_ids {
            let id = Uuid::new_v4();
            let inserted: Option<Uuid> = sqlx::query_scalar(
                r#"
                INSERT INTO notifications (
                    id, recipient_user_id, type, title, body,
                    ticket_id, run_id, agent_id, comment_id, mention_id,
                    source_key
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                ON CONFLICT (recipient_user_id, source_key) DO NOTHING
                RETURNING id
                "#,
            )
            .bind(id)
            .bind(user_id)
            .bind(input.kind.as_str())
            .bind(input.title)
            .bind(input.body)
            .bind(input.ticket_id)
            .bind(input.run_id)
            .bind(input.agent_id)
            .bind(input.comment_id)
            .bind(input.mention_id)
            .bind(input.source_key)
            .fetch_optional(self.pool)
            .await?;

            if let Some(id) = inserted {
                created.push(id);
            }
        }

        Ok(created)
    }

    /// Newest-first listing with keyset pagination over `(created_at DESC, id DESC)`.
    pub async fn list_for_user(
        &self,
        recipient_user_id: Uuid,
        filter: NotificationFilter,
        limit: Option<i64>,
        cursor: Option<&str>,
    ) -> Result<NotificationPage, NotificationError> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let fetch = limit + 1;

        let cursor = cursor.map(decode_cursor);

        let rows = if let Some((ts, id)) = cursor {
            sqlx::query(
                r#"
                SELECT
                    id, recipient_user_id, type, title, body,
                    ticket_id, run_id, agent_id, comment_id, mention_id,
                    source_key, read_at, created_at
                FROM notifications
                WHERE recipient_user_id = $1
                  AND ($2 = 'all' OR read_at IS NULL)
                  AND (created_at, id) < ($3, $4)
                ORDER BY created_at DESC, id DESC
                LIMIT $5
                "#,
            )
            .bind(recipient_user_id)
            .bind(filter.as_db_str())
            .bind(ts)
            .bind(id)
            .bind(fetch)
            .fetch_all(self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT
                    id, recipient_user_id, type, title, body,
                    ticket_id, run_id, agent_id, comment_id, mention_id,
                    source_key, read_at, created_at
                FROM notifications
                WHERE recipient_user_id = $1
                  AND ($2 = 'all' OR read_at IS NULL)
                ORDER BY created_at DESC, id DESC
                LIMIT $3
                "#,
            )
            .bind(recipient_user_id)
            .bind(filter.as_db_str())
            .bind(fetch)
            .fetch_all(self.pool)
            .await?
        };

        let mut items: Vec<Notification> = rows.iter().map(row_to_notification).collect();
        let has_more = items.len() as i64 > limit;
        if has_more {
            items.truncate(limit as usize);
        }

        let next_cursor = if has_more {
            items.last().map(|n| encode_cursor(n.created_at, n.id))
        } else {
            None
        };

        Ok(NotificationPage {
            items,
            next_cursor,
        })
    }

    pub async fn unread_count(
        &self,
        recipient_user_id: Uuid,
    ) -> Result<i64, NotificationError> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM notifications
            WHERE recipient_user_id = $1 AND read_at IS NULL
            "#,
        )
        .bind(recipient_user_id)
        .fetch_one(self.pool)
        .await?;
        Ok(count)
    }

    /// Mark a single notification read. Only succeeds for notifications owned
    /// by `recipient_user_id`; other users' notifications are treated as not
    /// found so callers can return a 404 without leaking existence.
    pub async fn mark_read(
        &self,
        notification_id: Uuid,
        recipient_user_id: Uuid,
    ) -> Result<(), NotificationError> {
        let result = sqlx::query(
            r#"
            UPDATE notifications
            SET read_at = COALESCE(read_at, now())
            WHERE id = $1 AND recipient_user_id = $2 AND read_at IS NULL
            "#,
        )
        .bind(notification_id)
        .bind(recipient_user_id)
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            // Distinguish "not found / not yours" from "already read": an
            // already-read row is not an error.
            let exists: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM notifications
                    WHERE id = $1 AND recipient_user_id = $2
                )
                "#,
            )
            .bind(notification_id)
            .bind(recipient_user_id)
            .fetch_one(self.pool)
            .await?;

            if !exists {
                return Err(NotificationError::NotFound);
            }
        }

        Ok(())
    }

    pub async fn mark_all_read(
        &self,
        recipient_user_id: Uuid,
    ) -> Result<u64, NotificationError> {
        let result = sqlx::query(
            r#"
            UPDATE notifications
            SET read_at = COALESCE(read_at, now())
            WHERE recipient_user_id = $1 AND read_at IS NULL
            "#,
        )
        .bind(recipient_user_id)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum NotificationFilter {
    All,
    Unread,
}

impl NotificationFilter {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::to_ascii_lowercase).as_deref() {
            Some("all") => NotificationFilter::All,
            _ => NotificationFilter::Unread,
        }
    }

    fn as_db_str(self) -> &'static str {
        match self {
            NotificationFilter::All => "all",
            NotificationFilter::Unread => "unread",
        }
    }
}

fn encode_cursor(ts: OffsetDateTime, id: Uuid) -> String {
    let rfc = ts.format(&Rfc3339).unwrap_or_default();
    let raw = format!("{rfc}|{id}");
    hex::encode(raw.as_bytes())
}

fn decode_cursor(raw: &str) -> (OffsetDateTime, Uuid) {
    let decoded = hex::decode(raw.as_bytes()).ok();
    let text = decoded
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    let (ts_str, id_str) = text.split_once('|').unwrap_or(("", ""));
    let ts = OffsetDateTime::parse(ts_str, &Rfc3339).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    let id = Uuid::parse_str(id_str).unwrap_or_else(|_| Uuid::nil());
    (ts, id)
}

fn row_to_notification(row: &sqlx::postgres::PgRow) -> Notification {
    let type_str: String = row.get("type");
    let kind = NotificationType::parse_str(&type_str)
        .unwrap_or(NotificationType::AgentRunFinished);

    Notification {
        id: row.get("id"),
        recipient_user_id: row.get("recipient_user_id"),
        kind,
        title: row.get("title"),
        body: row.get("body"),
        ticket_id: row.get("ticket_id"),
        run_id: row.get("run_id"),
        agent_id: row.get("agent_id"),
        comment_id: row.get("comment_id"),
        mention_id: row.get("mention_id"),
        source_key: row.get("source_key"),
        read_at: row.get("read_at"),
        created_at: row.get("created_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_roundtrip() {
        let ts = OffsetDateTime::now_utc();
        let id = Uuid::new_v4();
        let encoded = encode_cursor(ts, id);
        let (dec_ts, dec_id) = decode_cursor(&encoded);
        assert_eq!(dec_id, id);
        assert_eq!(dec_ts.unix_timestamp(), ts.unix_timestamp());
    }

    #[test]
    fn decode_garbage_cursor_yields_epoch_and_nil() {
        let (ts, id) = decode_cursor("not-real-hex");
        assert_eq!(id, Uuid::nil());
        assert_eq!(ts, OffsetDateTime::UNIX_EPOCH);
    }

    #[test]
    fn filter_defaults_to_unread() {
        assert!(matches!(
            NotificationFilter::parse(None),
            NotificationFilter::Unread
        ));
        assert!(matches!(
            NotificationFilter::parse(Some("all")),
            NotificationFilter::All
        ));
        assert!(matches!(
            NotificationFilter::parse(Some("ALL")),
            NotificationFilter::All
        ));
        assert!(matches!(
            NotificationFilter::parse(Some("unread")),
            NotificationFilter::Unread
        ));
        assert!(matches!(
            NotificationFilter::parse(Some("garbage")),
            NotificationFilter::Unread
        ));
    }
}
