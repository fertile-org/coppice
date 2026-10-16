//! Non-superuser role for the desktop server and backup import.
//!
//! `initdb -U coppice` makes `coppice` a superuser. Backup import feeds
//! `database.sql` to `psql` as the runtime role, and a superuser can run
//! `COPY ... TO PROGRAM`. The server and that import connect as `coppice_app`
//! instead. The superuser password is only used here, to create the role and
//! to adopt a database left by an older install. Adoption opens that database
//! once; `pg_app_role_ready` in the data directory records it, and later
//! launches do not open the database as the superuser.

use std::path::Path;

use anyhow::{bail, Context};
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Connection, PgConnection};

use super::postgres::{percent_encode, SUPERUSER};

pub(crate) const APP_ROLE: &str = "coppice_app";

/// Written in the desktop data directory after ownership has moved.
pub(crate) const READY_FLAG_FILE: &str = "pg_app_role_ready";

const ROLE_ATTRIBUTES: &str = "LOGIN NOSUPERUSER NOCREATEROLE NOCREATEDB NOREPLICATION NOBYPASSRLS";

const SERVER_PROGRAM_ROLES: [&str; 3] = [
    "pg_execute_server_program",
    "pg_read_server_files",
    "pg_write_server_files",
];

pub(crate) fn app_database_url(port: u16, password: &str, name: &str) -> anyhow::Result<String> {
    if !super::postgres::is_valid_db_name(name) {
        bail!("invalid database name {name:?}");
    }
    Ok(format!(
        "postgres://{APP_ROLE}:{}@127.0.0.1:{port}/{name}",
        percent_encode(password)
    ))
}

/// Creates `coppice_app` when missing, makes it own `database`, and moves
/// existing schema objects off the bootstrap superuser. An existing role keeps
/// its password so a crashed replacement can still be detected from the files.
///
/// Role and database checks run on the `postgres` database. Object ownership
/// is moved only when `ready_flag` is absent (or the database was just
/// created). That flag is written after the move succeeds.
pub(crate) async fn provision_app_role(
    host: &str,
    port: u16,
    superuser_password: &str,
    app_password: &str,
    database: &str,
    ready_flag: &Path,
) -> anyhow::Result<()> {
    if !super::postgres::is_valid_db_name(database) {
        bail!("invalid database name {database:?}");
    }
    let mut maintenance = connect(host, port, superuser_password, "postgres").await?;
    let role_existed = {
        let mut tx = catalog_tx(&mut maintenance).await?;
        let role_existed = role_exists(&mut tx).await?;
        if role_existed {
            lock_down_role(&mut tx).await?;
        }
        tx.commit().await.context("commit")?;
        role_existed
    };
    if !role_existed {
        if app_password.is_empty() {
            bail!("application role password is empty");
        }
        let password_literal =
            crate::services::secret_rotation::role_password_literal(app_password)
                .map_err(|err| anyhow::anyhow!("{err}"))?;
        exec(
            &mut maintenance,
            &format!(
                "CREATE ROLE {} WITH {ROLE_ATTRIBUTES} PASSWORD {password_literal}",
                quote_ident(APP_ROLE)
            ),
        )
        .await
        .context("create application role")?;
        let mut tx = catalog_tx(&mut maintenance).await?;
        lock_down_role(&mut tx).await?;
        tx.commit().await.context("commit")?;
    }

    let database_existed = {
        let mut tx = catalog_tx(&mut maintenance).await?;
        let database_existed = database_exists(&mut tx, database).await?;
        tx.commit().await.context("commit")?;
        database_existed
    };
    let database_ident = quote_ident(database);
    if database_existed {
        exec(
            &mut maintenance,
            &format!(
                "ALTER DATABASE {database_ident} OWNER TO {}",
                quote_ident(APP_ROLE)
            ),
        )
        .await
        .context("transfer database ownership")?;
    } else {
        exec(
            &mut maintenance,
            &format!(
                "CREATE DATABASE {database_ident} OWNER {}",
                quote_ident(APP_ROLE)
            ),
        )
        .await
        .context("create application database")?;
    }
    maintenance.close().await.ok();

    // A database GUC cannot redirect a connection we never open.
    if ready_flag.is_file() && database_existed {
        return Ok(());
    }

    let mut conn = connect(host, port, superuser_password, database).await?;
    let mut tx = catalog_tx(&mut conn).await?;
    transfer_ownership(&mut tx).await?;
    tx.commit().await.context("commit")?;
    conn.close().await.ok();
    write_ready_flag(ready_flag)?;
    Ok(())
}

