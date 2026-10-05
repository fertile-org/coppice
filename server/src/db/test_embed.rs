use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use tokio::sync::OnceCell;

#[cfg(feature = "embedded-test-db")]
use fs4::fs_std::FileExt;
#[cfg(feature = "embedded-test-db")]
use pg_embed::pg_access::PgAccess;
#[cfg(feature = "embedded-test-db")]
use pg_embed::pg_enums::PgAuthMethod;
#[cfg(feature = "embedded-test-db")]
use pg_embed::pg_fetch::{PgFetchSettings, PG_V16};
#[cfg(feature = "embedded-test-db")]
use pg_embed::postgres::{PgEmbed, PgSettings};

const TEST_DB: &str = "coppice_test";
const TEMPLATE_DATABASE_PREFIX: &str = "coppice_template_v2_";
const CASE_DATABASE_PREFIX: &str = "coppice_case_";
const TEST_DATABASE_LOCK: i64 = 0x434f_5050_4943_4554;
const CASE_DATABASE_GRACE: Duration = Duration::from_secs(60);
const TEST_USER: &str = "coppice";
const TEST_PASSWORD: &str = "coppice";

static SESSION_URL: OnceCell<String> = OnceCell::const_new();
static TEMPLATE_DATABASE: OnceCell<String> = OnceCell::const_new();
static CASE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestPgSession {
    port: u16,
}

impl TestPgSession {
    fn database_url(&self) -> String {
        format!(
            "postgres://{TEST_USER}:{TEST_PASSWORD}@127.0.0.1:{}/{TEST_DB}",
            self.port
        )
    }
}

pub fn use_external_test_db() -> bool {
    std::env::var("COPPICE_TEST_USE_EXTERNAL_DB").as_deref() == Ok("1")
}

/// Shared Postgres URL for all tests in this process (and other processes on this machine).
pub async fn session_database_url() -> anyhow::Result<String> {
    if use_external_test_db() {
        return std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("COPPICE_TEST_USE_EXTERNAL_DB=1 requires DATABASE_URL"));
    }

    SESSION_URL
        .get_or_try_init(|| async {
            #[cfg(not(feature = "embedded-test-db"))]
            {
                anyhow::bail!(
                    "embedded postgres requires the embedded-test-db feature; run: cargo test --features embedded-test-db"
                );
            }
            #[cfg(feature = "embedded-test-db")]
            {
                resolve_shared_session_url().await
            }
        })
        .await
        .cloned()
}

/// Fresh, migrated database per call — isolated across parallel test runtimes.
pub async fn embedded_test_pool() -> anyhow::Result<PgPool> {
    if use_external_test_db() {
        let url = session_database_url().await?;
        return crate::db::pool::connect_and_migrate_for_tests(&url).await;
    }

    let session_url = session_database_url().await?;
    let template = template_database(&session_url).await?;
    clone_case_database(&session_url, &template).await
}

async fn template_database(session_url: &str) -> anyhow::Result<String> {
    TEMPLATE_DATABASE
        .get_or_try_init(|| async {
            let template = format!(
                "{TEMPLATE_DATABASE_PREFIX}{:016x}",
                crate::db::pool::test_migration_fingerprint()
            );
            // The template only appears under its final name once fully
            // migrated, so cloning processes never connect to it: Postgres
            // refuses `CREATE DATABASE ... TEMPLATE` while it has connections.
            let mut admin = admin_connection(session_url).await?;
            if database_exists(&mut admin, &template).await? {
                return Ok(template);
            }

            acquire_test_database_lock(&mut admin).await?;
            if database_exists(&mut admin, &template).await? {
                return Ok(template);
            }

            let building = format!("{template}_building");
            drop_database(&mut admin, &building).await?;
            create_database(&mut admin, &building, None).await?;
            let pool = connect_database_pool(session_url, &building, 2)
                .await
                .with_context(|| format!("connect test template database {building}"))?;
            let migration_result = crate::db::pool::migrate_pool(&pool)
                .await
                .with_context(|| format!("migrate test template database {building}"));
            pool.close().await;
            migration_result?;

            let rename = format!(
                "ALTER DATABASE {} RENAME TO {}",
                quote_identifier(&building),
                quote_identifier(&template)
            );
            sqlx::query(&rename)
                .execute(&mut admin)
                .await
                .with_context(|| format!("publish test template database {template}"))?;

            Ok::<String, anyhow::Error>(template)
        })
        .await
        .cloned()
}

