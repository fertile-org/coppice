CREATE TABLE ticket_mentions (
    id UUID PRIMARY KEY,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    comment_id UUID NOT NULL REFERENCES ticket_comments(id) ON DELETE CASCADE,
    mentioned_agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    resume_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    handled_at TIMESTAMPTZ
);

CREATE INDEX ticket_mentions_ticket_status_idx
    ON ticket_mentions (ticket_id, status);

ALTER TABLE tickets
    ADD COLUMN pending_assign_recommendation JSONB,
    ADD COLUMN clarification_round INT NOT NULL DEFAULT 0;
