-- M09 chat actions: ticket provenance, knowledge source, session lineage

ALTER TABLE tickets
    ADD COLUMN IF NOT EXISTS source_chat_session_id UUID
        REFERENCES chat_sessions(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS tickets_source_chat_session_id_idx
    ON tickets (source_chat_session_id)
    WHERE source_chat_session_id IS NOT NULL;

ALTER TABLE chat_sessions
    ADD COLUMN IF NOT EXISTS parent_session_id UUID
        REFERENCES chat_sessions(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS chat_sessions_parent_session_id_idx
    ON chat_sessions (parent_session_id)
    WHERE parent_session_id IS NOT NULL;

ALTER TABLE knowledge_revisions
    DROP CONSTRAINT IF EXISTS knowledge_revisions_source_type_check;

ALTER TABLE knowledge_revisions
    ADD CONSTRAINT knowledge_revisions_source_type_check CHECK (
        source_type IN (
            'ticket', 'comment', 'review', 'human_note', 'agent_summary',
            'workspace_signal', 'observation_run', 'chat_session'
        )
    );
