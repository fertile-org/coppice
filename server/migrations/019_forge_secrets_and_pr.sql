-- Encrypted secrets (forge tokens, etc.) + ticket PR URL.
CREATE TABLE secrets (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    ciphertext BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX secrets_name_idx ON secrets (name);

ALTER TABLE repos
    ADD COLUMN forge_token_secret_id UUID REFERENCES secrets(id) ON DELETE SET NULL;

ALTER TABLE tickets
    ADD COLUMN pr_url TEXT;
