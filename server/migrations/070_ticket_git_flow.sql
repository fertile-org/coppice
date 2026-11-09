ALTER TABLE repos
    ADD COLUMN merge_after_accept BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN push_after_merge BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE tickets
    ADD COLUMN skip_human_review BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN agent_review_passed_at TIMESTAMPTZ;

ALTER TABLE ticket_plans
    ADD COLUMN suggest_skip_review BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE ticket_human_reviews
    ADD COLUMN accepted_by_coppice BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN merged_base_branch TEXT,
    ADD COLUMN git_outcome TEXT,
    ADD COLUMN git_outcome_reason TEXT,
    ADD COLUMN git_outcome_at TIMESTAMPTZ;
