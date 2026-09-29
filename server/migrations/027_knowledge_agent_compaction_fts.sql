-- Knowledge: agent compaction replaces mock extraction; full-text search replaces embeddings.

CREATE EXTENSION IF NOT EXISTS unaccent;

-- unaccent() is STABLE (dictionary lookup); generated columns need IMMUTABLE.
CREATE OR REPLACE FUNCTION coppice_unaccent(value TEXT)
RETURNS TEXT
LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT
AS $$ SELECT public.unaccent('public.unaccent'::regdictionary, value) $$;

-- Workspace settings (single row).
CREATE TABLE workspace_settings (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    knowledge_compaction_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO workspace_settings DEFAULT VALUES;

-- Compaction batches, queue, and membership.
CREATE TABLE knowledge_compaction_batches (
    id UUID PRIMARY KEY,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed')),
    trigger TEXT NOT NULL CHECK (trigger IN ('scheduled', 'manual', 'retry')),
    cycle_id UUID NOT NULL,
    cycle_started_at TIMESTAMPTZ NOT NULL,
    ticket_count INT NOT NULL CHECK (ticket_count > 0),
    candidate_count INT CHECK (candidate_count >= 0),
    summary TEXT,
    error_message TEXT,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX knowledge_compaction_one_active_batch
    ON knowledge_compaction_batches ((true))
    WHERE status IN ('queued', 'running');
CREATE INDEX knowledge_compaction_batches_created_idx
    ON knowledge_compaction_batches (created_at DESC, id DESC);

CREATE TABLE knowledge_compaction_queue (
    ticket_id UUID PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
    enqueued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    batch_id UUID REFERENCES knowledge_compaction_batches(id) ON DELETE SET NULL,
    attempts INT NOT NULL DEFAULT 0 CHECK (attempts >= 0)
);

CREATE INDEX knowledge_compaction_queue_eligible_idx
    ON knowledge_compaction_queue (enqueued_at, ticket_id)
    WHERE batch_id IS NULL;

CREATE TABLE knowledge_compaction_batch_tickets (
    batch_id UUID NOT NULL REFERENCES knowledge_compaction_batches(id) ON DELETE CASCADE,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    PRIMARY KEY (batch_id, ticket_id)
);

-- Compaction runs are agent runs owned by a batch.
ALTER TABLE agent_runs
    ADD COLUMN compaction_batch_id UUID
        REFERENCES knowledge_compaction_batches(id) ON DELETE CASCADE;

ALTER TABLE agent_runs DROP CONSTRAINT agent_runs_ticket_xor_chat_check;
ALTER TABLE agent_runs
    ADD CONSTRAINT agent_runs_owner_check CHECK (
        num_nonnulls(ticket_id, chat_session_id, compaction_batch_id) = 1
    );

CREATE UNIQUE INDEX agent_runs_compaction_batch_idx
    ON agent_runs (compaction_batch_id)
    WHERE compaction_batch_id IS NOT NULL;

ALTER TABLE agent_runs DROP CONSTRAINT agent_runs_context_profile_check;
ALTER TABLE agent_runs
    ADD CONSTRAINT agent_runs_context_profile_check
    CHECK (context_profile IN (
        'full', 'human_agent', 'human_chat', 'conversation', 'knowledge_compaction'
    ));

-- Knowledge provenance from compaction.
ALTER TABLE knowledge_items
    ADD COLUMN compaction_batch_id UUID
        REFERENCES knowledge_compaction_batches(id) ON DELETE SET NULL,
    ADD COLUMN compaction_candidate_index INT CHECK (compaction_candidate_index >= 0),
    ADD CONSTRAINT knowledge_items_compaction_pair_check CHECK (
        (compaction_batch_id IS NULL) = (compaction_candidate_index IS NULL)
    ),
    ADD CONSTRAINT knowledge_items_compaction_candidate_uniq
        UNIQUE (compaction_batch_id, compaction_candidate_index);

CREATE TABLE knowledge_item_sources (
    item_id UUID NOT NULL REFERENCES knowledge_items(id) ON DELETE CASCADE,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, ticket_id)
);
CREATE INDEX knowledge_item_sources_ticket_idx ON knowledge_item_sources (ticket_id);

-- Full-text search over revisions (ALTER rewrites rows without firing UPDATE triggers).
ALTER TABLE knowledge_revisions
    ADD COLUMN search_vector tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector('simple'::regconfig, coppice_unaccent(title)), 'A')
        || setweight(to_tsvector('simple'::regconfig, coppice_unaccent(content)), 'B')
    ) STORED;
CREATE INDEX knowledge_revisions_search_idx
    ON knowledge_revisions USING gin (search_vector);

-- Failure notifications for compaction (no ticket).
ALTER TABLE notifications DROP CONSTRAINT notifications_type_check;
ALTER TABLE notifications
    ADD CONSTRAINT notifications_type_check CHECK (
        type IN ('agent_run_finished', 'agent_mentioned', 'knowledge_compaction_failed')
    );

-- Approved items that never finished embedding become retrievable now.
UPDATE knowledge_items
SET active_revision_id = current_revision_id, updated_at = now()
WHERE status = 'approved'
  AND active_revision_id IS NULL
  AND superseded_by IS NULL
  AND current_revision_id IS NOT NULL;

-- Replace the extraction trigger with the compaction queue trigger.
DROP TRIGGER tickets_schedule_knowledge_extraction ON tickets;
DROP FUNCTION schedule_knowledge_extraction_on_done();

CREATE OR REPLACE FUNCTION enqueue_knowledge_compaction_on_status()
RETURNS trigger AS $$
BEGIN
    IF NEW.status = 'done' AND OLD.status IS DISTINCT FROM 'done' THEN
        INSERT INTO knowledge_compaction_queue (ticket_id)
        VALUES (NEW.id)
        ON CONFLICT (ticket_id) DO NOTHING;
    ELSIF OLD.status = 'done' AND NEW.status IS DISTINCT FROM 'done' THEN
        DELETE FROM knowledge_compaction_queue
        WHERE ticket_id = NEW.id AND batch_id IS NULL;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tickets_enqueue_knowledge_compaction
AFTER UPDATE OF status ON tickets
FOR EACH ROW EXECUTE FUNCTION enqueue_knowledge_compaction_on_status();

-- Drop embedding and extraction machinery.
ALTER TABLE knowledge_items DROP CONSTRAINT knowledge_items_extraction_job_fk;
ALTER TABLE knowledge_items
    DROP COLUMN extraction_job_id,
    DROP COLUMN extraction_candidate_index;
DROP TABLE knowledge_jobs;
DROP TABLE knowledge_embeddings;

ALTER TABLE knowledge_usage_logs RENAME COLUMN similarity TO score;
