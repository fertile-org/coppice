CREATE TABLE design_sets (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL,
    ticket_id UUID REFERENCES tickets(id) ON DELETE CASCADE,
    chat_session_id UUID REFERENCES chat_sessions(id) ON DELETE CASCADE,
    run_id UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id),
    dir TEXT NOT NULL,
    options JSONB NOT NULL DEFAULT '[]',
    chosen_option_id TEXT,
    chosen_by_user_id UUID REFERENCES users(id),
    chosen_at TIMESTAMPTZ,
    published_at TIMESTAMPTZ,
    superseded_by UUID REFERENCES design_sets(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT design_sets_one_scope CHECK (
        (ticket_id IS NOT NULL AND chat_session_id IS NULL)
        OR (ticket_id IS NULL AND chat_session_id IS NOT NULL)
    )
);
CREATE UNIQUE INDEX design_sets_one_published_per_run
    ON design_sets (run_id) WHERE published_at IS NOT NULL;
CREATE INDEX design_sets_ticket_idx ON design_sets (ticket_id, created_at DESC);
CREATE INDEX design_sets_chat_idx ON design_sets (chat_session_id, created_at DESC);
