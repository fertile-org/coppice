CREATE TABLE plugin_dirs (
    id UUID PRIMARY KEY, path TEXT NOT NULL UNIQUE, position INTEGER NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT false, created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE plugins (
    id UUID PRIMARY KEY,
    plugin_dir_id UUID NOT NULL REFERENCES plugin_dirs(id) ON DELETE CASCADE,
    rel_path TEXT NOT NULL, name TEXT NOT NULL, version TEXT NOT NULL DEFAULT '0.0.0',
    description TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'local' CHECK (source IN ('local', 'git')),
    git_url TEXT, git_ref TEXT, git_commit TEXT,
    manifest JSONB NOT NULL DEFAULT '{}',
    status TEXT NOT NULL CHECK (status IN ('ok', 'invalid', 'missing', 'shadowed')),
    error TEXT, enabled BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (plugin_dir_id, rel_path)
);
CREATE TABLE agent_plugins (
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    PRIMARY KEY (agent_id, plugin_id)
);
CREATE TABLE plugin_installs (
    id UUID PRIMARY KEY,
    plugin_dir_id UUID NOT NULL REFERENCES plugin_dirs(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('install', 'update')),
    git_url TEXT NOT NULL, git_ref TEXT,
    plugin_id UUID REFERENCES plugins(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
    error TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), finished_at TIMESTAMPTZ
);
ALTER TABLE run_tool_tokens ADD COLUMN plugin_ids UUID[] NOT NULL DEFAULT '{}';
ALTER TABLE agent_presets ADD COLUMN default_plugins TEXT[] NOT NULL DEFAULT '{}';
