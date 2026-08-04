ALTER TABLE knowledge_jobs
    ADD COLUMN claim_token UUID;

-- A process may restart with the same stable worker name. Requeue any legacy
-- in-flight rows so every running claim created after this migration has a
-- unique lease token instead of relying on that reusable name alone.
UPDATE knowledge_jobs
SET status = 'pending',
    available_at = now(),
    locked_at = NULL,
    locked_by = NULL,
    claim_token = NULL,
    updated_at = now(),
    last_error = COALESCE(last_error, 'migration 015: reclaimed legacy running job')
WHERE status = 'running';

ALTER TABLE knowledge_jobs
    ADD CONSTRAINT knowledge_jobs_running_claim_check CHECK (
        (status = 'running'
            AND locked_at IS NOT NULL
            AND locked_by IS NOT NULL
            AND claim_token IS NOT NULL)
        OR
        (status <> 'running'
            AND locked_at IS NULL
            AND locked_by IS NULL
            AND claim_token IS NULL)
    );
