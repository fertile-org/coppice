-- A knowledge usage snapshot is durable audit evidence for the exact run that
-- consumed a revision. Refuse deletion of that run instead of cascading away
-- the audit trail.
ALTER TABLE knowledge_usage_logs
    DROP CONSTRAINT knowledge_usage_logs_run_id_fkey,
    ADD CONSTRAINT knowledge_usage_logs_run_id_fkey
        FOREIGN KEY (run_id) REFERENCES agent_runs(id) ON DELETE RESTRICT;
