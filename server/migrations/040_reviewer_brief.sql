CREATE TABLE ticket_reviewer_briefs (
    ticket_id UUID PRIMARY KEY REFERENCES tickets (id) ON DELETE CASCADE,
    how_it_works TEXT,
    changes_from_plan TEXT,
    look_here_first TEXT,
    -- Commit the brief was written for. Human Review shows the brief only
    -- when this is the commit that review is bound to.
    commit_sha TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
