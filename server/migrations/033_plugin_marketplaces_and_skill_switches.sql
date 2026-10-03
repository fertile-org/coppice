ALTER TABLE plugins DROP CONSTRAINT plugins_status_check;
ALTER TABLE plugins ADD CONSTRAINT plugins_status_check
    CHECK (status IN ('ok', 'invalid', 'missing', 'shadowed', 'external'));

ALTER TABLE plugins
    ADD COLUMN git_root TEXT,
    ADD COLUMN disabled_skills TEXT[] NOT NULL DEFAULT '{}';
UPDATE plugins SET git_root = rel_path WHERE source = 'git';

ALTER TABLE plugin_installs ADD COLUMN plugin_ids UUID[] NOT NULL DEFAULT '{}';
UPDATE plugin_installs SET plugin_ids = ARRAY[plugin_id] WHERE plugin_id IS NOT NULL;
