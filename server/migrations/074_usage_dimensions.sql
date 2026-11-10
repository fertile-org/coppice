-- M15 D5: record each run's connector, and keep token usage when the run,
-- ticket, agent or board it came from is deleted.

-- The connector that ran this run, written when the run starts. NULL for runs
-- that never started and for runs from before this column. `session_connector`
-- is only written when the CLI returns a session id, so it fills older rows
-- where it can; the agent's current connector is never used.
ALTER TABLE agent_runs ADD COLUMN connector TEXT;
UPDATE agent_runs SET connector = session_connector WHERE session_connector IS NOT NULL;

-- run_usage gets its own id. It equals the run's id when written, and stays
-- after the run is deleted; `run_id` then becomes NULL. Request and subagent
-- rows hang off the usage row, so they stay with it.
ALTER TABLE run_llm_requests DROP CONSTRAINT run_llm_requests_run_id_fkey;
ALTER TABLE run_subagents DROP CONSTRAINT run_subagents_run_id_fkey;
ALTER TABLE run_usage DROP CONSTRAINT run_usage_run_id_fkey;
ALTER TABLE run_usage RENAME COLUMN run_id TO id;
ALTER TABLE run_usage ADD COLUMN run_id UUID UNIQUE REFERENCES agent_runs(id) ON DELETE SET NULL;
UPDATE run_usage SET run_id = id;

ALTER TABLE run_llm_requests RENAME COLUMN run_id TO usage_id;
ALTER TABLE run_llm_requests
    ADD CONSTRAINT run_llm_requests_usage_id_fkey
    FOREIGN KEY (usage_id) REFERENCES run_usage(id) ON DELETE CASCADE;
ALTER TABLE run_subagents RENAME COLUMN run_id TO usage_id;
ALTER TABLE run_subagents
    ADD CONSTRAINT run_subagents_usage_id_fkey
    FOREIGN KEY (usage_id) REFERENCES run_usage(id) ON DELETE CASCADE;

-- What the usage is grouped by, copied from the run while it exists. Names are
-- kept so a deleted agent or board still has a label.
ALTER TABLE run_usage
    ADD COLUMN connector TEXT,
    ADD COLUMN agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    ADD COLUMN agent_name TEXT,
    ADD COLUMN job_type TEXT,
    ADD COLUMN ticket_id UUID REFERENCES tickets(id) ON DELETE SET NULL,
    -- The top of the ticket's split tree (the ticket itself when it has no parent).
    ADD COLUMN root_ticket_id UUID REFERENCES tickets(id) ON DELETE SET NULL,
    -- NULL for runs without a board (chat, compaction, connector checks): Workspace.
    ADD COLUMN board_id UUID REFERENCES boards(id) ON DELETE SET NULL,
    ADD COLUMN board_name TEXT,
    ADD COLUMN chat_session_id UUID REFERENCES chat_sessions(id) ON DELETE SET NULL,
    ADD COLUMN reported_model TEXT,
    -- When the run was created; `updated_at` is when its last snapshot was written.
    ADD COLUMN run_created_at TIMESTAMPTZ;

WITH RECURSIVE up AS (
    SELECT t.id AS start_id, t.id, t.parent_ticket_id
    FROM tickets t
    WHERE t.id IN (SELECT ar.ticket_id FROM agent_runs ar JOIN run_usage u ON u.run_id = ar.id)
    UNION ALL
    SELECT up.start_id, p.id, p.parent_ticket_id
    FROM up JOIN tickets p ON p.id = up.parent_ticket_id
),
roots AS (
    SELECT start_id, id AS root_id FROM up WHERE parent_ticket_id IS NULL
)
UPDATE run_usage u SET
    connector = ar.connector,
    agent_id = ar.agent_id,
    agent_name = a.name,
    job_type = ar.job_type,
    ticket_id = ar.ticket_id,
    root_ticket_id = roots.root_id,
    board_id = t.board_id,
    board_name = b.name,
    chat_session_id = ar.chat_session_id,
    reported_model = ar.reported_model,
    run_created_at = ar.created_at
FROM agent_runs ar
LEFT JOIN agents a ON a.id = ar.agent_id
LEFT JOIN tickets t ON t.id = ar.ticket_id
LEFT JOIN boards b ON b.id = t.board_id
LEFT JOIN roots ON roots.start_id = ar.ticket_id
WHERE ar.id = u.run_id;
