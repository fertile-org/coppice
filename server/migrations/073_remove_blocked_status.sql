-- Blocked is no longer a ticket column. Remap every blocked row — including
-- archived — to the open restart decision's previous column when it is a real
-- status, otherwise Backlog. Keep substatus as-is.
UPDATE tickets AS t
SET status = COALESCE(
    (
        SELECT d.previous_status
        FROM run_restart_decisions AS d
        WHERE d.ticket_id = t.id
          AND d.resolved_at IS NULL
          AND d.previous_status IS DISTINCT FROM 'blocked'
        LIMIT 1
    ),
    'backlog'
)
WHERE t.status = 'blocked';

-- Open restart rows that stored Blocked as previous_status fall back to Backlog.
UPDATE run_restart_decisions
SET previous_status = 'backlog'
WHERE resolved_at IS NULL
  AND previous_status = 'blocked';
