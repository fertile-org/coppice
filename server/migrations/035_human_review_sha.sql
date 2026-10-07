-- Acceptance in Wait for Human Review is bound to one commit.
-- comment_ids: review comments and QA check comments that existed then.
-- run_ids: finished runs (and any run that still holds a submitted result).

CREATE TABLE ticket_human_reviews (
    ticket_id UUID PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
    head_sha TEXT NOT NULL,
    comment_ids UUID[] NOT NULL DEFAULT '{}',
    run_ids UUID[] NOT NULL DEFAULT '{}',
    stale BOOLEAN NOT NULL DEFAULT false,
    accepted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    accepted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    invalidated_at TIMESTAMPTZ
);
