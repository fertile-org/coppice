-- Connector checks: a "Test connection" agent run owned by a check row.

CREATE TABLE connector_checks (
    id UUID PRIMARY KEY,
    connector TEXT NOT NULL,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'passed', 'failed')),
    failure TEXT,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX connector_checks_connector_created_idx
    ON connector_checks (connector, created_at DESC);
CREATE UNIQUE INDEX connector_checks_one_active_per_connector
    ON connector_checks (connector)
    WHERE status IN ('queued', 'running');

ALTER TABLE agent_runs
    ADD COLUMN connector_check_id UUID
        REFERENCES connector_checks(id) ON DELETE CASCADE;

ALTER TABLE agent_runs DROP CONSTRAINT agent_runs_owner_check;
ALTER TABLE agent_runs
    ADD CONSTRAINT agent_runs_owner_check CHECK (
        num_nonnulls(ticket_id, chat_session_id, compaction_batch_id, connector_check_id) = 1
    );

CREATE UNIQUE INDEX agent_runs_connector_check_idx
    ON agent_runs (connector_check_id)
    WHERE connector_check_id IS NOT NULL;

ALTER TABLE agent_runs DROP CONSTRAINT agent_runs_context_profile_check;
ALTER TABLE agent_runs
    ADD CONSTRAINT agent_runs_context_profile_check
    CHECK (context_profile IN (
        'full', 'human_agent', 'human_chat', 'conversation', 'knowledge_compaction',
        'connector_check'
    ));