async fn role_exists(conn: &mut PgConnection) -> anyhow::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_catalog.pg_roles
            WHERE rolname::pg_catalog.text OPERATOR(pg_catalog.=) $1
        )",
    )
    .bind(APP_ROLE)
    .fetch_one(&mut *conn)
    .await
    .context("check application role")
}

async fn database_exists(conn: &mut PgConnection, database: &str) -> anyhow::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_catalog.pg_database
            WHERE datname::pg_catalog.text OPERATOR(pg_catalog.=) $1
        )",
    )
    .bind(database)
    .fetch_one(&mut *conn)
    .await
    .context("check application database")
}

async fn lock_down_role(conn: &mut PgConnection) -> anyhow::Result<()> {
    exec(
        conn,
        &format!(
            "ALTER ROLE {} WITH {ROLE_ATTRIBUTES}",
            quote_ident(APP_ROLE)
        ),
    )
    .await
    .context("restrict application role")?;
    revoke_server_file_roles(conn).await?;
    Ok(())
}

async fn revoke_server_file_roles(conn: &mut PgConnection) -> anyhow::Result<()> {
    for granted in SERVER_PROGRAM_ROLES {
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM pg_catalog.pg_auth_members m
                JOIN pg_catalog.pg_roles g
                  ON g.oid OPERATOR(pg_catalog.=) m.roleid
                JOIN pg_catalog.pg_roles u
                  ON u.oid OPERATOR(pg_catalog.=) m.member
                WHERE u.rolname::pg_catalog.text OPERATOR(pg_catalog.=) $1
                  AND g.rolname::pg_catalog.text OPERATOR(pg_catalog.=) $2
            )",
        )
        .bind(APP_ROLE)
        .bind(granted)
        .fetch_one(&mut *conn)
        .await
        .with_context(|| format!("check membership of {granted}"))?;
        if member {
            exec(
                conn,
                &format!(
                    "REVOKE {} FROM {}",
                    quote_ident(granted),
                    quote_ident(APP_ROLE)
                ),
            )
            .await
            .with_context(|| format!("revoke {granted}"))?;
        }
    }
    Ok(())
}

