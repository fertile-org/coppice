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

async fn embedding_column_type(pool: &sqlx::PgPool) -> anyhow::Result<String> {
    sqlx::query_scalar(
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
    .ok_or_else(|| anyhow::anyhow!("knowledge embedding column is missing"))
}

/// Read-only check: configured dimension must match the live `vector(n)` column type.
pub async fn validate_schema_dimension(
    pool: &sqlx::PgPool,
    configured_dimension: usize,
) -> anyhow::Result<()> {
    let database_type = embedding_column_type(pool).await?;
    let expected = format!("vector({configured_dimension})");
    if database_type != expected {
        anyhow::bail!(
            "configured embedding dimension {configured_dimension} does not match migrated database type {database_type}"
        );
    }
    Ok(())
}

/// Ensure the live column matches `knowledge.embedding.dimension`.
///
/// When the typed column already matches, this is a no-op.
/// When it differs and `knowledge_embeddings` is empty, the column (and HNSW index)
/// are rewritten to `vector(configured_dimension)`.
/// When it differs and rows exist, fails with a clear clear-and-re-embed message.
pub async fn ensure_schema_dimension(
    pool: &sqlx::PgPool,
    configured_dimension: usize,
) -> anyhow::Result<()> {
    if configured_dimension == 0 {
        anyhow::bail!("configured embedding dimension must be greater than zero");
    }
    let database_type = embedding_column_type(pool).await?;
    let expected = format!("vector({configured_dimension})");
    if database_type == expected {
        return Ok(());
    }

    let row_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM knowledge_embeddings")
            .fetch_one(pool)
            .await?;
    if row_count > 0 {
        anyhow::bail!(
            "configured embedding dimension {configured_dimension} does not match database type {database_type} \
             and knowledge_embeddings has {row_count} row(s). \
             Clear embeddings (`DELETE FROM knowledge_embeddings`) and restart so Coppice can rewrite the column, \
             then re-embed; or set knowledge.embedding.dimension to match {database_type}. \
             Vectors are never padded or truncated."
        );
    }

    rewrite_empty_embedding_column(pool, configured_dimension).await?;
    validate_schema_dimension(pool, configured_dimension).await
}

async fn rewrite_empty_embedding_column(
    pool: &sqlx::PgPool,
    configured_dimension: usize,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DROP INDEX IF EXISTS knowledge_embeddings_hnsw_cosine_idx")
        .execute(&mut *tx)
        .await?;
    // Identifier is a usize from config (already validated > 0); not user SQL text.
    let alter = format!(
        "ALTER TABLE knowledge_embeddings ALTER COLUMN embedding TYPE vector({configured_dimension})"
    );
    sqlx::query(&alter).execute(&mut *tx).await?;
    sqlx::query(
        r#"
        CREATE INDEX knowledge_embeddings_hnsw_cosine_idx
            ON knowledge_embeddings USING hnsw (embedding vector_cosine_ops)
        "#,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
