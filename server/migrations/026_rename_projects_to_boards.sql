-- Rename the board container entity: projects -> boards.
-- Renames the table, every FK column that referenced it, the dependent
-- indexes, and the knowledge scope value 'project' -> 'board'.

ALTER TABLE projects RENAME TO boards;

-- Postgres carries the old auto-generated names through RENAME TO; realign them
-- so future migrations can reference predictable constraint/index names.
ALTER TABLE boards RENAME CONSTRAINT projects_pkey TO boards_pkey;
ALTER INDEX projects_slug_key RENAME TO boards_slug_key;

-- tickets
ALTER TABLE tickets RENAME COLUMN project_id TO board_id;
ALTER TABLE tickets RENAME CONSTRAINT tickets_project_id_fkey TO tickets_board_id_fkey;
ALTER INDEX tickets_project_id_idx RENAME TO tickets_board_id_idx;
ALTER INDEX tickets_project_archived_at_idx RENAME TO tickets_board_archived_at_idx;

-- chat_sessions
ALTER TABLE chat_sessions RENAME COLUMN project_id TO board_id;
ALTER TABLE chat_sessions RENAME CONSTRAINT chat_sessions_project_id_fkey TO chat_sessions_board_id_fkey;
ALTER INDEX chat_sessions_project_updated_idx RENAME TO chat_sessions_board_updated_idx;

-- knowledge_revisions: the immutability trigger rejects any row UPDATE, so it
-- has to come off before the scope backfill and go back on afterwards.
DROP TRIGGER knowledge_revisions_immutable ON knowledge_revisions;

DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    FOR constraint_name IN
        SELECT conname
        FROM pg_constraint
        WHERE conrelid = 'knowledge_revisions'::regclass
          AND contype = 'c'
          AND (
              pg_get_constraintdef(oid) LIKE '%project_id%'
              OR pg_get_constraintdef(oid) LIKE '%''project''%'
          )
    LOOP
        EXECUTE format(
            'ALTER TABLE knowledge_revisions DROP CONSTRAINT %I',
            constraint_name
        );
    END LOOP;
END $$;

UPDATE knowledge_revisions SET scope = 'board' WHERE scope = 'project';

ALTER TABLE knowledge_revisions RENAME COLUMN project_id TO board_id;
ALTER TABLE knowledge_revisions
    RENAME CONSTRAINT knowledge_revisions_project_id_fkey TO knowledge_revisions_board_id_fkey;
ALTER INDEX knowledge_revisions_project_scope_idx RENAME TO knowledge_revisions_board_scope_idx;

ALTER TABLE knowledge_revisions
    ADD CONSTRAINT knowledge_revisions_scope_check
        CHECK (scope IN ('workspace', 'board', 'agent')),
    ADD CONSTRAINT knowledge_revisions_scope_ids_check
        CHECK (
            (scope = 'workspace' AND board_id IS NULL AND agent_id IS NULL)
            OR (scope = 'board' AND board_id IS NOT NULL AND agent_id IS NULL)
            OR (scope = 'agent' AND board_id IS NOT NULL AND agent_id IS NOT NULL)
        );

CREATE TRIGGER knowledge_revisions_immutable
BEFORE UPDATE ON knowledge_revisions
FOR EACH ROW EXECUTE FUNCTION prevent_knowledge_revision_update();