async fn transfer_ownership(conn: &mut PgConnection) -> anyhow::Result<()> {
    let schemas: Vec<String> = sqlx::query_scalar(
        "SELECT n.nspname
         FROM pg_catalog.pg_namespace n
         JOIN pg_catalog.pg_roles r
           ON r.oid OPERATOR(pg_catalog.=) n.nspowner
         WHERE n.nspname::pg_catalog.text OPERATOR(pg_catalog.<>) 'pg_catalog'
           AND n.nspname::pg_catalog.text OPERATOR(pg_catalog.<>) 'information_schema'
           AND n.nspname::pg_catalog.text OPERATOR(pg_catalog.!~~) 'pg_toast%'
           AND n.nspname::pg_catalog.text OPERATOR(pg_catalog.!~~) 'pg_temp%'
           AND r.rolname::pg_catalog.text OPERATOR(pg_catalog.<>) $1",
    )
    .bind(APP_ROLE)
    .fetch_all(&mut *conn)
    .await
    .context("list schemas")?;
    for schema in schemas {
        exec(
            conn,
            &format!(
                "ALTER SCHEMA {} OWNER TO {}",
                quote_ident(&schema),
                quote_ident(APP_ROLE)
            ),
        )
        .await
        .with_context(|| format!("transfer schema {schema}"))?;
    }

    let relations: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT n.nspname, c.relname, c.relkind::pg_catalog.text
         FROM pg_catalog.pg_class c
         JOIN pg_catalog.pg_namespace n
           ON n.oid OPERATOR(pg_catalog.=) c.relnamespace
         JOIN pg_catalog.pg_roles r
           ON r.oid OPERATOR(pg_catalog.=) c.relowner
         WHERE n.nspname::pg_catalog.text OPERATOR(pg_catalog.<>) 'pg_catalog'
           AND n.nspname::pg_catalog.text OPERATOR(pg_catalog.<>) 'information_schema'
           AND n.nspname::pg_catalog.text OPERATOR(pg_catalog.!~~) 'pg_toast%'
           AND n.nspname::pg_catalog.text OPERATOR(pg_catalog.!~~) 'pg_temp%'
           AND c.relkind::pg_catalog.text OPERATOR(pg_catalog.=) ANY (
               ARRAY['r', 'p', 'S', 'v', 'm', 'i', 'I']::pg_catalog.text[]
           )
           AND r.rolname::pg_catalog.text OPERATOR(pg_catalog.<>) $1
           AND NOT EXISTS (
               SELECT 1 FROM pg_catalog.pg_depend d
               WHERE d.classid OPERATOR(pg_catalog.=)
                     'pg_catalog.pg_class'::pg_catalog.regclass::pg_catalog.oid
                 AND d.objid OPERATOR(pg_catalog.=) c.oid
                 AND d.deptype::pg_catalog.text OPERATOR(pg_catalog.=) 'e'
           )",
    )
    .bind(APP_ROLE)
    .fetch_all(&mut *conn)
    .await
    .context("list relations")?;
    for (schema, name, kind) in relations {
        let verb = match kind.as_str() {
            "S" => "SEQUENCE",
            "v" => "VIEW",
            "m" => "MATERIALIZED VIEW",
            "i" | "I" => "INDEX",
            _ => "TABLE",
        };
        exec(
            conn,
            &format!(
                "ALTER {verb} {}.{} OWNER TO {}",
                quote_ident(&schema),
                quote_ident(&name),
                quote_ident(APP_ROLE)
            ),
        )
        .await
        .with_context(|| format!("transfer {verb} {schema}.{name}"))?;
    }

    let functions: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT n.nspname,
                p.proname,
                p.prokind::pg_catalog.text,
                pg_catalog.pg_get_function_identity_arguments(p.oid)
         FROM pg_catalog.pg_proc p
         JOIN pg_catalog.pg_namespace n
           ON n.oid OPERATOR(pg_catalog.=) p.pronamespace
         JOIN pg_catalog.pg_roles r
           ON r.oid OPERATOR(pg_catalog.=) p.proowner
         WHERE n.nspname::pg_catalog.text OPERATOR(pg_catalog.<>) 'pg_catalog'
           AND n.nspname::pg_catalog.text OPERATOR(pg_catalog.<>) 'information_schema'
           AND n.nspname::pg_catalog.text OPERATOR(pg_catalog.!~~) 'pg_toast%'
           AND n.nspname::pg_catalog.text OPERATOR(pg_catalog.!~~) 'pg_temp%'
           AND r.rolname::pg_catalog.text OPERATOR(pg_catalog.<>) $1
           AND NOT EXISTS (
               SELECT 1 FROM pg_catalog.pg_depend d
               WHERE d.classid OPERATOR(pg_catalog.=)
                     'pg_catalog.pg_proc'::pg_catalog.regclass::pg_catalog.oid
                 AND d.objid OPERATOR(pg_catalog.=) p.oid
                 AND d.deptype::pg_catalog.text OPERATOR(pg_catalog.=) 'e'
           )",
    )
    .bind(APP_ROLE)
    .fetch_all(&mut *conn)
    .await
    .context("list functions")?;
    for (schema, name, kind, args) in functions {
        let verb = match kind.as_str() {
            "p" => "PROCEDURE",
            "a" => "AGGREGATE",
            _ => "FUNCTION",
        };
        exec(
            conn,
            &format!(
                "ALTER {verb} {}.{}({args}) OWNER TO {}",
                quote_ident(&schema),
                quote_ident(&name),
                quote_ident(APP_ROLE)
            ),
        )
        .await
        .with_context(|| format!("transfer {verb} {schema}.{name}"))?;
    }

    let extensions: Vec<String> = sqlx::query_scalar(
        "SELECT e.extname
         FROM pg_catalog.pg_extension e
         JOIN pg_catalog.pg_roles r
           ON r.oid OPERATOR(pg_catalog.=) e.extowner
         WHERE e.extname::pg_catalog.text OPERATOR(pg_catalog.<>) 'plpgsql'
           AND r.rolname::pg_catalog.text OPERATOR(pg_catalog.<>) $1",
    )
    .bind(APP_ROLE)
    .fetch_all(&mut *conn)
    .await
    .context("list extensions")?;
    for extension in extensions {
        exec(
            conn,
            &format!(
                "ALTER EXTENSION {} OWNER TO {}",
                quote_ident(&extension),
                quote_ident(APP_ROLE)
            ),
        )
        .await
        .with_context(|| format!("transfer extension {extension}"))?;
    }
    Ok(())
}

