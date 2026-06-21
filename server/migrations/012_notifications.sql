CREATE TABLE notifications (
    id UUID PRIMARY KEY,
    recipient_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    type TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT,
    ticket_id UUID REFERENCES tickets(id) ON DELETE CASCADE,
    run_id UUID,
    agent_id UUID,
    comment_id UUID,
    mention_id UUID,
    source_key TEXT NOT NULL,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT notifications_type_check CHECK (type IN ('agent_run_finished', 'agent_mentioned'))
);

CREATE UNIQUE INDEX notifications_recipient_source_uniq
    ON notifications (recipient_user_id, source_key);

CREATE INDEX notifications_recipient_unread_idx
    ON notifications (recipient_user_id, read_at, created_at DESC);

CREATE INDEX notifications_recipient_list_idx
    ON notifications (recipient_user_id, created_at DESC, id DESC);
