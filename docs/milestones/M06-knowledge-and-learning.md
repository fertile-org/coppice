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

## Architecture contract

The reviewed design gate is [M06 Knowledge and Learning Design](../superpowers/specs/2026-08-03-m06-knowledge-and-learning-design.md). It fixes the trust boundary, revision model, vector dimension, retrieval ordering, capacity envelope, token counter, worker ownership, and API contract. The matching implementation plan is [here](../superpowers/plans/2026-08-03-m06-knowledge-and-learning.md).

### Server modules

```text
server/src/
  knowledge/
    embedder.rs           # EmbeddingProvider trait
    mock_embedder.rs
    retrieval.rs
    extractor.rs
    openai_embedder.rs
  services/
    knowledge_service.rs
    knowledge_job_service.rs
    context_budget.rs
  api/
    knowledge.rs
  workers/
    knowledge_worker.rs   # dedicated embed + extraction queue
```

### New database tables

```text
knowledge_items
knowledge_revisions        # immutable semantic history
knowledge_embeddings
knowledge_usage_logs
knowledge_jobs             # dedicated durable work queue
```

### EmbeddingProvider trait

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError>;
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn dimension(&self) -> usize;
}
```

Mock returns deterministic normalized vectors from text hashes for reproducible retrieval tests. The OpenAI-compatible adapter uses `POST /embeddings` and strictly checks result count, ordering, finiteness, and the configured dimension. M06 migrates `vector(1536)` and startup rejects a different configured dimension.

### Context budget (default)

The selected `ByteTokenCounter` counts `ceil(UTF-8 bytes / 4)` and records its name in the assembled context path. TOML config bounds ticket, comments, project rules, retrieved knowledge, prior-attempt summary, output contract, and the total. Safety and result-contract sections are mandatory; overflow fails before provider invocation.

### API endpoints

```text
GET   /api/knowledge
GET   /api/knowledge/inbox
POST  /api/knowledge
GET   /api/knowledge/:id
PATCH /api/knowledge/:id
POST  /api/knowledge/:id/approve
POST  /api/knowledge/:id/reject
POST  /api/knowledge/:id/supersede
POST  /api/knowledge/:id/mark-stale
POST  /api/knowledge/:id/expire
GET   /api/agent-runs/:id/knowledge-used
```

## Docker Compose delta

```yaml
  server:
    environment:
      COPPICE_KNOWLEDGE__EMBEDDING__PROVIDER: mock
      COPPICE_KNOWLEDGE__EMBEDDING__DIMENSION: "1536"
```

Postgres uses an HNSW cosine index. M06 first materializes relational eligibility and then performs bounded, deterministic cosine ranking; this ordering is intentional for the documented 10,000-active-items-per-project envelope.

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

`make e2e-smoke-m06-knowledge`:

1. Create, edit, approve, and wait for a project-scoped manual candidate to embed.
2. Run an agent with an exact-match ticket and verify the exact revision appears once in Knowledge Used.
3. Move a separate ticket to Done and verify deterministic extraction creates one Pending candidate even after a repeated Done update.
4. Verify the `/knowledge` SPA route is served.

### E2E full (local)

- Edit candidate before approve
- Reject and verify absent from retrieval
- View source ticket link from knowledge item

## Acceptance criteria

- [ ] The design gate is committed before persistence implementation.
- [ ] Manual candidates support concurrency-safe approve, edit, reject, supersede, expire, and stale operations with immutable provenance.
- [ ] Only active, embedding-ready, in-scope, confident, unexpired, and unsuperseded knowledge enters Full runs.
- [ ] Relational filtering precedes bounded stable cosine ranking and representative query plans use the documented indexes.
- [ ] Selected-token-counter context totals stay within configuration while mandatory safety and result-contract sections are preserved.
- [ ] Every included exact revision is logged at most once per run and appears under Knowledge Used.
- [ ] Done transitions durably and idempotently schedule deterministic extraction.
- [ ] Default extraction is Pending; only explicitly allowlisted, high-confidence, low-risk types can auto-save.
- [ ] Knowledge UI exposes Pending, Approved, Rejected, and Stale views plus source, embedding, expiry, supersession, and usage metadata.
- [ ] Targeted Rust/web tests and the distinct default-Compose knowledge smoke pass without changing the existing M06 context smoke.

## References

- Product design §13 (knowledge & self-learning), §16 (context package knowledge section)
- Framework selection §3 (pgvector), §8 (embedding provider)
