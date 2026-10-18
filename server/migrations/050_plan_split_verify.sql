-- Tickets split into sub-tickets at Plan Review (#153). The parent's approved
-- plan is the umbrella plan; the parent gets no implementation run and is
-- verified against its acceptance criteria once every sub-ticket is merged.
CREATE TABLE ticket_plan_splits (
    parent_ticket_id UUID PRIMARY KEY REFERENCES tickets(id) ON DELETE CASCADE,
    plan_comment_id UUID REFERENCES ticket_comments(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
