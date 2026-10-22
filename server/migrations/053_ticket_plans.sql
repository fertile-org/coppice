-- Plans and Ask-for-changes live on ticket_plans, not ticket_comments.
-- Legacy plan comments stay in place; only new plans skip the thread.

CREATE TABLE ticket_plans (
    id UUID PRIMARY KEY,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    run_id UUID REFERENCES agent_runs(id) ON DELETE SET NULL,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    revision INTEGER NOT NULL,
    body TEXT NOT NULL,
    content_version INTEGER NOT NULL,
    change_request TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    source_comment_id UUID REFERENCES ticket_comments(id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX ticket_plans_ticket_revision_idx
    ON ticket_plans (ticket_id, revision);

ALTER TABLE tickets
    ADD COLUMN approved_plan_id UUID;

ALTER TABLE ticket_plan_splits
    ADD COLUMN plan_id UUID;

INSERT INTO ticket_plans (
    id, ticket_id, run_id, agent_id, revision, body, content_version, created_at, source_comment_id
)
SELECT
    gen_random_uuid(),
    comment.ticket_id,
    NULL,
    CASE WHEN comment.author_type = 'agent' THEN comment.author_id ELSE NULL END,
    ROW_NUMBER() OVER (
        PARTITION BY comment.ticket_id
        ORDER BY comment.created_at ASC, comment.id ASC
    )::integer,
    comment.body,
    COALESCE(comment.plan_content_version, ticket.content_version),
    comment.created_at,
    comment.id
FROM ticket_comments comment
JOIN tickets ticket ON ticket.id = comment.ticket_id
WHERE comment.intent = 'plan';

-- Each plan_ticket run triggered by review_feedback attaches that body to the
-- latest revision created before the trigger. Two triggers on one revision append.
WITH triggers AS (
    SELECT
        run.ticket_id,
        comment.body,
        comment.created_at AS trigger_at,
        comment.id AS trigger_id,
        (
            SELECT plan.id
            FROM ticket_plans plan
            WHERE plan.ticket_id = run.ticket_id
              AND plan.created_at < comment.created_at
            ORDER BY plan.created_at DESC, plan.revision DESC
            LIMIT 1
        ) AS plan_id
    FROM agent_runs run
    JOIN ticket_comments comment ON comment.id = run.trigger_comment_id
    WHERE run.job_type = 'plan_ticket'
      AND comment.intent = 'review_feedback'
),
combined AS (
    SELECT
        plan_id,
        string_agg(body, E'\n\n' ORDER BY trigger_at, trigger_id) AS requests
    FROM triggers
    WHERE plan_id IS NOT NULL
    GROUP BY plan_id
)
UPDATE ticket_plans plan
SET change_request = combined.requests
FROM combined
WHERE plan.id = combined.plan_id;

UPDATE tickets ticket
SET approved_plan_id = plan.id
FROM ticket_plans plan
WHERE plan.source_comment_id IS NOT NULL
  AND plan.source_comment_id = ticket.approved_plan_comment_id;

UPDATE ticket_plan_splits split
SET plan_id = plan.id
FROM ticket_plans plan
WHERE plan.source_comment_id IS NOT NULL
  AND plan.source_comment_id = split.plan_comment_id;

ALTER TABLE tickets
    DROP CONSTRAINT tickets_approved_plan_comment_id_fkey;

ALTER TABLE tickets
    DROP COLUMN approved_plan_comment_id;

ALTER TABLE tickets
    ADD CONSTRAINT tickets_approved_plan_id_fkey
    FOREIGN KEY (approved_plan_id)
    REFERENCES ticket_plans (id)
    ON DELETE SET NULL;

ALTER TABLE ticket_plan_splits
    DROP COLUMN plan_comment_id;

ALTER TABLE ticket_plan_splits
    ADD CONSTRAINT ticket_plan_splits_plan_id_fkey
    FOREIGN KEY (plan_id)
    REFERENCES ticket_plans (id)
    ON DELETE SET NULL;

ALTER TABLE ticket_plans
    DROP COLUMN source_comment_id;
