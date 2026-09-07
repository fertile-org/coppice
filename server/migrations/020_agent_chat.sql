CREATE TABLE chat_sessions (
    id UUID PRIMARY KEY,
    project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE RESTRICT,
    repo_id UUID REFERENCES repos(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'archived', 'cutoff')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX chat_sessions_owner_updated_idx
    ON chat_sessions (owner_user_id, updated_at DESC);
CREATE INDEX chat_sessions_project_updated_idx
    ON chat_sessions (project_id, updated_at DESC);

CREATE TABLE chat_messages (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    seq BIGINT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('human', 'agent', 'system')),
    body TEXT NOT NULL,
    agent_run_id UUID REFERENCES agent_runs(id) ON DELETE SET NULL,
    action_metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (session_id, seq)
);

CREATE INDEX chat_messages_session_seq_idx
    ON chat_messages (session_id, seq);

ALTER TABLE agent_runs ALTER COLUMN ticket_id DROP NOT NULL;
ALTER TABLE agent_runs
    ADD COLUMN chat_session_id UUID REFERENCES chat_sessions(id) ON DELETE CASCADE,
    ADD COLUMN chat_message_id UUID REFERENCES chat_messages(id) ON DELETE SET NULL;

ALTER TABLE agent_runs
    ADD CONSTRAINT agent_runs_ticket_xor_chat_check CHECK (
        (ticket_id IS NOT NULL AND chat_session_id IS NULL)
        OR (ticket_id IS NULL AND chat_session_id IS NOT NULL)
    );

DROP INDEX IF EXISTS agent_runs_active_ticket_agent_idx;
CREATE UNIQUE INDEX agent_runs_active_ticket_agent_idx
    ON agent_runs (ticket_id, agent_id)
    WHERE status IN ('queued', 'running') AND ticket_id IS NOT NULL;
CREATE UNIQUE INDEX agent_runs_active_chat_session_agent_idx
    ON agent_runs (chat_session_id, agent_id)
    WHERE status IN ('queued', 'running') AND chat_session_id IS NOT NULL;

ALTER TABLE agent_runs DROP CONSTRAINT agent_runs_context_profile_check;
ALTER TABLE agent_runs
    ADD CONSTRAINT agent_runs_context_profile_check
    CHECK (context_profile IN ('full', 'human_agent', 'human_chat', 'conversation'));
