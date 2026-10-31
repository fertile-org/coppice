-- Knowledge search: match word forms (english stems) and parts of dotted names
-- and paths ("config.toml" also indexes "config" and "toml").

-- The text as written, then again with punctuation turned into spaces. Schema
-- qualified because backup restore recomputes the generated column with an
-- empty search_path.
CREATE FUNCTION coppice_search_text(value TEXT)
RETURNS TEXT
LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT
AS $$
    SELECT public.coppice_unaccent(value) || ' '
        || regexp_replace(public.coppice_unaccent(value), '[^[:alnum:]]+', ' ', 'g')
$$;

-- ALTER rewrites rows without firing the revision immutability UPDATE trigger.
-- Dropping the column drops its GIN index too.
ALTER TABLE knowledge_revisions DROP COLUMN search_vector;
ALTER TABLE knowledge_revisions
    ADD COLUMN search_vector tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector('simple'::regconfig, coppice_search_text(title)), 'A')
        || setweight(to_tsvector('english'::regconfig, coppice_search_text(title)), 'A')
        || setweight(to_tsvector('simple'::regconfig, coppice_search_text(content)), 'B')
        || setweight(to_tsvector('english'::regconfig, coppice_search_text(content)), 'B')
    ) STORED;
CREATE INDEX knowledge_revisions_search_idx
    ON knowledge_revisions USING gin (search_vector);
