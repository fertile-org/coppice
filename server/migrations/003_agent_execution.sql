CREATE TABLE agent_runs (
    id UUID PRIMARY KEY,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL,
    sandbox_profile_id TEXT NOT NULL,
    worktree_path TEXT,
    branch_name TEXT,
    error_message TEXT,
    started_at TIMESTAMPTZ,
    ended_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX agent_runs_ticket_id_created_at_idx
    ON agent_runs (ticket_id, created_at DESC);
CREATE INDEX agent_runs_agent_id_idx ON agent_runs (agent_id);

CREATE UNIQUE INDEX agent_runs_active_ticket_agent_idx
    ON agent_runs (ticket_id, agent_id)
    WHERE status IN ('queued', 'running');

CREATE TABLE agent_jobs (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL,
    attempts INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 3,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    locked_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX agent_jobs_pending_idx
    ON agent_jobs (status, available_at)
    WHERE status = 'pending';
