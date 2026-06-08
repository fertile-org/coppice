ALTER TABLE repos
    ADD COLUMN local_path TEXT,
    ADD COLUMN verification_status TEXT NOT NULL DEFAULT 'path_missing',
    ADD COLUMN verification_error TEXT,
    ADD COLUMN last_verified_at TIMESTAMPTZ,
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- Dev/CI: truncate or backfill before NOT NULL enforcement in same migration:
UPDATE repos SET local_path = '/tmp/coppice-migration-placeholder' WHERE local_path IS NULL;
ALTER TABLE repos ALTER COLUMN local_path SET NOT NULL;

ALTER TABLE repos DROP CONSTRAINT IF EXISTS repos_project_id_fkey;
DROP INDEX IF EXISTS repos_project_id_idx;
ALTER TABLE repos DROP COLUMN project_id;

CREATE UNIQUE INDEX repos_local_path_idx ON repos (local_path);
