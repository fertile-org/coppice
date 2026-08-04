-- M06: governed, revisioned knowledge with durable extraction/embedding jobs.

CREATE TABLE knowledge_items (
    id UUID PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'rejected', 'stale')),
    version INT NOT NULL DEFAULT 1 CHECK (version > 0),
    current_revision_id UUID,
    active_revision_id UUID,
    approved_by UUID REFERENCES users(id) ON DELETE SET NULL,
    approved_at TIMESTAMPTZ,
    approval_mode TEXT CHECK (approval_mode IN ('human', 'policy')),
    policy_decision TEXT,
    policy_reason TEXT,
    rejection_reason TEXT,
    stale_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    supersedes_item_id UUID REFERENCES knowledge_items(id) ON DELETE SET NULL,
    superseded_by UUID REFERENCES knowledge_items(id) ON DELETE SET NULL,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    extraction_job_id UUID,
    extraction_candidate_index INT CHECK (extraction_candidate_index >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (supersedes_item_id IS NULL OR supersedes_item_id <> id),
    CHECK (superseded_by IS NULL OR superseded_by <> id),
    CHECK (
        (extraction_job_id IS NULL AND extraction_candidate_index IS NULL)
        OR (extraction_job_id IS NOT NULL AND extraction_candidate_index IS NOT NULL)
    ),
    UNIQUE (extraction_job_id, extraction_candidate_index)
);

CREATE TABLE knowledge_revisions (
    id UUID PRIMARY KEY,
    item_id UUID NOT NULL REFERENCES knowledge_items(id) ON DELETE CASCADE,
    revision_number INT NOT NULL CHECK (revision_number > 0),
    scope TEXT NOT NULL CHECK (scope IN ('workspace', 'project', 'agent')),
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
    knowledge_type TEXT NOT NULL CHECK (
        knowledge_type IN (
            'coding_convention', 'architecture_rule', 'bug_pattern',
            'test_command', 'review_feedback', 'dependency_note',
            'api_contract', 'workflow_rule', 'human_preference',
            'operational_runbook', 'security_rule', 'performance_note'
        )
    ),
    title TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 160),
    content TEXT NOT NULL CHECK (char_length(content) BETWEEN 1 AND 12000),
    source_type TEXT NOT NULL CHECK (
        source_type IN (
            'ticket', 'comment', 'review', 'human_note', 'agent_summary',
            'workspace_signal', 'observation_run'
        )
    ),
    source_id UUID,
    source_run_id UUID REFERENCES agent_runs(id) ON DELETE SET NULL,
    confidence TEXT NOT NULL CHECK (confidence IN ('low', 'medium', 'high')),
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (item_id, revision_number),
    CHECK (
        (scope = 'workspace' AND project_id IS NULL AND agent_id IS NULL)
        OR (scope = 'project' AND project_id IS NOT NULL AND agent_id IS NULL)
        OR (scope = 'agent' AND project_id IS NOT NULL AND agent_id IS NOT NULL)
    )
);

ALTER TABLE knowledge_items
    ADD CONSTRAINT knowledge_items_current_revision_fk
        FOREIGN KEY (current_revision_id) REFERENCES knowledge_revisions(id) ON DELETE RESTRICT,
    ADD CONSTRAINT knowledge_items_active_revision_fk
        FOREIGN KEY (active_revision_id) REFERENCES knowledge_revisions(id) ON DELETE RESTRICT;

CREATE OR REPLACE FUNCTION prevent_knowledge_revision_update()
RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'knowledge revisions are immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER knowledge_revisions_immutable
BEFORE UPDATE ON knowledge_revisions
FOR EACH ROW EXECUTE FUNCTION prevent_knowledge_revision_update();

CREATE TABLE knowledge_embeddings (
    revision_id UUID PRIMARY KEY REFERENCES knowledge_revisions(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    embedding_dimension INT NOT NULL CHECK (embedding_dimension = 1536),
    embedding vector(1536) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX knowledge_embeddings_hnsw_cosine_idx
    ON knowledge_embeddings USING hnsw (embedding vector_cosine_ops);

CREATE TABLE knowledge_usage_logs (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES knowledge_items(id) ON DELETE RESTRICT,
    revision_id UUID NOT NULL REFERENCES knowledge_revisions(id) ON DELETE RESTRICT,
    rank INT NOT NULL CHECK (rank > 0),
    similarity DOUBLE PRECISION NOT NULL,
    token_count INT NOT NULL CHECK (token_count > 0),
    rendered_content TEXT NOT NULL,
    included_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, revision_id)
);

CREATE INDEX knowledge_usage_logs_run_rank_idx
    ON knowledge_usage_logs (run_id, rank, revision_id);
CREATE INDEX knowledge_usage_logs_item_time_idx
    ON knowledge_usage_logs (item_id, included_at DESC);

CREATE TABLE knowledge_jobs (
    id UUID PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('embed_revision', 'extract_ticket')),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    ticket_id UUID REFERENCES tickets(id) ON DELETE CASCADE,
    revision_id UUID REFERENCES knowledge_revisions(id) ON DELETE CASCADE,
    attempts INT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts INT NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    locked_by TEXT,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CHECK (
        (kind = 'embed_revision' AND revision_id IS NOT NULL AND ticket_id IS NULL)
        OR (kind = 'extract_ticket' AND ticket_id IS NOT NULL AND revision_id IS NULL)
    )
);

ALTER TABLE knowledge_items
    ADD CONSTRAINT knowledge_items_extraction_job_fk
        FOREIGN KEY (extraction_job_id) REFERENCES knowledge_jobs(id) ON DELETE SET NULL;

CREATE UNIQUE INDEX knowledge_jobs_embed_revision_uniq
    ON knowledge_jobs (revision_id) WHERE kind = 'embed_revision';
CREATE UNIQUE INDEX knowledge_jobs_extract_ticket_uniq
    ON knowledge_jobs (ticket_id) WHERE kind = 'extract_ticket';
CREATE INDEX knowledge_jobs_claim_idx
    ON knowledge_jobs (available_at, created_at, id)
    WHERE status = 'pending';
CREATE INDEX knowledge_jobs_stale_lock_idx
    ON knowledge_jobs (locked_at)
    WHERE status = 'running';

CREATE INDEX knowledge_items_list_idx
    ON knowledge_items (status, updated_at DESC, id DESC);
CREATE INDEX knowledge_items_retrieval_state_idx
    ON knowledge_items (status, superseded_by, expires_at, active_revision_id)
    WHERE status = 'approved';
CREATE INDEX knowledge_revisions_project_scope_idx
    ON knowledge_revisions (project_id, scope, confidence, knowledge_type, id);
CREATE INDEX knowledge_revisions_agent_scope_idx
    ON knowledge_revisions (agent_id, scope, confidence, knowledge_type, id);
CREATE INDEX knowledge_revisions_workspace_scope_idx
    ON knowledge_revisions (scope, confidence, knowledge_type, id)
    WHERE scope = 'workspace';

CREATE OR REPLACE FUNCTION schedule_knowledge_extraction_on_done()
RETURNS trigger AS $$
BEGIN
    IF NEW.status = 'done' AND OLD.status IS DISTINCT FROM 'done' THEN
        INSERT INTO knowledge_jobs (id, kind, ticket_id)
        VALUES (gen_random_uuid(), 'extract_ticket', NEW.id)
        ON CONFLICT (ticket_id) WHERE kind = 'extract_ticket' DO NOTHING;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tickets_schedule_knowledge_extraction
AFTER UPDATE OF status ON tickets
FOR EACH ROW EXECUTE FUNCTION schedule_knowledge_extraction_on_done();