async fn clone_case_database(session_url: &str, template: &str) -> anyhow::Result<PgPool> {
    let mut admin = admin_connection(session_url).await?;
    cleanup_inactive_case_databases(&mut admin).await?;

    let case = format!(
        "{CASE_DATABASE_PREFIX}{}_{}_{}",
        unix_millis(),
        std::process::id(),
        CASE_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    create_database(&mut admin, &case, Some(template)).await?;

    match connect_database_pool(session_url, &case, 10).await {
        Ok(pool) => Ok(pool),
        Err(error) => {
            let _ = drop_database(&mut admin, &case).await;
            Err(error).with_context(|| format!("connect isolated test database {case}"))
        }
    }
}

async fn admin_connection(session_url: &str) -> anyhow::Result<PgConnection> {
    let options = database_connect_options(session_url, "postgres")?;
    PgConnection::connect_with(&options)
        .await
        .context("connect embedded Postgres admin database")
}

fn database_connect_options(session_url: &str, database: &str) -> anyhow::Result<PgConnectOptions> {
    Ok(PgConnectOptions::from_str(session_url)
        .context("parse embedded Postgres session URL")?
        .database(database))
}

async fn connect_database_pool(
    session_url: &str,
    database: &str,
    max_connections: u32,
) -> anyhow::Result<PgPool> {
    let options = database_connect_options(session_url, database)?;
    PgPoolOptions::new()
        // Case cleanup treats this as a sentinel; template pools are closed explicitly.
        .min_connections(1)
        .max_connections(max_connections)
        .max_lifetime(None)
        .idle_timeout(None)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await
        .map_err(Into::into)
}

async fn acquire_test_database_lock(admin: &mut PgConnection) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(TEST_DATABASE_LOCK)
        .execute(&mut *admin)
        .await
        .context("acquire embedded test database lock")?;
    Ok(())
}

async fn database_exists(admin: &mut PgConnection, database: &str) -> anyhow::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
        .bind(database)
        .fetch_one(&mut *admin)
        .await
        .with_context(|| format!("check whether test database {database} exists"))
}

fn unix_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

/// A case database is stale once it has no connections and is older than
/// `CASE_DATABASE_GRACE` — younger ones may belong to a test that created
/// it but has not connected yet. Names without a timestamp are stale.
fn case_database_is_stale(name: &str, now_millis: u128) -> bool {
    let created = name
        .strip_prefix(CASE_DATABASE_PREFIX)
        .and_then(|rest| rest.split('_').next())
        .and_then(|millis| millis.parse::<u128>().ok());
    match created {
        Some(created) if created > 1_000_000_000_000 => {
            now_millis.saturating_sub(created) > CASE_DATABASE_GRACE.as_millis()
        }
        _ => true,
    }
}

/// Lock-free so parallel test processes can clone concurrently; a racing
/// drop of the same database is harmless and ignored.
async fn cleanup_inactive_case_databases(admin: &mut PgConnection) -> anyhow::Result<()> {
    let inactive: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT database.datname
        FROM pg_database AS database
        WHERE starts_with(database.datname, $1)
          AND NOT EXISTS (
              SELECT 1
              FROM pg_stat_activity AS activity
              WHERE activity.datname = database.datname
          )
        "#,
    )
    .bind(CASE_DATABASE_PREFIX)
    .fetch_all(&mut *admin)
    .await
    .context("list inactive embedded test databases")?;

    let now = unix_millis();
    for database in inactive {
        if case_database_is_stale(&database, now) {
            let _ = drop_database(admin, &database).await;
        }
    }
    Ok(())
}

async fn create_database(
    admin: &mut PgConnection,
    database: &str,
    template: Option<&str>,
) -> anyhow::Result<()> {
    let database = quote_identifier(database);
    let statement = match template {
        Some(template) => format!(
            "CREATE DATABASE {database} TEMPLATE {}",
            quote_identifier(template)
        ),
        None => format!("CREATE DATABASE {database}"),
    };
    sqlx::query(&statement)
        .execute(&mut *admin)
        .await
        .with_context(|| format!("create embedded test database with `{statement}`"))?;
    Ok(())
}

