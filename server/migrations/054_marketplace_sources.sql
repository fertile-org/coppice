-- Marketplace sources: the built-in Coppice catalog (seeded by the service at
-- startup) and user-added git marketplaces. Plugins installed from an entry
-- record where they came from; removing a source keeps the plugin installed.

CREATE TABLE marketplace_sources (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('builtin', 'git')),
    git_url TEXT,
    git_ref TEXT,
    commit_sha TEXT,
    refreshed_at TIMESTAMPTZ,
    last_error TEXT,
    position INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((kind = 'builtin' AND git_url IS NULL) OR (kind = 'git' AND git_url IS NOT NULL))
);

ALTER TABLE plugins
    ADD COLUMN origin_source_id UUID REFERENCES marketplace_sources(id) ON DELETE SET NULL,
    ADD COLUMN origin_entry TEXT,
    ADD COLUMN origin_commit TEXT;
