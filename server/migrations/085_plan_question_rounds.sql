-- Up to three question rounds before the first plan.
-- Existing rows stay round 1. Each question object in `questions` may
-- include "recommended": the short answer the planner would pick.
ALTER TABLE ticket_plan_questions DROP CONSTRAINT ticket_plan_questions_pkey;

ALTER TABLE ticket_plan_questions
    ADD COLUMN round SMALLINT NOT NULL DEFAULT 1;

ALTER TABLE ticket_plan_questions
    ADD PRIMARY KEY (ticket_id, round);

ALTER TABLE ticket_plan_questions
    ADD CONSTRAINT ticket_plan_questions_round_range CHECK (round BETWEEN 1 AND 3);
