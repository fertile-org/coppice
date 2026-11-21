-- Entity ids: UUID → VARCHAR(36), UUID[] → VARCHAR(36)[].
-- Discover columns from information_schema; recreate FKs with the same ON DELETE.
-- Drop/recreate views that depend on those columns. PK columns get a CHECK for
-- UUID text or 12-char [0-9a-z].

DO $$
DECLARE
  r record;
  fk record;
  pk record;
  v record;
  check_name text;
  check_sql text;
BEGIN
  CREATE TEMP TABLE _id_refactor_views ON COMMIT DROP AS
  SELECT
    c.oid,
    c.relname AS view_name,
    pg_get_viewdef(c.oid, true) AS def
  FROM pg_class c
  JOIN pg_namespace nsp ON nsp.oid = c.relnamespace
  WHERE nsp.nspname = 'public'
    AND c.relkind = 'v'
    AND EXISTS (
      SELECT 1
      FROM pg_depend d
      JOIN pg_rewrite rw ON rw.oid = d.objid
      JOIN pg_attribute a
        ON a.attrelid = d.refobjid
       AND a.attnum = d.refobjsubid
       AND NOT a.attisdropped
      JOIN pg_type t ON t.oid = a.atttypid
      WHERE rw.ev_class = c.oid
        AND d.refclassid = 'pg_class'::regclass
        AND d.classid = 'pg_rewrite'::regclass
        AND t.typname IN ('uuid', '_uuid')
    );

  FOR v IN SELECT * FROM _id_refactor_views ORDER BY view_name LOOP
    EXECUTE format('DROP VIEW %I', v.view_name);
  END LOOP;

  -- CHECKs like `supersedes_item_id <> id` fail mid-ALTER (uuid <> varchar).
  CREATE TEMP TABLE _id_refactor_checks ON COMMIT DROP AS
  SELECT
    c.conname,
    rel.relname AS table_name,
    pg_get_constraintdef(c.oid) AS def
  FROM pg_constraint c
  JOIN pg_class rel ON rel.oid = c.conrelid
  JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
  WHERE nsp.nspname = 'public'
    AND c.contype = 'c'
    AND EXISTS (
      SELECT 1
      FROM unnest(c.conkey) AS colnum(attnum)
      JOIN pg_attribute a
        ON a.attrelid = c.conrelid
       AND a.attnum = colnum.attnum
       AND NOT a.attisdropped
      JOIN pg_type t ON t.oid = a.atttypid
      WHERE t.typname IN ('uuid', '_uuid')
    );

  FOR r IN SELECT * FROM _id_refactor_checks ORDER BY conname LOOP
    EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I', r.table_name, r.conname);
  END LOOP;

  CREATE TEMP TABLE _id_refactor_fks ON COMMIT DROP AS
  SELECT
    c.conname,
    rel.relname AS table_name,
    pg_get_constraintdef(c.oid) AS def
  FROM pg_constraint c
  JOIN pg_class rel ON rel.oid = c.conrelid
  JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
  WHERE nsp.nspname = 'public'
    AND c.contype = 'f'
    AND (
      EXISTS (
        SELECT 1
        FROM unnest(c.conkey) AS colnum(attnum)
        JOIN pg_attribute a
          ON a.attrelid = c.conrelid
         AND a.attnum = colnum.attnum
         AND NOT a.attisdropped
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE t.typname IN ('uuid', '_uuid')
      )
      OR EXISTS (
        SELECT 1
        FROM unnest(c.confkey) AS colnum(attnum)
        JOIN pg_attribute a
          ON a.attrelid = c.confrelid
         AND a.attnum = colnum.attnum
         AND NOT a.attisdropped
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE t.typname IN ('uuid', '_uuid')
      )
    );

  FOR fk IN SELECT * FROM _id_refactor_fks ORDER BY conname LOOP
    EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I', fk.table_name, fk.conname);
  END LOOP;

  FOR r IN
    SELECT c.table_name, c.column_name, c.udt_name
    FROM information_schema.columns c
    WHERE c.table_schema = 'public'
      AND (c.data_type = 'uuid' OR c.udt_name = '_uuid')
    ORDER BY c.table_name, c.ordinal_position
  LOOP
    IF r.udt_name = '_uuid' THEN
      EXECUTE format(
        'ALTER TABLE %I ALTER COLUMN %I TYPE VARCHAR(36)[] USING %I::text[]',
        r.table_name, r.column_name, r.column_name
      );
    ELSE
      EXECUTE format(
        'ALTER TABLE %I ALTER COLUMN %I TYPE VARCHAR(36) USING %I::text',
        r.table_name, r.column_name, r.column_name
      );
    END IF;
  END LOOP;

  FOR fk IN SELECT * FROM _id_refactor_fks ORDER BY conname LOOP
    EXECUTE format(
      'ALTER TABLE %I ADD CONSTRAINT %I %s',
      fk.table_name, fk.conname, fk.def
    );
  END LOOP;

  FOR r IN SELECT * FROM _id_refactor_checks ORDER BY conname LOOP
    EXECUTE format(
      'ALTER TABLE %I ADD CONSTRAINT %I %s',
      r.table_name, r.conname, r.def
    );
  END LOOP;

  FOR v IN SELECT * FROM _id_refactor_views ORDER BY view_name LOOP
    EXECUTE format('CREATE VIEW %I AS %s', v.view_name, v.def);
  END LOOP;

  FOR pk IN
    SELECT
      tc.table_name,
      kcu.column_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
      ON kcu.constraint_name = tc.constraint_name
     AND kcu.table_schema = tc.table_schema
     AND kcu.table_name = tc.table_name
    JOIN information_schema.columns col
      ON col.table_schema = tc.table_schema
     AND col.table_name = tc.table_name
     AND col.column_name = kcu.column_name
    WHERE tc.table_schema = 'public'
      AND tc.constraint_type = 'PRIMARY KEY'
      AND col.data_type IN ('character varying', 'text')
      AND col.character_maximum_length = 36
    ORDER BY tc.table_name, kcu.ordinal_position
  LOOP
    check_name := format('%s_%s_id_format', pk.table_name, pk.column_name);
    IF length(check_name) > 63 THEN
      check_name := left(check_name, 63);
    END IF;
    check_sql := format(
      $c$%I ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' OR %I ~ '^[0-9a-z]{12}$'$c$,
      pk.column_name, pk.column_name
    );
    EXECUTE format(
      'ALTER TABLE %I ADD CONSTRAINT %I CHECK (%s)',
      pk.table_name, check_name, check_sql
    );
  END LOOP;
END $$;
