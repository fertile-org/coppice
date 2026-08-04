-- Updating source_run_id during run deletion conflicts with immutable knowledge
-- revisions and would discard provenance. Preserve the referenced run instead.
ALTER TABLE knowledge_revisions
    DROP CONSTRAINT knowledge_revisions_source_run_id_fkey,
    ADD CONSTRAINT knowledge_revisions_source_run_id_fkey
        FOREIGN KEY (source_run_id) REFERENCES agent_runs(id) ON DELETE RESTRICT;

CREATE INDEX knowledge_revisions_source_run_id_idx
    ON knowledge_revisions (source_run_id)
    WHERE source_run_id IS NOT NULL;
