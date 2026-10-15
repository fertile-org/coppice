-- Per-board review capacity and the diff budget agreed when a plan is approved.
-- Tickets reopened by a merge conflict keep review_cap_exempt until they
-- re-enter Wait for Human Review.

ALTER TABLE boards
    ADD COLUMN review_cap INTEGER,
    ADD COLUMN review_cap_counts_in_review BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN diff_budget INTEGER;

ALTER TABLE boards
    ADD CONSTRAINT boards_review_cap_nonneg
        CHECK (review_cap IS NULL OR review_cap >= 0),
    ADD CONSTRAINT boards_diff_budget_nonneg
        CHECK (diff_budget IS NULL OR diff_budget >= 0);

ALTER TABLE tickets
    ADD COLUMN review_cap_exempt BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN review_hold BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN review_entry_held BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN agreed_diff_budget INTEGER,
    ADD COLUMN diff_budget_agreed BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE tickets
    ADD CONSTRAINT tickets_agreed_diff_budget_nonneg
        CHECK (agreed_diff_budget IS NULL OR agreed_diff_budget >= 0);
