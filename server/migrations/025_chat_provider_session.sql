ALTER TABLE chat_sessions
    ADD COLUMN IF NOT EXISTS provider_session_id TEXT NULL,
    ADD COLUMN IF NOT EXISTS provider_session_connector TEXT NULL;