async fn drop_database(admin: &mut PgConnection, database: &str) -> anyhow::Result<()> {
    let statement = format!("DROP DATABASE IF EXISTS {}", quote_identifier(database));
    sqlx::query(&statement)
        .execute(&mut *admin)
        .await
        .with_context(|| format!("drop inactive embedded test database {database}"))?;
    Ok(())
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

#[cfg(feature = "embedded-test-db")]
async fn resolve_shared_session_url() -> anyhow::Result<String> {
    if let Some(session) = read_session_file() {
        if session_reachable(&session).await {
            return Ok(session.database_url());
        }
    }

    let _leader = lock_session_leader()?;

    if let Some(session) = read_session_file() {
        if session_reachable(&session).await {
            return Ok(session.database_url());
        }
    }

    let session = start_shared_embedded_pg().await?;
    write_session_file(&session)?;
    Ok(session.database_url())
}

/// Held while downloading binaries or starting the shared cluster, across processes.
#[cfg(feature = "embedded-test-db")]
fn lock_session_leader() -> anyhow::Result<File> {
    let session_dir = session_dir_path();
    std::fs::create_dir_all(&session_dir)?;
    let lock_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(session_dir.join("leader.lock"))?;
    lock_file.lock_exclusive()?;
    Ok(lock_file)
}

#[cfg(feature = "embedded-test-db")]
fn pg_fetch_settings() -> PgFetchSettings {
    PgFetchSettings {
        version: PG_V16,
        ..Default::default()
    }
}

/// pg-embed's cached Postgres install (`bin/`, `lib/`, `share/`), downloaded if missing.
#[cfg(feature = "embedded-test-db")]
pub async fn embedded_pg_install_dir() -> anyhow::Result<PathBuf> {
    let _leader = lock_session_leader()?;
    let access = PgAccess::new(&pg_fetch_settings(), &session_dir_path().join("cluster")).await?;
    access.maybe_acquire_postgres().await?;
    Ok(access.cache_dir)
}

#[cfg(feature = "embedded-test-db")]
async fn session_reachable(session: &TestPgSession) -> bool {
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(2))
        .connect(&session.database_url())
        .await
        .is_ok()
}

#[cfg(feature = "embedded-test-db")]
async fn start_shared_embedded_pg() -> anyhow::Result<TestPgSession> {
    let port = pick_free_port()?;
    let cluster_dir = session_dir_path().join("cluster");

    let pg_settings = PgSettings {
        database_dir: cluster_dir,
        port,
        user: TEST_USER.to_string(),
        password: TEST_PASSWORD.to_string(),
        auth_method: PgAuthMethod::Plain,
        persistent: true,
        timeout: Some(Duration::from_secs(30)),
        migration_dir: None,
    };

    let mut pg = PgEmbed::new(pg_settings, pg_fetch_settings()).await?;
    pg.setup().await?;
    pg.start_db().await?;
    if !pg.database_exists(TEST_DB).await.unwrap_or(false) {
        pg.create_database(TEST_DB).await?;
    }

    std::mem::forget(pg);

    Ok(TestPgSession { port })
}

fn session_dir_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("coppice")
        .join("test-pg")
}

fn session_file_path() -> PathBuf {
    session_dir_path().join("session.json")
}

fn read_session_file() -> Option<TestPgSession> {
    let path = session_file_path();
    let mut file = File::open(path).ok()?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_session_file(session: &TestPgSession) -> anyhow::Result<()> {
    let path = session_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    file.write_all(serde_json::to_string(session)?.as_bytes())?;
    Ok(())
}

fn pick_free_port() -> anyhow::Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn embedded_test_pool_connects_and_migrates() {
        let pool = super::embedded_test_pool().await.expect("embedded pool");
        crate::db::truncate_test_workspace(&pool)
            .await
            .expect("truncate");
        let row: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
        assert_eq!(row.0, 1);

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn embedded_test_pools_isolate_fixture_data() {
        if super::use_external_test_db() {
            // The manual escape hatch intentionally uses one shared database.
            return;
        }

        let first = super::embedded_test_pool().await.expect("first pool");
        let second = super::embedded_test_pool().await.expect("second pool");
        crate::db::truncate_test_workspace(&first)
            .await
            .expect("truncate first");

        let board_id = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
            .bind(board_id)
            .bind("isolated board")
            .bind(format!("isolated-{board_id}"))
            .execute(&first)
            .await
            .expect("insert board");

        crate::db::truncate_test_workspace(&second)
            .await
            .expect("truncate second");

        sqlx::query(
            r#"
            INSERT INTO tickets (id, board_id, title, status, created_by)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(board_id)
        .bind("isolated ticket")
        .bind("backlog")
        .bind("test")
        .execute(&first)
        .await
        .expect("first fixture survives second reset");

        let second_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM boards")
            .fetch_one(&second)
            .await
            .expect("count second boards");
        assert_eq!(second_count, 0);
    }

    #[test]
    fn case_database_staleness_honours_grace_and_legacy_names() {
        let now = 1_800_000_000_000u128;
        let fresh = format!("{}{}_42_0", super::CASE_DATABASE_PREFIX, now - 5_000);
        let old = format!("{}{}_42_0", super::CASE_DATABASE_PREFIX, now - 120_000);
        let legacy = format!("{}4242_7", super::CASE_DATABASE_PREFIX);
        assert!(!super::case_database_is_stale(&fresh, now));
        assert!(super::case_database_is_stale(&old, now));
        assert!(super::case_database_is_stale(&legacy, now));
    }
}
