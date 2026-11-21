-- Format check + disabled install + auto-test fields for plugin builds (PB-7/8).

ALTER TABLE plugin_builds
    ADD COLUMN check_errors JSONB,
    ADD COLUMN format_retries INT NOT NULL DEFAULT 0,
    ADD COLUMN plugin_id UUID REFERENCES plugins(id) ON DELETE SET NULL,
    ADD COLUMN test_result JSONB;
