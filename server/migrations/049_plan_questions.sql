-- Questions a planning run asked before writing the first plan (#151).
-- A row with answered_at NULL is the stored "waiting for your answers" state.
-- One row per ticket: at most one question round.
CREATE TABLE ticket_plan_questions (
    ticket_id UUID PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
    run_id UUID REFERENCES agent_runs(id) ON DELETE SET NULL,
    comment_id UUID REFERENCES ticket_comments(id) ON DELETE SET NULL,
    questions JSONB NOT NULL,
    answers JSONB,
    answers_comment_id UUID REFERENCES ticket_comments(id) ON DELETE SET NULL,
    asked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    answered_at TIMESTAMPTZ
);
