-- Plugin builder: durable build records + repo-less workspaces under plugin-builds/.

CREATE TABLE plugin_builds (
    id UUID PRIMARY KEY,
    answers JSONB NOT NULL DEFAULT '{}'::jsonb,
    plans JSONB NOT NULL DEFAULT '[]'::jsonb,
    status TEXT NOT NULL
        CHECK (status IN (
            'drafting',
            'planning',
            'plan_ready',
            'building',
            'ready_to_try',
            'accepted',
            'discarded',
            'cancelled'
        )),
    plugin_name TEXT,
    workspace_path TEXT NOT NULL,
    publish_target TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX plugin_builds_created_at_idx ON plugin_builds (created_at DESC);
