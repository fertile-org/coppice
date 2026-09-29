use crate::domain::context_profile::ContextProfile;
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewRunToolScope {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub ticket_id: Option<Uuid>,
    pub chat_session_id: Option<Uuid>,
    pub board_id: Option<Uuid>,
    pub profile: ContextProfile,
    pub job_type: String,
    pub compaction_ticket_ids: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct RunToolScope {
    pub token_id: Uuid,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub ticket_id: Option<Uuid>,
    pub chat_session_id: Option<Uuid>,
    pub board_id: Option<Uuid>,
    pub profile: ContextProfile,
    pub job_type: String,
    pub compaction_ticket_ids: Vec<Uuid>,
}

pub struct TokenService<'a> {
    pool: &'a PgPool,
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

impl<'a> TokenService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Stores only the SHA-256 of the token; the plaintext is returned once.
    pub async fn mint(
        &self,
        scope: &NewRunToolScope,
        ttl: Duration,
    ) -> Result<String, sqlx::Error> {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let plaintext = hex::encode(bytes);

        sqlx::query(
            r#"
            INSERT INTO run_tool_tokens (
                id, token_hash, subject_kind, run_id, agent_id, ticket_id,
                chat_session_id, board_id, context_profile, job_type,
                compaction_ticket_ids, expires_at
            )
            VALUES (
                $1, $2, 'run', $3, $4, $5, $6, $7, $8, $9, $10,
                now() + make_interval(secs => $11)
            )
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(hash_token(&plaintext))
        .bind(scope.run_id)
        .bind(scope.agent_id)
        .bind(scope.ticket_id)
        .bind(scope.chat_session_id)
        .bind(scope.board_id)
        .bind(scope.profile.as_str())
        .bind(&scope.job_type)
        .bind(&scope.compaction_ticket_ids)
        .bind(ttl.as_secs_f64())
        .execute(self.pool)
        .await?;

        Ok(plaintext)
    }

    /// `None` when the token is unknown, expired, or revoked.
    pub async fn verify(&self, token: &str) -> Result<Option<RunToolScope>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, run_id, agent_id, ticket_id, chat_session_id, board_id,
                   context_profile, job_type, compaction_ticket_ids
            FROM run_tool_tokens
            WHERE token_hash = $1
              AND subject_kind = 'run'
              AND run_id IS NOT NULL
              AND revoked_at IS NULL
              AND expires_at > now()
            "#,
        )
        .bind(hash_token(token))
        .fetch_optional(self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let profile = ContextProfile::from_str(&row.get::<String, _>("context_profile"))
            .map_err(|e| sqlx::Error::Decode(e.into()))?;

        Ok(Some(RunToolScope {
            token_id: row.get("id"),
            run_id: row.get("run_id"),
            agent_id: row.get("agent_id"),
            ticket_id: row.get("ticket_id"),
            chat_session_id: row.get("chat_session_id"),
            board_id: row.get("board_id"),
            profile,
            job_type: row.get("job_type"),
            compaction_ticket_ids: row.get("compaction_ticket_ids"),
        }))
    }

    pub async fn revoke_for_run(&self, run_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE run_tool_tokens SET revoked_at = now() WHERE run_id = $1 AND revoked_at IS NULL",
        )
        .bind(run_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}
