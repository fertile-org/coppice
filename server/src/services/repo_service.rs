use std::path::Path;

use crate::domain::repo::{
    verification_status_from_str, verification_status_to_str, Repo, VerificationStatus,
};
use crate::services::repo_verifier::verify_local_path;
use sqlx::PgPool;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

const REPO_COLUMNS: &str = r#"
    id, name, local_path, remote_url, default_branch,
    verification_status, verification_error, last_verified_at,
    forge_token_secret_id, created_at, updated_at
"#;

pub struct RepoService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("repo not found")]
    NotFound,
    #[error("repo in use by tickets")]
    InUse,
    #[error("duplicate local_path")]
    DuplicatePath,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl<'a> RepoService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_all(&self) -> Result<Vec<Repo>, RepoError> {
        let query = format!(
            "SELECT {REPO_COLUMNS} FROM repos ORDER BY created_at ASC"
        );
        let rows = sqlx::query(&query).fetch_all(self.pool).await?;
        Ok(rows.iter().map(row_to_repo).collect())
    }

    pub async fn get(&self, id: Uuid) -> Result<Repo, RepoError> {
        let query = format!("SELECT {REPO_COLUMNS} FROM repos WHERE id = $1");
        let row = sqlx::query(&query)
            .bind(id)
            .fetch_optional(self.pool)
            .await?
            .ok_or(RepoError::NotFound)?;

        Ok(row_to_repo(&row))
    }

    pub async fn create(
        &self,
        name: &str,
        local_path: &str,
        remote_url: Option<&str>,
        default_branch: &str,
    ) -> Result<Repo, RepoError> {
        if local_path.trim().is_empty() {
            return Err(RepoError::Validation("local_path is required".into()));
        }

        let verify_result = verify_local_path(Path::new(local_path));
        let now = OffsetDateTime::now_utc();
        let status_str = verification_status_to_str(verify_result.status);

        let id = Uuid::new_v4();
        let query = format!(
            r#"
            INSERT INTO repos (
                id, name, local_path, remote_url, default_branch,
                verification_status, verification_error, last_verified_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING {REPO_COLUMNS}
            "#
        );
        let row = sqlx::query(&query)
            .bind(id)
            .bind(name)
            .bind(local_path)
            .bind(remote_url)
            .bind(default_branch)
            .bind(status_str)
            .bind(&verify_result.error)
            .bind(now)
            .fetch_one(self.pool)
            .await
            .map_err(map_unique_path_error)?;

        Ok(row_to_repo(&row))
    }

    pub async fn update(
        &self,
        id: Uuid,
        name: Option<&str>,
        local_path: Option<&str>,
        remote_url: Option<Option<&str>>,
        default_branch: Option<&str>,
    ) -> Result<Repo, RepoError> {
        let current = self.get(id).await?;

        let name = name.unwrap_or(&current.name);
        let local_path = local_path.unwrap_or(&current.local_path);
        if local_path.trim().is_empty() {
            return Err(RepoError::Validation("local_path is required".into()));
        }

        let path_changed = local_path != current.local_path;
        let remote_url = match remote_url {
            Some(value) => value.map(str::to_string),
            None => current.remote_url.clone(),
        };
        let default_branch = default_branch.unwrap_or(&current.default_branch);

        let row = if path_changed {
            let verify_result = verify_local_path(Path::new(local_path));
            let now = OffsetDateTime::now_utc();
            let status_str = verification_status_to_str(verify_result.status);

            let query = format!(
                r#"
                UPDATE repos
                SET name = $2,
                    local_path = $3,
                    remote_url = $4,
                    default_branch = $5,
                    verification_status = $6,
                    verification_error = $7,
                    last_verified_at = $8,
                    updated_at = now()
                WHERE id = $1
                RETURNING {REPO_COLUMNS}
                "#
            );
            sqlx::query(&query)
                .bind(id)
                .bind(name)
                .bind(local_path)
                .bind(&remote_url)
                .bind(default_branch)
                .bind(status_str)
                .bind(&verify_result.error)
                .bind(now)
                .fetch_optional(self.pool)
                .await
                .map_err(map_unique_path_error)?
        } else {
            let query = format!(
                r#"
                UPDATE repos
                SET name = $2,
                    remote_url = $3,
                    default_branch = $4,
                    updated_at = now()
                WHERE id = $1
                RETURNING {REPO_COLUMNS}
                "#
            );
            sqlx::query(&query)
                .bind(id)
                .bind(name)
                .bind(&remote_url)
                .bind(default_branch)
                .fetch_optional(self.pool)
                .await?
        }
        .ok_or(RepoError::NotFound)?;

        Ok(row_to_repo(&row))
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), RepoError> {
        let in_use: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM tickets WHERE repo_id = $1)",
        )
        .bind(id)
        .fetch_one(self.pool)
        .await?;

        if in_use {
            return Err(RepoError::InUse);
        }

        let result = sqlx::query("DELETE FROM repos WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(RepoError::NotFound);
        }

        Ok(())
    }

    pub async fn set_forge_token_secret(
        &self,
        id: Uuid,
        secret_id: Option<Uuid>,
    ) -> Result<Repo, RepoError> {
        let _ = self.get(id).await?;
        let query = format!(
            r#"
            UPDATE repos
            SET forge_token_secret_id = $2, updated_at = now()
            WHERE id = $1
            RETURNING {REPO_COLUMNS}
            "#
        );
        let row = sqlx::query(&query)
            .bind(id)
            .bind(secret_id)
            .fetch_optional(self.pool)
            .await?
            .ok_or(RepoError::NotFound)?;
        Ok(row_to_repo(&row))
    }

    pub async fn verify(&self, id: Uuid) -> Result<Repo, RepoError> {
        let current = self.get(id).await?;
        let verify_result = verify_local_path(Path::new(&current.local_path));
        let now = OffsetDateTime::now_utc();
        let status_str = verification_status_to_str(verify_result.status);

        let query = format!(
            r#"
            UPDATE repos
            SET verification_status = $2,
                verification_error = $3,
                last_verified_at = $4,
                updated_at = now()
            WHERE id = $1
            RETURNING {REPO_COLUMNS}
            "#
        );
        let row = sqlx::query(&query)
            .bind(id)
            .bind(status_str)
            .bind(&verify_result.error)
            .bind(now)
            .fetch_optional(self.pool)
            .await?
            .ok_or(RepoError::NotFound)?;

        Ok(row_to_repo(&row))
    }
}

fn map_unique_path_error(err: sqlx::Error) -> RepoError {
    if let sqlx::Error::Database(db_err) = &err {
        if db_err.constraint() == Some("repos_local_path_idx") {
            return RepoError::DuplicatePath;
        }
    }
    RepoError::Database(err)
}

fn row_to_repo(row: &sqlx::postgres::PgRow) -> Repo {
    let status_str: String = row.get("verification_status");
    let verification_status = verification_status_from_str(&status_str)
        .unwrap_or(VerificationStatus::Error);

    Repo {
        id: row.get("id"),
        name: row.get("name"),
        local_path: row.get("local_path"),
        remote_url: row.get("remote_url"),
        default_branch: row.get("default_branch"),
        verification_status,
        verification_error: row.get("verification_error"),
        last_verified_at: row.get("last_verified_at"),
        forge_token_secret_id: row.get("forge_token_secret_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
