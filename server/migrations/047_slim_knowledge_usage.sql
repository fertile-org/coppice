-- Knowledge no longer pins agent_runs (#157).
--
-- Usage rows keep only the audit fields (revision, rank, score, token count).
-- The exact text a run saw is rebuilt from the immutable revision, so the
-- per-run copy is dropped. A deleted run leaves its usage rows with no run.
ALTER TABLE knowledge_usage_logs DROP COLUMN rendered_content;
ALTER TABLE knowledge_usage_logs
    ALTER COLUMN run_id DROP NOT NULL,
    DROP CONSTRAINT knowledge_usage_logs_run_id_fkey,
    ADD CONSTRAINT knowledge_usage_logs_run_id_fkey
        FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE SET NULL;

-- "This run produced knowledge" lives in its own small table. It copies what
-- it needs from the run and has no foreign key to agent_runs, so runs can be
-- purged while every revision keeps its provenance.
CREATE TABLE knowledge_provenance (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL UNIQUE,
    ticket_id UUID,
    agent_id UUID,
    job_type TEXT,
    run_created_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO knowledge_provenance (id, run_id, ticket_id, agent_id, job_type, run_created_at)
SELECT gen_random_uuid(), run.id, run.ticket_id, run.agent_id, run.job_type, run.created_at
FROM agent_runs run
WHERE run.id IN (
    SELECT DISTINCT source_run_id FROM knowledge_revisions WHERE source_run_id IS NOT NULL
);

ALTER TABLE knowledge_revisions
    ADD COLUMN provenance_id UUID REFERENCES knowledge_provenance(id) ON DELETE RESTRICT;

-- Revisions are immutable; this one-time backfill is the only update.
ALTER TABLE knowledge_revisions DISABLE TRIGGER knowledge_revisions_immutable;
UPDATE knowledge_revisions revision
SET provenance_id = provenance.id
FROM knowledge_provenance provenance
WHERE provenance.run_id = revision.source_run_id;
ALTER TABLE knowledge_revisions ENABLE TRIGGER knowledge_revisions_immutable;

DROP INDEX IF EXISTS knowledge_revisions_source_run_id_idx;
ALTER TABLE knowledge_revisions DROP COLUMN source_run_id;

CREATE INDEX knowledge_revisions_provenance_id_idx
    ON knowledge_revisions (provenance_id)
    WHERE provenance_id IS NOT NULL;
