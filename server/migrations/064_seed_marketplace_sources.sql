-- Ship three more git catalogs once. Deleting a seeded source is permanent
-- (this migration does not re-run). Skip a row when the name is taken or when
-- any source already uses that git_url (a user may have added it under another
-- name). Positions append after whatever is already there.
WITH base AS (
    SELECT COALESCE(MAX(position), 0) AS max_pos FROM marketplace_sources
)
INSERT INTO marketplace_sources (id, name, kind, git_url, git_ref, position)
SELECT gen_random_uuid(), v.name, 'git', v.git_url, 'main', base.max_pos + v.seq
FROM base,
    (VALUES
        (1, 'claude-community', 'https://github.com/anthropics/claude-plugins-community.git'),
        (2, 'superpowers-marketplace', 'https://github.com/obra/superpowers-marketplace.git'),
        (3, 'ponytail', 'https://github.com/DietrichGebert/ponytail.git')
    ) AS v(seq, name, git_url)
WHERE NOT EXISTS (
    SELECT 1 FROM marketplace_sources s
    WHERE s.name = v.name OR s.git_url = v.git_url
)
ON CONFLICT (name) DO NOTHING;
