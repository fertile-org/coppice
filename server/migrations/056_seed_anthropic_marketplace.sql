-- Ship one pinned external catalog URL while preserving user ownership. This
-- runs only once in the migration history, so deleting the source is permanent.
-- Position appends after existing sources on upgrade; fresh installs seed this
-- row before the built-in Coppice source is created at position 0.
INSERT INTO marketplace_sources (id, name, kind, git_url, git_ref, position)
SELECT
    gen_random_uuid(),
    'claude-plugins-official',
    'git',
    'https://github.com/anthropics/claude-plugins-official.git',
    'main',
    COALESCE(MAX(position), 0) + 1
FROM marketplace_sources
ON CONFLICT (name) DO NOTHING;
