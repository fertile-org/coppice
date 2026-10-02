CREATE TABLE plugin_settings (
    plugin_id UUID NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    secret_id UUID NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (plugin_id, key)
);
