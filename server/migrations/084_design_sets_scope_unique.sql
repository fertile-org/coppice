-- Allow a published chat design and its ticket copy to share the same run+kind.
-- Uniqueness stays one published set per (run, kind) within each scope side.
DROP INDEX IF EXISTS design_sets_one_published_per_run_kind;
CREATE UNIQUE INDEX design_sets_one_published_per_run_kind_scope
    ON design_sets (run_id, kind, (COALESCE(ticket_id, chat_session_id)))
    WHERE published_at IS NOT NULL;