async fn connect(
    host: &str,
    port: u16,
    superuser_password: &str,
    database: &str,
) -> anyhow::Result<PgConnection> {
    let mut conn = PgConnectOptions::new()
        .host(host)
        .port(port)
        .username(SUPERUSER)
        .password(superuser_password)
        .database(database)
        .options([("search_path", "pg_catalog")])
        .connect()
        .await
        .with_context(|| format!("connect to {database} as {SUPERUSER}"))?;
    // Client startup options are applied before a database GUC on PostgreSQL
    // 16. `SET` still wins if a build applies `ALTER DATABASE SET` later.
    sqlx::query("SET search_path TO pg_catalog")
        .execute(&mut conn)
        .await
        .context("pin search_path")?;
    Ok(conn)
}

async fn catalog_tx(
    conn: &mut PgConnection,
) -> anyhow::Result<sqlx::Transaction<'_, sqlx::Postgres>> {
    let mut tx = conn.begin().await.context("begin")?;
    sqlx::query("SET LOCAL search_path TO pg_catalog")
        .execute(&mut *tx)
        .await
        .context("pin search_path")?;
    Ok(tx)
}

fn write_ready_flag(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).context("record application role")?;
        }
    }
    let tmp = path.with_extension("writing");
    {
        use std::os::unix::fs::PermissionsExt;
        let file = std::fs::File::create(&tmp).context("record application role")?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .context("record application role")?;
        file.sync_all().context("record application role")?;
    }
    std::fs::rename(&tmp, path).context("record application role")?;
    Ok(())
}

async fn exec(conn: &mut PgConnection, sql: &str) -> anyhow::Result<()> {
    sqlx::query(sql)
        .execute(&mut *conn)
        .await
        .with_context(|| format!("postgres: {sql}"))?;
    Ok(())
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_database_url_uses_the_app_role() {
        let url = app_database_url(5432, "p@ss", "coppice").unwrap();
        assert_eq!(url, "postgres://coppice_app:p%40ss@127.0.0.1:5432/coppice");
        assert!(url.starts_with("postgres://coppice_app:"));
    }
}

#[cfg(all(test, feature = "embedded-test-db"))]
mod embedded_tests {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::str::FromStr;

