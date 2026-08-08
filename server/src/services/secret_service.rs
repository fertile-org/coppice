use sqlx::PgPool;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::crypto::{SecretStore, SecretStoreError};

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("secret not found")]
    NotFound,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Crypto(#[from] SecretStoreError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub struct SecretService<'a> {
    pool: &'a PgPool,
    store: &'a SecretStore,
}

impl<'a> SecretService<'a> {
    pub fn new(pool: &'a PgPool, store: &'a SecretStore) -> Self {
        Self { pool, store }
    }

    /// Create or replace a named secret; returns the secret id.
    pub async fn upsert_named(&self, name: &str, value: &str) -> Result<Uuid, SecretError> {
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() {
            return Err(SecretError::Validation("secret name is required".into()));
        }
        if value.is_empty() {
            return Err(SecretError::Validation("secret value is required".into()));
        }

        let (ciphertext, nonce) = self.store.encrypt(value)?;
        let now = OffsetDateTime::now_utc();

        if let Some(existing_id) = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM secrets WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(self.pool)
        .await?
        {
            sqlx::query(
                r#"
                UPDATE secrets
                SET ciphertext = $2, nonce = $3, updated_at = $4
                WHERE id = $1
                "#,
            )
            .bind(existing_id)
            .bind(&ciphertext)
            .bind(&nonce)
            .bind(now)
            .execute(self.pool)
            .await?;
            return Ok(existing_id);
        }

        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO secrets (id, name, ciphertext, nonce, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(&ciphertext)
        .bind(&nonce)
        .bind(now)
        .bind(now)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    pub async fn decrypt_by_id(&self, id: Uuid) -> Result<String, SecretError> {
        let row = sqlx::query(
            "SELECT ciphertext, nonce FROM secrets WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(SecretError::NotFound)?;

        let ciphertext: Vec<u8> = row.get("ciphertext");
        let nonce: Vec<u8> = row.get("nonce");
        Ok(self.store.decrypt(&ciphertext, &nonce)?)
    }

    pub async fn delete_by_id(&self, id: Uuid) -> Result<(), SecretError> {
        let result = sqlx::query("DELETE FROM secrets WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(SecretError::NotFound);
        }
        Ok(())
    }
}
