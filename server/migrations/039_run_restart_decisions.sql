CREATE TABLE run_restart_decisions (
    run_id UUID PRIMARY KEY REFERENCES agent_runs(id) ON DELETE CASCADE,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    changed_files INTEGER NOT NULL CHECK (changed_files >= 0),
    previous_status TEXT NOT NULL,
    resolved_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX run_restart_decisions_one_open
    ON run_restart_decisions (ticket_id)
    WHERE resolved_at IS NULL;
