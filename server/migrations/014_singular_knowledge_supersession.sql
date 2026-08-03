-- A knowledge item may have only one replacement that can still become active.
-- Rejected and stale candidates remain immutable history and do not block a successor.

CREATE UNIQUE INDEX knowledge_items_one_live_replacement_idx
    ON knowledge_items (supersedes_item_id)
    WHERE supersedes_item_id IS NOT NULL
      AND status IN ('pending', 'approved');
