-- Review-round stop. A ticket remembers how many times a reviewer sent
-- work back, the commit that round covered, and the findings so far.
-- review_escalated is set when the one reconsider comment has been posted.

ALTER TABLE tickets
    ADD COLUMN review_round INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN review_anchor_sha TEXT,
    ADD COLUMN review_findings TEXT NOT NULL DEFAULT '',
    ADD COLUMN review_escalated BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE tickets
    ADD CONSTRAINT tickets_review_round_non_negative
        CHECK (review_round >= 0);
