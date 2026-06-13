ALTER TABLE tickets
    ADD COLUMN parent_ticket_id UUID REFERENCES tickets(id) ON DELETE SET NULL,
    ADD COLUMN pending_split_recommendation JSONB;

CREATE INDEX tickets_parent_ticket_id_idx ON tickets(parent_ticket_id);