    use sqlx::postgres::PgPoolOptions;
    use sqlx::{ConnectOptions, Connection, PgConnection};

    use super::*;
    use crate::services::backup_service::BackupService;

    const SUPER_PASSWORD: &str = "coppice";
    const APP_PASSWORD: &str = "app-role-password";
    const UPGRADE_DB: &str = "app_role_upgrade";
    const FRESH_DB: &str = "app_role_fresh";
    const IMPORT_DB: &str = "app_role_import";
    const HIJACK_DB: &str = "app_role_hijack";

    /// Held for the whole test. Nextest runs each test in its own process, so
    /// an in-process mutex would not stop two processes creating `coppice_app`.
    async fn hold_role_lock(port: u16) -> PgConnection {
        let mut conn = maintenance(port).await;
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(0x434f_5050_4150_5052_i64)
            .execute(&mut conn)
            .await
            .expect("lock application role");
        conn
    }

    async fn embedded_port() -> u16 {
        if crate::db::use_external_test_db() {
            panic!("this test needs the embedded superuser");
        }
        let url = crate::db::session_database_url()
            .await
            .expect("embedded postgres");
        PgConnectOptions::from_str(&url)
            .expect("session url")
            .get_port()
    }

    async fn maintenance(port: u16) -> PgConnection {
        connect("127.0.0.1", port, SUPER_PASSWORD, "postgres")
            .await
            .expect("maintenance connection")
    }

    async fn reset_role(port: u16) {
        let mut conn = maintenance(port).await;
        for name in [UPGRADE_DB, FRESH_DB, IMPORT_DB, HIJACK_DB] {
            let _ = sqlx::query(&format!(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{name}'"
            ))
            .execute(&mut conn)
            .await;
            let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {name}"))
                .execute(&mut conn)
                .await;
        }
        let owned: Vec<String> = sqlx::query_scalar(
            "SELECT datname FROM pg_database d
             JOIN pg_roles r ON r.oid = d.datdba
             WHERE r.rolname = $1",
        )
        .bind(APP_ROLE)
        .fetch_all(&mut conn)
        .await
        .unwrap_or_default();
        for name in owned {
            let _ = sqlx::query(&format!(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = {}",
                quote_literal(&name)
            ))
            .execute(&mut conn)
            .await;
            let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {}", quote_ident(&name)))
                .execute(&mut conn)
                .await;
        }
        sqlx::query(&format!("DROP ROLE IF EXISTS {}", quote_ident(APP_ROLE)))
            .execute(&mut conn)
            .await
            .expect("drop application role");
        conn.close().await.ok();
    }

    async fn owner_of(conn: &mut PgConnection, sql: &str) -> String {
        sqlx::query_scalar(sql).fetch_one(conn).await.expect(sql)
    }

