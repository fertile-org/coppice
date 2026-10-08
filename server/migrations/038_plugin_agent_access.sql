-- One assignment per plugin, shared by the plugin card and the agent form.
-- 'all' includes agents created later. 'explicit' is the agent_plugins list.
-- Existing per-agent rows stay an explicit list. Nothing is converted to 'all'.

ALTER TABLE plugins
    ADD COLUMN agent_access TEXT NOT NULL DEFAULT 'explicit'
    CHECK (agent_access IN ('all', 'explicit'));
