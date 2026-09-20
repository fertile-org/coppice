-- Chat message attachments (upload-then-link, same pattern as ticket_comments)

ALTER TABLE chat_messages
    ADD COLUMN IF NOT EXISTS attachment_ids UUID[] NOT NULL DEFAULT '{}';
