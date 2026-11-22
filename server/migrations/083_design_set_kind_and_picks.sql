-- Design sets can share a run when their kinds differ (ui vs architecture…).
-- ticket_plans.design_picks records every set → option the plan was written around.
-- Idempotent: some databases already applied this SQL under a colliding version 077.
ALTER TABLE design_sets
    ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'ui';

DO $$
BEGIN
    ALTER TABLE design_sets
        ADD CONSTRAINT design_sets_kind_check
        CHECK (kind IN ('ui', 'architecture', 'module', 'refactor', 'dependency'));
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

DROP INDEX IF EXISTS design_sets_one_published_per_run;
CREATE UNIQUE INDEX IF NOT EXISTS design_sets_one_published_per_run_kind
    ON design_sets (run_id, kind) WHERE published_at IS NOT NULL;

ALTER TABLE ticket_plans
    ADD COLUMN IF NOT EXISTS design_picks JSONB NOT NULL DEFAULT '{}'::jsonb;
