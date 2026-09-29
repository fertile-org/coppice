CREATE TABLE run_tool_tokens (
    id UUID PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    subject_kind TEXT NOT NULL DEFAULT 'run' CHECK (subject_kind IN ('run', 'personal')),
    run_id UUID REFERENCES agent_runs(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL,
    ticket_id UUID,
    chat_session_id UUID,
    board_id UUID,
    context_profile TEXT NOT NULL,
    job_type TEXT NOT NULL,
    compaction_ticket_ids UUID[] NOT NULL DEFAULT '{}',
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX run_tool_tokens_run_idx ON run_tool_tokens (run_id);

CREATE TABLE run_tool_calls (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    tool TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('core', 'skill', 'plugin')),
    plugin_id UUID,
    args_summary TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL CHECK (status IN ('ok', 'error', 'denied', 'timeout')),
    error TEXT,
    duration_ms INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX run_tool_calls_run_idx ON run_tool_calls (run_id, created_at);

ALTER TABLE agent_runs ADD COLUMN submitted_result JSONB;
