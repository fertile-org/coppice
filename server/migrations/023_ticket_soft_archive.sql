-- Soft-archive tickets (orthogonal to board status)

ALTER TABLE tickets ADD COLUMN archived_at TIMESTAMPTZ NULL;

CREATE INDEX tickets_project_archived_at_idx ON tickets (project_id, archived_at);