    #[tokio::test]
    async fn existing_install_transfers_ownership_to_a_non_superuser() {
        let port = embedded_port().await;
        let _lock = hold_role_lock(port).await;
        reset_role(port).await;

        let mut admin = maintenance(port).await;
        sqlx::query(&format!("CREATE DATABASE {UPGRADE_DB}"))
            .execute(&mut admin)
            .await
            .expect("create upgrade database");
        admin.close().await.ok();

        let mut db = connect("127.0.0.1", port, SUPER_PASSWORD, UPGRADE_DB)
            .await
            .expect("upgrade database");
        sqlx::query("CREATE TABLE public.kept_rows (id int)")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("CREATE SEQUENCE public.kept_seq")
            .execute(&mut db)
            .await
            .unwrap();
        sqlx::query("CREATE FUNCTION public.kept_fn() RETURNS int LANGUAGE sql AS $$ SELECT 1 $$")
            .execute(&mut db)
            .await
            .unwrap();
        db.close().await.ok();

        let flag_dir = tempfile::tempdir().expect("tempdir");
        let flag = flag_dir.path().join(READY_FLAG_FILE);
        provision_app_role(
            "127.0.0.1",
            port,
            SUPER_PASSWORD,
            APP_PASSWORD,
            UPGRADE_DB,
            &flag,
        )
        .await
        .expect("provision");
        provision_app_role(
            "127.0.0.1",
            port,
            SUPER_PASSWORD,
            APP_PASSWORD,
            UPGRADE_DB,
            &flag,
        )
        .await
        .expect("provision again");

        let mut admin = maintenance(port).await;
        let (superuser, create_role, create_db, login, replication, bypass): (
            bool,
            bool,
            bool,
            bool,
            bool,
            bool,
        ) = sqlx::query_as(
            "SELECT rolsuper, rolcreaterole, rolcreatedb, rolcanlogin, rolreplication, rolbypassrls
             FROM pg_roles WHERE rolname = $1",
        )
        .bind(APP_ROLE)
        .fetch_one(&mut admin)
        .await
        .expect("role row");
        assert!(!superuser);
        assert!(!create_role);
        assert!(!create_db);
        assert!(login);
        assert!(!replication);
        assert!(!bypass);
        let dangerous: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_auth_members m
             JOIN pg_roles g ON g.oid = m.roleid
             JOIN pg_roles u ON u.oid = m.member
             WHERE u.rolname = $1
               AND g.rolname IN (
                   'pg_execute_server_program',
                   'pg_read_server_files',
                   'pg_write_server_files'
               )",
        )
        .bind(APP_ROLE)
        .fetch_one(&mut admin)
        .await
        .unwrap();
        assert_eq!(dangerous, 0);
        assert_eq!(
            owner_of(
                &mut admin,
                &format!(
                    "SELECT pg_catalog.pg_get_userbyid(datdba) FROM pg_database WHERE datname = '{UPGRADE_DB}'"
                ),
            )
            .await,
            APP_ROLE
        );
        admin.close().await.ok();

        let mut db = connect("127.0.0.1", port, SUPER_PASSWORD, UPGRADE_DB)
            .await
            .unwrap();
        assert_eq!(
            owner_of(
                &mut db,
                "SELECT pg_catalog.pg_get_userbyid(nspowner) FROM pg_namespace WHERE nspname = 'public'",
            )
            .await,
            APP_ROLE
        );
        for rel in ["kept_rows", "kept_seq"] {
            assert_eq!(
                owner_of(
                    &mut db,
                    &format!(
                        "SELECT pg_catalog.pg_get_userbyid(c.relowner)
                         FROM pg_class c
                         JOIN pg_namespace n ON n.oid = c.relnamespace
                         WHERE n.nspname = 'public' AND c.relname = '{rel}'"
                    ),
                )
                .await,
                APP_ROLE,
                "{rel}"
            );
        }
        assert_eq!(
            owner_of(
                &mut db,
                "SELECT pg_catalog.pg_get_userbyid(proowner) FROM pg_proc WHERE proname = 'kept_fn'",
            )
            .await,
            APP_ROLE
        );
        db.close().await.ok();

