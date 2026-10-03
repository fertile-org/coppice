//! Connector checks: a "Test connection" agent run owned by a `connector_checks` row.

use crate::domain::connector_check::{sanitize_failure, CheckStatus};
use crate::services::agent_service::{AgentError, AgentService};
use crate::services::run_service::{RunError, RunService};
use crate::AppConfig;
use serde::Serialize;
use sqlx::PgPool;
use sqlx::Row;
use std::collections::HashMap;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

const ONE_ACTIVE_CHECK_INDEX: &str = "connector_checks_one_active_per_connector";
const SERVER_RESTARTED: &str = "server restarted";

#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error("a check is already active for this connector")]
    ActiveCheck,
    #[error("agent does not use this connector")]
    WrongConnector,
    #[error("connector is disabled")]
    Disabled,
    #[error("unknown connector")]
    UnknownConnector,
    #[error("agent not found")]
    AgentNotFound,
    #[error("check not found")]
    NotFound,
    #[error(transparent)]
    Agent(AgentError),
    #[error(transparent)]
    Run(RunError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<RunError> for CheckError {
    fn from(err: RunError) -> Self {
        match err {
            RunError::Database(err) => Self::Database(err),
            other => Self::Run(other),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckSummary {
    pub id: Uuid,
    pub status: &'static str,
    pub failure: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckToolCall {
    pub tool: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckDetail {
    pub id: Uuid,
    pub connector: String,
    pub agent_id: Uuid,
    pub status: &'static str,
    pub failure: Option<String>,
    pub run_id: Option<Uuid>,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub tool_calls: Vec<CheckToolCall>,
}

pub struct ConnectorCheckService<'a> {
    pool: &'a PgPool,
}

impl<'a> ConnectorCheckService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Queue a check and its agent run. Any connector id the config knows is
    /// accepted, `mock` included; the HTTP route decides which ids are exposed.
    /// Returns `(check_id, run_id)`.
    pub async fn start(
        &self,
        config: &AppConfig,
        connector: &str,
        agent_id: Uuid,
        created_by: Uuid,
    ) -> Result<(Uuid, Uuid), CheckError> {
        let agent = AgentService::new(self.pool)
            .get(agent_id)
            .await
            .map_err(|err| match err {
                AgentError::AgentNotFound => CheckError::AgentNotFound,
                other => CheckError::Agent(other),
            })?;
        if agent.connector != connector {
            return Err(CheckError::WrongConnector);
        }
        match config.agent.connectors.enabled(connector) {
            None => return Err(CheckError::UnknownConnector),
            Some(false) => return Err(CheckError::Disabled),
            Some(true) => {}
        }

        let check_id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            INSERT INTO connector_checks (id, connector, agent_id, status, created_by)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(check_id)
        .bind(connector)
        .bind(agent_id)
        .bind(CheckStatus::Queued.as_str())
        .bind(created_by)
        .execute(&mut *tx)
        .await
        .map_err(|err| {
            if err.as_database_error().and_then(|e| e.constraint()) == Some(ONE_ACTIVE_CHECK_INDEX)
            {
                CheckError::ActiveCheck
            } else {
                CheckError::Database(err)
            }
        })?;
        let run = RunService::create_connector_check_run(&mut tx, check_id, agent_id).await?;
        tx.commit().await?;
        Ok((check_id, run.id))
    }

    pub async fn mark_running(&self, check_id: Uuid) -> Result<(), CheckError> {
        sqlx::query(
            "UPDATE connector_checks SET status = 'running' WHERE id = $1 AND status = 'queued'",
        )
        .bind(check_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Record the outcome of an active check; a finished check is left as is.
    /// The failure text is capped and stored only for failed checks.
    pub async fn finish(
        &self,
        check_id: Uuid,
        passed: bool,
        failure: Option<&str>,
    ) -> Result<bool, CheckError> {
        let (status, failure) = if passed {
            (CheckStatus::Passed, None)
        } else {
            let text = failure
                .map(|f| sanitize_failure(f, &[]))
                .filter(|f| !f.is_empty())
                .unwrap_or_else(|| "check failed".to_string());
            (CheckStatus::Failed, Some(text))
        };
        let updated = sqlx::query(
            r#"
            UPDATE connector_checks
            SET status = $2, failure = $3, finished_at = now()
            WHERE id = $1 AND status IN ('queued', 'running')
            "#,
        )
        .bind(check_id)
        .bind(status.as_str())
        .bind(failure)
        .execute(self.pool)
        .await?
        .rows_affected();
        Ok(updated > 0)
    }

    /// Fail the active check owning `run_id`, if any.
    pub async fn fail_for_run(&self, run_id: Uuid, failure: &str) -> Result<(), CheckError> {
        let check_id: Option<Option<Uuid>> =
            sqlx::query_scalar("SELECT connector_check_id FROM agent_runs WHERE id = $1")
                .bind(run_id)
                .fetch_optional(self.pool)
                .await?;
        if let Some(check_id) = check_id.flatten() {
            self.finish(check_id, false, Some(failure)).await?;
        }
        Ok(())
    }

    pub async fn get(&self, check_id: Uuid) -> Result<CheckDetail, CheckError> {
        let row = sqlx::query(
            r#"
            SELECT c.id, c.connector, c.agent_id, c.status, c.failure, c.created_at,
                   c.finished_at, r.id AS run_id
            FROM connector_checks c
            LEFT JOIN agent_runs r ON r.connector_check_id = c.id
            WHERE c.id = $1
            "#,
        )
        .bind(check_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(CheckError::NotFound)?;
        let run_id: Option<Uuid> = row.get("run_id");
        let tool_calls = match run_id {
            Some(run_id) => sqlx::query_as::<_, (String, String)>(
                "SELECT tool, status FROM run_tool_calls WHERE run_id = $1 ORDER BY created_at, id",
            )
            .bind(run_id)
            .fetch_all(self.pool)
            .await?
            .into_iter()
            .map(|(tool, status)| CheckToolCall { tool, status })
            .collect(),
            None => Vec::new(),
        };
        let finished_at: Option<OffsetDateTime> = row.get("finished_at");
        Ok(CheckDetail {
            id: row.get("id"),
            connector: row.get("connector"),
            agent_id: row.get("agent_id"),
            status: status_str(&row.get::<String, _>("status")),
            failure: row.get("failure"),
            run_id,
            created_at: format_time(row.get("created_at")),
            finished_at: finished_at.map(format_time),
            tool_calls,
        })
    }

    /// Most recent check per connector.
    pub async fn latest_by_connector(&self) -> Result<HashMap<String, CheckSummary>, CheckError> {
        let rows: Vec<(String, Uuid, String, Option<String>, OffsetDateTime)> = sqlx::query_as(
            r#"
            SELECT DISTINCT ON (connector) connector, id, status, failure, created_at
            FROM connector_checks
            ORDER BY connector, created_at DESC, id DESC
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(connector, id, status, failure, created_at)| {
                (
                    connector,
                    CheckSummary {
                        id,
                        status: status_str(&status),
                        failure,
                        created_at: format_time(created_at),
                    },
                )
            })
            .collect())
    }

    /// Checks left `queued`/`running` by a previous server process can never
    /// finish; mark them failed. Returns how many were updated.
    pub async fn fail_stale(&self) -> Result<u64, CheckError> {
        Ok(sqlx::query(
            r#"
            UPDATE connector_checks
            SET status = 'failed', failure = $1, finished_at = now()
            WHERE status IN ('queued', 'running')
            "#,
        )
        .bind(SERVER_RESTARTED)
        .execute(self.pool)
        .await?
        .rows_affected())
    }
}

fn status_str(value: &str) -> &'static str {
    CheckStatus::parse(value)
        .unwrap_or(CheckStatus::Failed)
        .as_str()
}

fn format_time(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}
