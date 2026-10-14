//! Models a person marked as not covered by their plan, per connector.
//!
//! The flag is one model id from that connector's live list, not a switch for
//! the whole connector.

use coppice_connectors::{get, MOCK};
use sqlx::PgPool;

const MAX_MODELS: usize = 200;
const MAX_MODEL_ID_LEN: usize = 256;

pub struct UncoveredModelService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum UncoveredModelError {
    #[error("unknown connector")]
    UnknownConnector,
    #[error("invalid model id")]
    InvalidModelId,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl<'a> UncoveredModelService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self, connector_id: &str) -> Result<Vec<String>, UncoveredModelError> {
        ensure_connector(connector_id)?;
        let ids = sqlx::query_scalar(
            r#"
            SELECT model_id
            FROM connector_uncovered_models
            WHERE connector_id = $1
            ORDER BY model_id
            "#,
        )
        .bind(connector_id)
        .fetch_all(self.pool)
        .await?;
        Ok(ids)
    }

    /// Replace the connector's uncovered set with `model_ids`.
    pub async fn replace(
        &self,
        connector_id: &str,
        model_ids: &[String],
    ) -> Result<Vec<String>, UncoveredModelError> {
        ensure_connector(connector_id)?;
        let model_ids = normalize_model_ids(model_ids)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM connector_uncovered_models WHERE connector_id = $1")
            .bind(connector_id)
            .execute(&mut *tx)
            .await?;
        for model_id in &model_ids {
            sqlx::query(
                r#"
                INSERT INTO connector_uncovered_models (connector_id, model_id)
                VALUES ($1, $2)
                "#,
            )
            .bind(connector_id)
            .bind(model_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(model_ids)
    }
}

fn ensure_connector(connector_id: &str) -> Result<(), UncoveredModelError> {
    match get(connector_id) {
        Some(descriptor) if descriptor.id != MOCK => Ok(()),
        _ => Err(UncoveredModelError::UnknownConnector),
    }
}

fn normalize_model_ids(model_ids: &[String]) -> Result<Vec<String>, UncoveredModelError> {
    if model_ids.len() > MAX_MODELS {
        return Err(UncoveredModelError::InvalidModelId);
    }
    let mut out = Vec::new();
    for id in model_ids {
        let id = id.trim();
        if !valid_model_id(id) {
            return Err(UncoveredModelError::InvalidModelId);
        }
        if !out.iter().any(|existing: &String| existing == id) {
            out.push(id.to_string());
        }
    }
    out.sort();
    Ok(out)
}

fn valid_model_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_MODEL_ID_LEN
        && !id.chars().any(|ch| ch.is_control() || ch.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_trims_dedupes_and_rejects_blank_or_spaced_ids() {
        let ids = normalize_model_ids(&[" composer-2.5 ".into(), "composer-2.5".into()]).unwrap();
        assert_eq!(ids, vec!["composer-2.5".to_string()]);
        assert!(normalize_model_ids(&["gpt 5".into()]).is_err());
        assert!(normalize_model_ids(&[String::new()]).is_err());
        assert!(normalize_model_ids(&[" ".into()]).is_err());
    }

    #[tokio::test]
    async fn replace_stores_one_row_per_model_for_that_connector() {
        let pool = crate::db::shared_test_pool().await.expect("pool");
        crate::db::truncate_test_workspace(&pool)
            .await
            .expect("truncate");
        let service = UncoveredModelService::new(&pool);
        assert!(service.replace(MOCK, &["auto".into()]).await.is_err());
        assert!(service
            .replace("not-a-connector", &["auto".into()])
            .await
            .is_err());

        let stored = service
            .replace("cursor", &["auto".into(), "composer-2.5".into()])
            .await
            .expect("replace");
        assert_eq!(stored, vec!["auto".to_string(), "composer-2.5".to_string()]);
        assert_eq!(service.list("cursor").await.expect("list"), stored);
        assert!(service.list("claude-code").await.expect("other").is_empty());

        let cleared = service.replace("cursor", &[]).await.expect("clear");
        assert!(cleared.is_empty());
        assert!(service.list("cursor").await.expect("list").is_empty());
    }
}
