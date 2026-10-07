-- Acceptance in Wait for Human Review is bound to one commit.
-- comment_ids: review comments and QA check comments that existed then.
-- run_ids: finished runs (and any run that still holds a submitted result).
-- merged_sha / merged_at: set by the merge path when that acceptance is merged.
-- Null until then. Never derived from comment text.

CREATE TABLE ticket_human_reviews (
    ticket_id UUID PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
    head_sha TEXT NOT NULL,
    comment_ids UUID[] NOT NULL DEFAULT '{}',
    run_ids UUID[] NOT NULL DEFAULT '{}',
    stale BOOLEAN NOT NULL DEFAULT false,
    accepted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    accepted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    invalidated_at TIMESTAMPTZ,
    merged_sha TEXT,
    merged_at TIMESTAMPTZ,
    CONSTRAINT ticket_human_reviews_merge_recorded CHECK (
        (merged_at IS NULL AND merged_sha IS NULL)
        OR (merged_at IS NOT NULL AND merged_sha IS NOT NULL)
    )
);
