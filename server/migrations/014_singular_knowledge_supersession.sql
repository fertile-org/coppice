-- A knowledge item may have only one replacement that can still become active.
-- Rejected and stale candidates remain immutable history and do not block a successor.

-- Migration 013 did not enforce singular replacements. Choose one live winner per
-- original before adding the index. An existing live superseded_by target wins;
-- a link to any other historical target retires every competing live child.
CREATE TEMP TABLE knowledge_supersession_repair_winners
ON COMMIT DROP AS
SELECT
    original.id AS original_id,
    CASE
        WHEN original.superseded_by IS NOT NULL THEN linked.id
        ELSE ranked.id
    END AS winner_id
FROM knowledge_items original
LEFT JOIN knowledge_items linked
    ON linked.id = original.superseded_by
   AND linked.supersedes_item_id = original.id
   AND linked.status IN ('pending', 'approved')
LEFT JOIN LATERAL (
    SELECT candidate.id
    FROM knowledge_items candidate
    WHERE candidate.supersedes_item_id = original.id
      AND candidate.status IN ('pending', 'approved')
    ORDER BY
        (candidate.status = 'approved' AND candidate.active_revision_id IS NOT NULL) DESC,
        (candidate.status = 'approved') DESC,
        candidate.created_at ASC,
        candidate.id ASC
    LIMIT 1
) ranked ON original.superseded_by IS NULL
WHERE EXISTS (
    SELECT 1
    FROM knowledge_items candidate
    WHERE candidate.supersedes_item_id = original.id
      AND candidate.status IN ('pending', 'approved')
);

UPDATE knowledge_items candidate
SET status = 'rejected',
    active_revision_id = NULL,
    rejection_reason = 'migration 014: competing live supersession candidate quarantined',
    version = candidate.version + 1,
    updated_at = now()
FROM knowledge_supersession_repair_winners repair
WHERE candidate.supersedes_item_id = repair.original_id
  AND candidate.status IN ('pending', 'approved')
  AND candidate.id IS DISTINCT FROM repair.winner_id;

-- Repair an original that should already have been retired by an active winner.
UPDATE knowledge_items original
SET superseded_by = repair.winner_id,
    version = original.version + 1,
    updated_at = now()
FROM knowledge_supersession_repair_winners repair
JOIN knowledge_items winner ON winner.id = repair.winner_id
WHERE original.id = repair.original_id
  AND original.superseded_by IS NULL
  AND winner.status = 'approved'
  AND winner.active_revision_id IS NOT NULL;

DROP TABLE knowledge_supersession_repair_winners;

CREATE UNIQUE INDEX knowledge_items_one_live_replacement_idx
    ON knowledge_items (supersedes_item_id)
    WHERE supersedes_item_id IS NOT NULL
      AND status IN ('pending', 'approved');
