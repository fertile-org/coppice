-- Agent verdicts on knowledge revisions. run_id has no foreign key, matching knowledge_provenance.
CREATE TABLE knowledge_feedback (
    id UUID PRIMARY KEY,
    item_id UUID NOT NULL REFERENCES knowledge_items(id) ON DELETE CASCADE,
    revision_id UUID NOT NULL,
    run_id UUID,
    agent_id UUID,
    verdict TEXT NOT NULL CHECK (verdict IN ('outdated', 'wrong', 'confirmed')),
    reason TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX knowledge_feedback_run_revision_idx ON knowledge_feedback(run_id, revision_id);
CREATE INDEX knowledge_feedback_item_idx ON knowledge_feedback(item_id, revision_id);
