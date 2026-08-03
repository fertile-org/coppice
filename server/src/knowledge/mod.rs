pub mod embedder;
pub mod extractor;
pub mod mock_embedder;
pub mod openai_embedder;
pub mod retrieval;

use crate::config::EmbeddingConfig;
use embedder::{EmbeddingError, EmbeddingProvider};
use mock_embedder::MockEmbeddingProvider;
use openai_embedder::OpenAiCompatibleEmbeddingProvider;
use std::sync::Arc;

pub fn embedding_provider(
    config: &EmbeddingConfig,
) -> Result<Arc<dyn EmbeddingProvider>, EmbeddingError> {
    match config.provider.as_str() {
        "mock" => Ok(Arc::new(MockEmbeddingProvider::new(
            config.dimension,
            config.model.clone(),
        ))),
        "openai_compatible" => Ok(Arc::new(OpenAiCompatibleEmbeddingProvider::new(config)?)),
        other => Err(EmbeddingError::Configuration(format!(
            "unsupported embedding provider: {other}"
        ))),
    }
}

pub async fn validate_schema_dimension(
    pool: &sqlx::PgPool,
    configured_dimension: usize,
) -> anyhow::Result<()> {
    let database_type: String = sqlx::query_scalar(
        r#"
        SELECT format_type(attribute.atttypid, attribute.atttypmod)
        FROM pg_attribute attribute
        JOIN pg_class relation ON relation.oid = attribute.attrelid
        JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
        WHERE namespace.nspname = current_schema()
          AND relation.relname = 'knowledge_embeddings'
          AND attribute.attname = 'embedding'
          AND attribute.attnum > 0
          AND NOT attribute.attisdropped
        "#,
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("knowledge embedding column is missing"))?;
    let expected = format!("vector({configured_dimension})");
    if database_type != expected {
        anyhow::bail!(
            "configured embedding dimension {configured_dimension} does not match migrated database type {database_type}"
        );
    }
    Ok(())
}
