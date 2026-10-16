-- Description attachments reuse the comment attachment store. No backfill:
-- existing tickets have none.
ALTER TABLE tickets
    ADD COLUMN attachment_ids UUID[] NOT NULL DEFAULT '{}';
