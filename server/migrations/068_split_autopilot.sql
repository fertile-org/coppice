-- Sub-ticket order and the parent's auto-start switch. Auto-start is on
-- so approving a plan starts the first child, and a recorded merge starts
-- the next one. Pause sets the flag off.
ALTER TABLE ticket_plan_splits
    ADD COLUMN auto_start BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN child_order UUID[] NOT NULL DEFAULT '{}';

UPDATE ticket_plan_splits split
SET child_order = ordered.ids
FROM (
    SELECT parent_ticket_id, array_agg(id ORDER BY created_at, id) AS ids
    FROM tickets
    WHERE parent_ticket_id IS NOT NULL
    GROUP BY parent_ticket_id
) ordered
WHERE ordered.parent_ticket_id = split.parent_ticket_id
  AND split.child_order = '{}';