        let mut app = PgConnectOptions::new()
            .host("127.0.0.1")
            .port(port)
            .username(APP_ROLE)
            .password(APP_PASSWORD)
            .database(UPGRADE_DB)
            .connect()
            .await
            .expect("app role connects");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM kept_rows")
            .fetch_one(&mut app)
            .await
            .expect("app role reads its table");
        assert_eq!(count, 0);
        app.close().await.ok();
    }

    #[tokio::test]
    async fn fresh_database_migrates_as_the_app_role() {
        let port = embedded_port().await;
        let _lock = hold_role_lock(port).await;
        reset_role(port).await;
        let flag_dir = tempfile::tempdir().expect("tempdir");
        provision_app_role(
            "127.0.0.1",
            port,
            SUPER_PASSWORD,
            APP_PASSWORD,
            FRESH_DB,
            &flag_dir.path().join(READY_FLAG_FILE),
        )
        .await
        .expect("provision fresh database");
        let url = app_database_url(port, APP_PASSWORD, FRESH_DB).unwrap();
        assert!(url.contains("postgres://coppice_app:"));
        crate::db::connect_and_migrate(&url)
            .await
            .expect("migrations as application role");
    }

    fn zip_with_sql(sql: &str) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("manifest.json", options).unwrap();
            zip.write_all(
                br#"{"formatVersion":1,"exportedAt":"2026-01-01T00:00:00Z","appVersion":"0.0.0"}"#,
            )
            .unwrap();
            zip.start_file("database.sql", options).unwrap();
            zip.write_all(sql.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[tokio::test]
    async fn import_as_app_role_cannot_copy_to_program() {
        let port = embedded_port().await;
        let _lock = hold_role_lock(port).await;
        reset_role(port).await;
        let flag_dir = tempfile::tempdir().expect("tempdir");
        provision_app_role(
            "127.0.0.1",
            port,
            SUPER_PASSWORD,
            APP_PASSWORD,
            IMPORT_DB,
            &flag_dir.path().join(READY_FLAG_FILE),
        )
        .await
        .expect("provision import database");

        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("copy-program-marker");
        let sql = format!("COPY (SELECT 1) TO PROGRAM 'touch {}';\n", marker.display());
        let bytes = zip_with_sql(&sql);
        let mut config = crate::AppConfig::load_defaults().expect("config");
        config.database.url = app_database_url(port, APP_PASSWORD, IMPORT_DB).unwrap();
        config.storage.artifacts_dir = dir.path().join("artifacts").display().to_string();
        config.agent.worktrees_path = dir.path().join("worktrees").display().to_string();
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "postgres://{SUPERUSER}:{SUPER_PASSWORD}@127.0.0.1:{port}/{IMPORT_DB}"
            ))
            .await
            .expect("pool");
        let store = crate::crypto::SecretStore::from_master_key("desktop-app-role");

        let imported = BackupService::import_from_bytes(&config, &pool, &bytes, None, &store).await;
        let err = imported.expect_err("hostile import must fail");
        let text = err.to_string();
        assert!(
            text.contains("pg_execute_server_program"),
            "import error should be the server-program denial, got: {text}"
        );
        assert!(
            !marker.exists(),
            "COPY TO PROGRAM ran during import as {APP_ROLE}"
        );

        config.database.url =
            format!("postgres://{SUPERUSER}:{SUPER_PASSWORD}@127.0.0.1:{port}/{IMPORT_DB}");
        let _superuser_import =
            BackupService::import_from_bytes(&config, &pool, &bytes, None, &store).await;
        assert!(
            marker.exists(),
            "the same COPY TO PROGRAM did not run as the superuser, so the denial was not proven"
        );
    }

    fn trojan_dump(database: &str, marker: &std::path::Path) -> String {
        let marker = marker.display().to_string().replace('\'', "''");
        format!(
            "ALTER DATABASE {database} SET search_path TO public, pg_catalog;\n\
             CREATE FUNCTION public.pg_get_function_identity_arguments(oid)\n\
             RETURNS text LANGUAGE plpgsql AS $fn$\n\
             BEGIN\n\
               EXECUTE 'COPY (SELECT 1) TO PROGRAM ''touch {marker}''';\n\
               RETURN '';\n\
             END;\n\
             $fn$;\n"
        )
    }

    async fn legacy_owner(conn: &mut PgConnection) -> String {
        sqlx::query_scalar(
            "SELECT pg_catalog.pg_get_userbyid(p.proowner)
             FROM pg_catalog.pg_proc p
             JOIN pg_catalog.pg_namespace n
               ON n.oid OPERATOR(pg_catalog.=) p.pronamespace
             WHERE n.nspname::pg_catalog.text OPERATOR(pg_catalog.=) 'public'
               AND p.proname::pg_catalog.text OPERATOR(pg_catalog.=) 'legacy_fn'",
        )
        .fetch_one(conn)
        .await
        .expect("legacy_fn owner")
    }

    /// A dump can set this database's `search_path` and plant a function the
    /// adoption query calls. Restarting must not run that function as the
    /// superuser, so the marker file stays absent. Removing the ready flag
    /// adopts again, and that connection must not run the trojan either.
    #[tokio::test]
    async fn restart_after_a_search_path_trojan_does_not_run_copy_to_program() {
        let port = embedded_port().await;
        let _lock = hold_role_lock(port).await;
        reset_role(port).await;
        let dir = tempfile::tempdir().expect("tempdir");
        let flag = dir.path().join(READY_FLAG_FILE);
        let marker = dir.path().join("search-path-marker");
        provision_app_role(
            "127.0.0.1",
            port,
            SUPER_PASSWORD,
            APP_PASSWORD,
            HIJACK_DB,
            &flag,
        )
        .await
        .expect("provision");
        let mode = std::fs::metadata(&flag)
            .expect("ready flag")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);

        let mut admin = connect("127.0.0.1", port, SUPER_PASSWORD, HIJACK_DB)
            .await
            .expect("hijacked database");
        sqlx::query(
            "CREATE FUNCTION public.legacy_fn() RETURNS int LANGUAGE sql AS $$ SELECT 1 $$",
        )
        .execute(&mut admin)
        .await
        .expect("superuser-owned function");
        admin.close().await.ok();

        let bytes = zip_with_sql(&trojan_dump(HIJACK_DB, &marker));
        let mut config = crate::AppConfig::load_defaults().expect("config");
        config.database.url = app_database_url(port, APP_PASSWORD, HIJACK_DB).unwrap();
        config.storage.artifacts_dir = dir.path().join("artifacts").display().to_string();
        config.agent.worktrees_path = dir.path().join("worktrees").display().to_string();
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&config.database.url)
            .await
            .expect("app pool");
        let store = crate::crypto::SecretStore::from_master_key("desktop-app-role");
        let _imported =
            BackupService::import_from_bytes(&config, &pool, &bytes, None, &store).await;
        assert!(
            !marker.exists(),
            "the dump ran COPY TO PROGRAM before restart"
        );

        let mut pinned = connect("127.0.0.1", port, SUPER_PASSWORD, HIJACK_DB)
            .await
            .expect("pinned superuser connection");
        let search_path: String = sqlx::query_scalar("SHOW search_path")
            .fetch_one(&mut pinned)
            .await
            .expect("show search_path");
        assert_eq!(search_path, "pg_catalog");
        pinned.close().await.ok();

        provision_app_role(
            "127.0.0.1",
            port,
            SUPER_PASSWORD,
            APP_PASSWORD,
            HIJACK_DB,
            &flag,
        )
        .await
        .expect("restart");
        assert!(
            !marker.exists(),
            "superuser startup ran the search_path trojan"
        );
        let mut admin = connect("127.0.0.1", port, SUPER_PASSWORD, HIJACK_DB)
            .await
            .unwrap();
        assert_eq!(legacy_owner(&mut admin).await, SUPERUSER);
        admin.close().await.ok();

        std::fs::remove_file(&flag).expect("remove ready flag");
        provision_app_role(
            "127.0.0.1",
            port,
            SUPER_PASSWORD,
            APP_PASSWORD,
            HIJACK_DB,
            &flag,
        )
        .await
        .expect("adopt again");
        assert!(
            !marker.exists(),
            "adopting the hijacked database ran COPY TO PROGRAM"
        );
        assert!(flag.is_file(), "adoption did not record the ready flag");
        let mut admin = connect("127.0.0.1", port, SUPER_PASSWORD, HIJACK_DB)
            .await
            .unwrap();
        assert_eq!(legacy_owner(&mut admin).await, APP_ROLE);
        admin.close().await.ok();
    }
}
