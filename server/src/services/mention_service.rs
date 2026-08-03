use crate::domain::agent::Agent;
use crate::domain::mention::{MentionStatus, TicketMention};
use crate::domain::slug::slugify;
use crate::services::agent_request::agent_requests_from_comment;
use crate::services::agent_service::AgentService;
use sqlx::PgPool;
use sqlx::Row;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub struct MentionService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum MentionError {
    #[error("mention not found")]
    MentionNotFound,
    #[error(transparent)]
    Agent(#[from] crate::services::agent_service::AgentError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl<'a> MentionService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub fn parse_mention_keys(body: &str, known_keys: &[&str]) -> Vec<String> {
        let known: HashMap<&str, ()> = known_keys.iter().map(|key| (*key, ())).collect();
        let mut found = Vec::new();

        for token in body.split('@').skip(1).filter_map(mention_token) {
            if known.contains_key(token) && !found.iter().any(|key| key == token) {
                found.push(token.to_string());
            }
        }

        found
    }

    pub async fn create_mentions(
        &self,
        ticket_id: Uuid,
        comment_id: Uuid,
        keys: &[String],
        resume_agent_id: Option<Uuid>,
        project_id: Uuid,
    ) -> Result<Vec<TicketMention>, MentionError> {
        let _ = project_id;
        let agent_map = self.build_agent_key_map().await?;
        let mut mentions = Vec::new();
        let mut mentioned_agent_ids = HashSet::new();

        for key in keys {
            let Some(&agent_id) = agent_map.get(key) else {
                continue;
            };
            if !mentioned_agent_ids.insert(agent_id) {
                continue;
            }

            let id = Uuid::new_v4();
            let row = sqlx::query(
                r#"
                INSERT INTO ticket_mentions (
                    id, ticket_id, comment_id, mentioned_agent_id, resume_agent_id, status
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING
                    id, ticket_id, comment_id, mentioned_agent_id, resume_agent_id, status
                "#,
            )
            .bind(id)
            .bind(ticket_id)
            .bind(comment_id)
            .bind(agent_id)
            .bind(resume_agent_id)
            .bind(mention_status_to_str(MentionStatus::Pending))
            .fetch_one(self.pool)
            .await?;

            mentions.push(row_to_mention(&row));
        }

        Ok(mentions)
    }

    pub async fn mark_handled(&self, mention_id: Uuid) -> Result<(), MentionError> {
        let result = sqlx::query(
            r#"
            UPDATE ticket_mentions
            SET status = $2, handled_at = now()
            WHERE id = $1
            "#,
        )
        .bind(mention_id)
        .bind(mention_status_to_str(MentionStatus::Handled))
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MentionError::MentionNotFound);
        }

        Ok(())
    }

    pub async fn get(&self, mention_id: Uuid) -> Result<TicketMention, MentionError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, ticket_id, comment_id, mentioned_agent_id, resume_agent_id, status
            FROM ticket_mentions
            WHERE id = $1
            "#,
        )
        .bind(mention_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(MentionError::MentionNotFound)?;

        Ok(row_to_mention(&row))
    }

    pub async fn mark_ignored(&self, mention_id: Uuid) -> Result<(), MentionError> {
        let result = sqlx::query(
            r#"
            UPDATE ticket_mentions
            SET status = $2, handled_at = now()
            WHERE id = $1
            "#,
        )
        .bind(mention_id)
        .bind(mention_status_to_str(MentionStatus::Ignored))
        .execute(self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(MentionError::MentionNotFound);
        }

        Ok(())
    }

    pub async fn find_pending_for_agent(
        &self,
        ticket_id: Uuid,
        mentioned_agent_id: Uuid,
    ) -> Result<Option<TicketMention>, MentionError> {
        self.find_pending_for_agent_and_comment(ticket_id, mentioned_agent_id, None)
            .await
    }

    pub async fn find_pending_for_agent_and_comment(
        &self,
        ticket_id: Uuid,
        mentioned_agent_id: Uuid,
        comment_id: Option<Uuid>,
    ) -> Result<Option<TicketMention>, MentionError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, ticket_id, comment_id, mentioned_agent_id, resume_agent_id, status
            FROM ticket_mentions
            WHERE ticket_id = $1
              AND mentioned_agent_id = $2
              AND status = $3
              AND ($4::uuid IS NULL OR comment_id = $4)
            ORDER BY created_at ASC
            LIMIT 1
            "#,
        )
        .bind(ticket_id)
        .bind(mentioned_agent_id)
        .bind(mention_status_to_str(MentionStatus::Pending))
        .bind(comment_id)
        .fetch_optional(self.pool)
        .await?;

        Ok(row.as_ref().map(row_to_mention))
    }

    pub async fn list_pending_for_ticket(
        &self,
        ticket_id: Uuid,
    ) -> Result<Vec<TicketMention>, MentionError> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, ticket_id, comment_id, mentioned_agent_id, resume_agent_id, status
            FROM ticket_mentions
            WHERE ticket_id = $1 AND status = $2
            ORDER BY created_at ASC
            "#,
        )
        .bind(ticket_id)
        .bind(mention_status_to_str(MentionStatus::Pending))
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_mention).collect())
    }

    /// Returns the oldest structured consultation request that has never had a response run.
    /// Any prior response attempt excludes the mention; failed/cancelled ordinary
    /// responses are marked ignored by the terminal-run orchestration hook rather
    /// than retried automatically in a loop.
    pub async fn find_next_unscheduled_agent_request(
        &self,
        ticket_id: Uuid,
        mentioned_agent_id: Uuid,
    ) -> Result<Option<TicketMention>, MentionError> {
        let rows = sqlx::query(
            r#"
            SELECT
                tm.id, tm.ticket_id, tm.comment_id, tm.mentioned_agent_id,
                tm.resume_agent_id, tm.status, tc.body AS comment_body
            FROM ticket_mentions tm
            JOIN ticket_comments tc ON tc.id = tm.comment_id
            JOIN agents a ON a.id = tm.mentioned_agent_id
            WHERE tm.ticket_id = $1
              AND tm.mentioned_agent_id = $2
              AND tm.status = $3
              AND tm.resume_agent_id IS NULL
              AND tc.author_type = 'agent'
              AND tc.author_id IS DISTINCT FROM tm.mentioned_agent_id
              AND a.enabled = true
              AND NOT EXISTS (
                  SELECT 1
                  FROM agent_runs ar
                  WHERE ar.ticket_id = tm.ticket_id
                    AND ar.agent_id = tm.mentioned_agent_id
                    AND ar.job_type = 'respond_to_mention'
                    AND ar.trigger_comment_id = tm.comment_id
              )
            ORDER BY tm.created_at ASC, tm.id ASC
            "#,
        )
        .bind(ticket_id)
        .bind(mentioned_agent_id)
        .bind(mention_status_to_str(MentionStatus::Pending))
        .fetch_all(self.pool)
        .await?;

        let agent_map = self.build_agent_key_map().await?;
        for row in rows {
            let comment_body: String = row.get("comment_body");
            let targets_agent = agent_requests_from_comment(&comment_body)
                .iter()
                .any(|request| agent_map.get(&request.agent_key) == Some(&mentioned_agent_id));
            if targets_agent {
                return Ok(Some(row_to_mention(&row)));
            }
        }

        Ok(None)
    }

    async fn build_agent_key_map(&self) -> Result<HashMap<String, Uuid>, MentionError> {
        let agents = AgentService::new(self.pool).list_agents().await?;
        Ok(resolve_agent_keys(&agents))
    }
}

