-- Legacy safety: tickets.branch_name was free-form and unused for git.
-- Tickets that ever had a worktree/branch on a run get the legacy name so
-- resolve_stored_branch points at the branch that already exists on disk.
-- Tickets that never ran lose any free-form value (stay NULL until first run).

UPDATE tickets AS t
SET branch_name = 'agent/TICKET-' || split_part(t.id::text, '-', 1)
WHERE EXISTS (
    SELECT 1
    FROM agent_runs AS r
    WHERE r.ticket_id = t.id
      AND (r.worktree_path IS NOT NULL OR r.branch_name IS NOT NULL)
);

UPDATE tickets AS t
SET branch_name = NULL
WHERE t.branch_name IS NOT NULL
  AND NOT EXISTS (
    SELECT 1
    FROM agent_runs AS r
    WHERE r.ticket_id = t.id
      AND (r.worktree_path IS NOT NULL OR r.branch_name IS NOT NULL)
);
