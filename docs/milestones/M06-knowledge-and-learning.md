# M06 — Knowledge & Learning

## Goal

Controlled agent memory: typed knowledge with pgvector retrieval, context budget enforcement, learning extraction, and a human-governed Knowledge Inbox.

## Product scope

- `KnowledgeItem` model (product design §13.3)
- pgvector embeddings column; `EmbeddingProvider` trait + mock embedder in tests
- Metadata-first retrieval: filter by project, agent, scope, type, confidence, approval, expiry — then vector similarity
- Context builder extended with knowledge section and strict token budget (product design §13.7)
- Knowledge admin screen: Pending / Approved / Rejected / Stale tabs
- Ticket detail: **Knowledge Used** tab on runs
- Learning Extractor: post-completion proposes candidate knowledge from ticket + comments + review feedback
- Knowledge Inbox: approve, edit, reject, supersede, mark stale
- Usage logging (`knowledge_usage_logs`); `usageCount`, `lastUsedAt`, `expiresAt`, `supersededBy`
- Auto-save low-risk items vs inbox queue (product design §13.5)

## Out of scope

- Capability-gated observation runs (M07)
- Consolidation batch jobs (optional stretch; basic expiry sufficient for v1)
- Local Ollama embeddings (future; OpenAI-compatible API default)

## Dependencies

- M01: pgvector extension
- M03: context builder, agent runs
- M05: completed ticket workflow (extractor trigger)

## Architecture notes

### New server modules

```text
server/src/
  knowledge/
    embedder.rs           # EmbeddingProvider trait
    mock_embedder.rs
    retrieval.rs
    extractor.rs
    inbox_service.rs
  services/
    context_builder.rs    # extended with budget + retrieval
  api/
    knowledge.rs
  workers/
    embed_worker.rs       # async embed on approve
```

### New database tables

```text
knowledge_items
knowledge_embeddings
knowledge_usage_logs
knowledge_candidates       # inbox pending
```

### EmbeddingProvider trait

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;
    fn dimension(&self) -> usize;
}
```

Mock returns deterministic vectors from text hash for reproducible retrieval tests.

### Context budget (default)

Per product design §13.7 — configurable via YAML.

### API endpoints

```text
GET   /api/knowledge
GET   /api/knowledge/inbox
POST  /api/knowledge/inbox/:id/approve
POST  /api/knowledge/inbox/:id/reject
PATCH /api/knowledge/:id
POST  /api/knowledge/:id/supersede
POST  /api/knowledge/:id/mark-stale
GET   /api/agent-runs/:id/knowledge-used
```

## Docker Compose delta

```yaml
  server:
    environment:
      EMBEDDING_PROVIDER: mock          # tests + default dev
      EMBEDDING_DIMENSION: 1536
      # optional real provider:
      # OPENAI_API_KEY: ${OPENAI_API_KEY}
```

Postgres pgvector index created in migration (IVFFlat or HNSW per pgvector version).

## Testing strategy

### Unit tests

- Context budget allocator: sections truncated in priority order
- Retrieval: metadata filter excludes unapproved/expired/superseded
- Mock embedder determinism
- Extractor proposes expected types from fixture ticket thread

### Integration tests

- Approve knowledge → embed job → vector stored → next run retrieves item in context package
- Rejected item never appears in retrieval
- Supersede links old → new; old excluded
- Expired item excluded after `expiresAt`
- Usage log incremented when item included in run

### E2E smoke (CI)

`e2e/smoke/m06-knowledge.spec`:

1. Complete ticket (or seed approved knowledge via API)
2. Open Knowledge screen → approve pending inbox item
3. Run agent on new ticket → Knowledge Used tab lists retrieved item

### E2E full (local)

- Edit candidate before approve
- Reject and verify absent from retrieval
- View source ticket link from knowledge item

## Acceptance criteria

- [ ] Approved knowledge appears in agent context package on subsequent runs
- [ ] Inbox governs what enters long-term memory
- [ ] pgvector retrieval respects metadata filters and budget
- [ ] Extractor creates candidates after ticket completion
- [ ] CI smoke E2E passes

## References

- Product design §13 (knowledge & self-learning), §16 (context package knowledge section)
- Framework selection §3 (pgvector), §8 (embedding provider)
