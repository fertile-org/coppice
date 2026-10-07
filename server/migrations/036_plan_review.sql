ALTER TABLE tickets
    ADD COLUMN skip_planning BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN content_version INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN approved_plan_comment_id UUID,
    ADD COLUMN approved_plan_content_version INTEGER;

ALTER TABLE ticket_comments
    ADD COLUMN plan_content_version INTEGER;

ALTER TABLE tickets
    ADD CONSTRAINT tickets_approved_plan_comment_id_fkey
    FOREIGN KEY (approved_plan_comment_id)
    REFERENCES ticket_comments (id)
    ON DELETE SET NULL;

-- One active planning run per ticket. A second drop does not insert another.
CREATE UNIQUE INDEX agent_runs_one_active_plan_idx
    ON agent_runs (ticket_id)
    WHERE job_type = 'plan_ticket'
      AND status IN ('queued', 'running')
      AND ticket_id IS NOT NULL;
