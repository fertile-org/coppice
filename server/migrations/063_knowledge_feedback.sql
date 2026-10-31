-- Agent verdicts on knowledge revisions. run_id has no foreign key, matching knowledge_provenance.
CREATE TABLE knowledge_feedback (
    id UUID PRIMARY KEY,
    item_id UUID NOT NULL REFERENCES knowledge_items(id) ON DELETE CASCADE,
    revision_id UUID NOT NULL,
    run_id UUID,
    agent_id UUID,
    verdict TEXT NOT NULL CHECK (verdict IN ('outdated', 'wrong', 'confirmed')),
    reason TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    dismissed_at TIMESTAMPTZ
);
CREATE UNIQUE INDEX knowledge_feedback_run_revision_idx ON knowledge_feedback(run_id, revision_id);
CREATE INDEX knowledge_feedback_item_idx ON knowledge_feedback(item_id, revision_id);
CREATE INDEX knowledge_feedback_revision_idx ON knowledge_feedback(revision_id, created_at);

-- A flag is open until a reviewer dismisses it or a later `confirmed` on the same revision closes it.
CREATE VIEW knowledge_open_flags AS
SELECT f.*
FROM knowledge_feedback f
WHERE f.verdict IN ('outdated', 'wrong')
  AND f.dismissed_at IS NULL
  AND NOT EXISTS (
      SELECT 1 FROM knowledge_feedback c
      WHERE c.revision_id = f.revision_id
        AND c.verdict = 'confirmed'
        AND c.created_at >= f.created_at
  );
