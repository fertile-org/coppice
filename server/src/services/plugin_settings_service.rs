use crate::crypto::SecretStore;
use crate::services::secret_service::{SecretError, SecretService};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PluginSettingsError {
    #[error("unknown setting \"{0}\"")]
    UnknownKey(String),
    #[error(transparent)]
    Secret(#[from] SecretError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Write-only plugin settings; values live in `secrets` as `plugin-setting-<plugin_id>-<key>`.
pub struct PluginSettingsService<'a> {
    pool: &'a PgPool,
    store: &'a SecretStore,
}

impl<'a> PluginSettingsService<'a> {
    pub fn new(pool: &'a PgPool, store: &'a SecretStore) -> Self {
        Self { pool, store }
    }

    fn secrets(&self) -> SecretService<'a> {
        SecretService::new(self.pool, self.store)
    }

    pub async fn configured_keys(
        &self,
        plugin_id: Uuid,
    ) -> Result<BTreeSet<String>, PluginSettingsError> {
        let keys: Vec<String> =
            sqlx::query_scalar("SELECT key FROM plugin_settings WHERE plugin_id = $1")
                .bind(plugin_id)
                .fetch_all(self.pool)
                .await?;
        Ok(keys.into_iter().collect())
    }

    pub async fn configured_keys_by_plugin(
        &self,
    ) -> Result<BTreeMap<Uuid, BTreeSet<String>>, PluginSettingsError> {
        let rows = sqlx::query("SELECT plugin_id, key FROM plugin_settings")
            .fetch_all(self.pool)
            .await?;
        let mut by_plugin: BTreeMap<Uuid, BTreeSet<String>> = BTreeMap::new();
        for row in rows {
            by_plugin
                .entry(row.get("plugin_id"))
                .or_default()
                .insert(row.get("key"));
        }
        Ok(by_plugin)
    }

    /// Validates every key against `allowed` before writing; an empty value clears the key.
    pub async fn set(
        &self,
        plugin_id: Uuid,
        allowed: &BTreeSet<String>,
        values: BTreeMap<String, String>,
    ) -> Result<(), PluginSettingsError> {
        if let Some(unknown) = values.keys().find(|key| !allowed.contains(*key)) {
            return Err(PluginSettingsError::UnknownKey(unknown.clone()));
        }
        for (key, value) in values {
            if value.trim().is_empty() {
                self.clear(plugin_id, &key).await?;
                continue;
            }
            let secret_id = self
                .secrets()
                .upsert_named(&format!("plugin-setting-{plugin_id}-{key}"), &value)
                .await?;
            sqlx::query(
                r#"
                INSERT INTO plugin_settings (plugin_id, key, secret_id)
                VALUES ($1, $2, $3)
                ON CONFLICT (plugin_id, key)
                DO UPDATE SET secret_id = EXCLUDED.secret_id, updated_at = now()
                "#,
            )
            .bind(plugin_id)
            .bind(&key)
            .bind(secret_id)
            .execute(self.pool)
            .await?;
        }
        Ok(())
    }

    async fn clear(&self, plugin_id: Uuid, key: &str) -> Result<(), PluginSettingsError> {
        let secret_id: Option<Uuid> = sqlx::query_scalar(
            "DELETE FROM plugin_settings WHERE plugin_id = $1 AND key = $2 RETURNING secret_id",
        )
        .bind(plugin_id)
        .bind(key)
        .fetch_optional(self.pool)
        .await?;
        if let Some(secret_id) = secret_id {
            match self.secrets().delete_by_id(secret_id).await {
                Ok(()) | Err(SecretError::NotFound) => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }

    pub async fn decrypted(
        &self,
        plugin_id: Uuid,
    ) -> Result<BTreeMap<String, String>, PluginSettingsError> {
        let rows = sqlx::query("SELECT key, secret_id FROM plugin_settings WHERE plugin_id = $1")
            .bind(plugin_id)
            .fetch_all(self.pool)
            .await?;
        let secrets = self.secrets();
        let mut values = BTreeMap::new();
        for row in rows {
            let value = secrets.decrypt_by_id(row.get("secret_id")).await?;
            values.insert(row.get("key"), value);
        }
        Ok(values)
    }
}
