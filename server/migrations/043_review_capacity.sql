-- Per-board review capacity.
-- review_cap has no default number: existing boards and new boards stay
-- empty (NULL), which means no cap.
-- Tickets reopened by a merge conflict keep review_cap_exempt until they
-- re-enter Wait for Human Review.

ALTER TABLE boards
    ADD COLUMN review_cap INTEGER,
    ADD COLUMN review_cap_counts_in_review BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE boards
    ADD CONSTRAINT boards_review_cap_at_least_one
        CHECK (review_cap IS NULL OR review_cap >= 1);

ALTER TABLE tickets
    ADD COLUMN review_cap_exempt BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN review_hold BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN review_entry_held BOOLEAN NOT NULL DEFAULT false;