pub(crate) fn resolve_agent_keys(agents: &[Agent]) -> HashMap<String, Uuid> {
    let mut map = HashMap::new();

    for agent in agents {
        if !agent.enabled {
            continue;
        }
        if let Some(ref preset) = agent.preset_source {
            if let Entry::Vacant(entry) = map.entry(preset.clone()) {
                entry.insert(agent.id);
            }
        }
        map.insert(slugify(&agent.name), agent.id);
    }

    map
}

fn mention_token(input: &str) -> Option<&str> {
    let end = input
        .char_indices()
        .find_map(|(idx, ch)| (!is_mention_key_char(ch)).then_some(idx))
        .unwrap_or(input.len());

    if end == 0 {
        None
    } else {
        Some(&input[..end])
    }
}

fn is_mention_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn mention_status_to_str(status: MentionStatus) -> &'static str {
    match status {
        MentionStatus::Pending => "pending",
        MentionStatus::Handled => "handled",
        MentionStatus::Ignored => "ignored",
    }
}

fn mention_status_from_str(s: &str) -> Option<MentionStatus> {
    match s {
        "pending" => Some(MentionStatus::Pending),
        "handled" => Some(MentionStatus::Handled),
        "ignored" => Some(MentionStatus::Ignored),
        _ => None,
    }
}

fn row_to_mention(row: &sqlx::postgres::PgRow) -> TicketMention {
    let status_str: String = row.get("status");
    TicketMention {
        id: row.get("id"),
        ticket_id: row.get("ticket_id"),
        comment_id: row.get("comment_id"),
        mentioned_agent_id: row.get("mentioned_agent_id"),
        resume_agent_id: row.get("resume_agent_id"),
        status: mention_status_from_str(&status_str).unwrap_or(MentionStatus::Pending),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_keys_from_comment_body() {
        let keys = MentionService::parse_mention_keys(
            "@backend_engineer thoughts on option A vs B?",
            &["pm", "backend_engineer"],
        );
        assert_eq!(keys, vec!["backend_engineer"]);
    }

    #[test]
    fn parses_full_name_slug_without_matching_preset_prefix() {
        let keys = MentionService::parse_mention_keys(
            "@pm-codex thoughts on option A vs B?",
            &["pm", "pm-codex"],
        );
        assert_eq!(keys, vec!["pm-codex"]);
    }

    #[test]
    fn parses_distinct_mentions_in_body_order() {
        let keys = MentionService::parse_mention_keys(
            "@pm-codex and @backend_engineer",
            &["pm", "backend_engineer", "pm-codex"],
        );
        assert_eq!(keys, vec!["pm-codex", "backend_engineer"]);
    }
}
