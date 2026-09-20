-- Allow knowledge.embedding.dimension to differ from the original M06 lock of 1536.
-- The pgvector column stays typed (vector(n)) so HNSW indexing remains valid.
-- Startup ensure_schema_dimension() ALTERs an empty column to match config; if rows
-- already exist at another dimension, startup fails and operators must clear
-- knowledge_embeddings (then re-embed) before changing dimension.

ALTER TABLE knowledge_embeddings
    DROP CONSTRAINT IF EXISTS knowledge_embeddings_embedding_dimension_check;

ALTER TABLE knowledge_embeddings
    ADD CONSTRAINT knowledge_embeddings_embedding_dimension_positive
    CHECK (embedding_dimension > 0);
