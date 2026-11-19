-- Plugin builder plan/build runs: agent_runs owned by a plugin_builds row.

ALTER TABLE plugin_builds
    ADD COLUMN plan_questions JSONB;

ALTER TABLE agent_runs
    ADD COLUMN plugin_build_id UUID
        REFERENCES plugin_builds(id) ON DELETE CASCADE;

ALTER TABLE agent_runs DROP CONSTRAINT agent_runs_owner_check;
ALTER TABLE agent_runs
    ADD CONSTRAINT agent_runs_owner_check CHECK (
        num_nonnulls(
            ticket_id,
            chat_session_id,
            compaction_batch_id,
            connector_check_id,
            plugin_build_id
        ) = 1
    );

-- One active (queued/running) run per build — same idea as chat's one-active-per-session.
CREATE UNIQUE INDEX agent_runs_one_active_plugin_build_idx
    ON agent_runs (plugin_build_id)
    WHERE plugin_build_id IS NOT NULL
      AND status IN ('queued', 'running');
