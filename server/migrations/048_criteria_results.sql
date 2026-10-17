-- The latest done result's per-criterion report (#150), bound to the commit it
-- was written for. Human Review reads it only for the reviewed commit.
CREATE TABLE ticket_criteria_results (
    ticket_id UUID PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
    commit_sha TEXT NOT NULL,
    results JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
