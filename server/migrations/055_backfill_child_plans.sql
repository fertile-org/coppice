-- Run splits created sub-tickets in Backlog with no plan, so they could never
-- move to Ready. Give each stuck child an approved plan built from its
-- description. Other tickets are left alone: top-level tickets, children
-- that already have a plan, children with Skip planning on, and archived or
-- already-moved children.

WITH stuck AS (
    SELECT ticket.id, ticket.description, ticket.content_version
    FROM tickets ticket
    WHERE ticket.archived_at IS NULL
      AND ticket.parent_ticket_id IS NOT NULL
      AND ticket.approved_plan_id IS NULL
      AND ticket.skip_planning = false
      AND ticket.status = 'backlog'
      AND NOT EXISTS (
          SELECT 1 FROM ticket_plans plan WHERE plan.ticket_id = ticket.id
      )
),
new_plans AS (
    INSERT INTO ticket_plans (
        id, ticket_id, run_id, agent_id, revision, body, content_version
    )
    SELECT gen_random_uuid(), stuck.id, NULL, NULL, 1, stuck.description, stuck.content_version
    FROM stuck
    RETURNING id, ticket_id, content_version
)
UPDATE tickets ticket
SET approved_plan_id = new_plans.id,
    approved_plan_content_version = new_plans.content_version
FROM new_plans
WHERE ticket.id = new_plans.ticket_id;
