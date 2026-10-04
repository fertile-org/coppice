-- pgvector is gone (001/013 rewritten, 027 dropped the vector columns), but
-- databases migrated on the pgvector image still have the extension, which
-- breaks pg_dump/restore into a plain Postgres. No-op on fresh databases.
DROP EXTENSION IF EXISTS vector;
