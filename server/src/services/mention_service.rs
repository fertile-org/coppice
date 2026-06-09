use crate::domain::mention::{MentionStatus, TicketMention};
use crate::domain::slug::slugify;
use crate::services::agent_service::AgentService;
use sqlx::PgPool;
use sqlx::Row;
use std::collections::HashMap;
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
        let mut found = Vec::new();
        for key in known_keys {
            let needle = format!("@{key}");
            if body.contains(&needle) && !found.iter().any(|k| k == key) {
                found.push((*key).to_string());
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

        for key in keys {
            let Some(&agent_id) = agent_map.get(key) else {
                continue;
            };

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

    async fn build_agent_key_map(&self) -> Result<HashMap<String, Uuid>, MentionError> {
        let agents = AgentService::new(self.pool).list_agents().await?;
        let mut map = HashMap::new();

        for agent in agents {
            if !agent.enabled {
                continue;
            }
            if let Some(ref preset) = agent.preset_source {
                map.insert(preset.clone(), agent.id);
            }
            map.insert(slugify(&agent.name), agent.id);
        }

        Ok(map)
    }
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
}
