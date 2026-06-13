use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::OnceCell;

#[cfg(feature = "embedded-test-db")]
use fs4::fs_std::FileExt;
#[cfg(feature = "embedded-test-db")]
use pg_embed::pg_enums::PgAuthMethod;
#[cfg(feature = "embedded-test-db")]
use pg_embed::pg_fetch::{PgFetchSettings, PG_V16};
#[cfg(feature = "embedded-test-db")]
use pg_embed::postgres::{PgEmbed, PgSettings};

const TEST_DB: &str = "coppice_test";
const PGVECTOR_RELEASE: &str = "v0.16.105";
const TEST_USER: &str = "coppice";
const TEST_PASSWORD: &str = "coppice";

static SESSION_URL: OnceCell<String> = OnceCell::const_new();
static SESSION_MIGRATED: OnceCell<()> = OnceCell::const_new();

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

/// Fresh pool per call — safe across parallel `#[tokio::test]` runtimes.
pub async fn embedded_test_pool() -> anyhow::Result<PgPool> {
    let url = session_database_url().await?;

    SESSION_MIGRATED
        .get_or_try_init(|| async {
            let pool = PgPoolOptions::new()
                .max_connections(2)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&url)
                .await?;
            crate::db::pool::migrate_pool(&pool).await?;
            pool.close().await;
            Ok::<(), anyhow::Error>(())
        })
        .await?;

    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&url)
        .await
        .map_err(Into::into)
}

#[cfg(feature = "embedded-test-db")]
async fn resolve_shared_session_url() -> anyhow::Result<String> {
    if let Some(session) = read_session_file() {
        if session_reachable(&session).await {
            return Ok(session.database_url());
        }
    }

    let session_dir = session_dir_path();
    std::fs::create_dir_all(&session_dir)?;
    let lock_path = session_dir.join("leader.lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;

    lock_file.lock_exclusive()?;

    if let Some(session) = read_session_file() {
        if session_reachable(&session).await {
            return Ok(session.database_url());
        }
    }

    let session = start_shared_embedded_pg().await?;
    write_session_file(&session)?;
    Ok(session.database_url())
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

    let fetch_settings = PgFetchSettings {
        version: PG_V16,
        ..Default::default()
    };

    let mut pg = PgEmbed::new(pg_settings, fetch_settings).await?;
    pg.setup().await?;

    let pgvector_root = ensure_pgvector_extension().await?;
    pg.install_extension(&pgvector_root.join("lib")).await?;
    pg.install_extension(&pgvector_root.join("share/extension"))
        .await?;
    stage_extension_libs_in_postgresql_libdir(&pg, &pgvector_root.join("lib")).await?;

    pg.start_db().await?;
    pg.create_database(TEST_DB).await?;

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

#[cfg(feature = "embedded-test-db")]
fn pgvector_target_triple() -> anyhow::Result<&'static str> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return Ok("aarch64-apple-darwin");
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return Ok("x86_64-apple-darwin");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Ok("x86_64-unknown-linux-gnu");
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return Ok("x86_64-pc-windows-msvc");
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    anyhow::bail!("unsupported platform for embedded pgvector; set COPPICE_TEST_USE_EXTERNAL_DB=1");
}

#[cfg(feature = "embedded-test-db")]
async fn ensure_pgvector_extension() -> anyhow::Result<PathBuf> {
    let target = pgvector_target_triple()?;
    let cache_root = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("coppice")
        .join("pgvector")
        .join(PGVECTOR_RELEASE)
        .join(target);

    let control = cache_root.join("share/extension/vector.control");
    if control.exists() {
        return Ok(cache_root);
    }

    std::fs::create_dir_all(&cache_root)?;

    let archive_name = format!("pgvector-{target}-pg16.tar.gz");
    let url = format!(
        "https://github.com/portalcorp/pgvector_compiled/releases/download/{PGVECTOR_RELEASE}/{archive_name}"
    );

    let response = reqwest::get(&url).await.map_err(|e| {
        anyhow::anyhow!("failed to download pgvector from {url}: {e} (network required once)")
    })?;

    if !response.status().is_success() {
        anyhow::bail!(
            "failed to download pgvector ({}) from {url}; set COPPICE_TEST_USE_EXTERNAL_DB=1 to use external Postgres",
            response.status()
        );
    }

    let bytes = response.bytes().await?;
    let archive_path = cache_root.parent().unwrap().join(&archive_name);
    tokio::fs::write(&archive_path, &bytes).await?;
    extract_tar_gz(&archive_path, &cache_root)?;

    if !control.exists() {
        anyhow::bail!(
            "pgvector archive did not contain share/extension/vector.control; check cache at {}",
            cache_root.display()
        );
    }

    Ok(cache_root)
}

#[cfg(feature = "embedded-test-db")]
async fn stage_extension_libs_in_postgresql_libdir(
    pg: &PgEmbed,
    extension_lib_dir: &Path,
) -> anyhow::Result<()> {
    let pg_lib_dir = pg.pg_access.cache_dir.join("lib/postgresql");
    tokio::fs::create_dir_all(&pg_lib_dir).await?;
    for entry in std::fs::read_dir(extension_lib_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_lib = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| matches!(ext, "so" | "dylib" | "dll"));
        if !is_lib {
            continue;
        }
        if let Some(name) = path.file_name() {
            tokio::fs::copy(&path, pg_lib_dir.join(name)).await?;
        }
    }
    Ok(())
}

#[cfg(feature = "embedded-test-db")]
fn extract_tar_gz(archive: &Path, dest: &Path) -> anyhow::Result<()> {
    let status = std::process::Command::new("tar")
        .args(["-xzf"])
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .status()?;

    if !status.success() {
        anyhow::bail!("tar failed extracting {}", archive.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn embedded_test_pool_connects_and_migrates() {
        let pool = super::embedded_test_pool().await.expect("embedded pool");
        crate::db::truncate_test_workspace(&pool)
            .await
            .expect("truncate");
        let row: (i32,) = sqlx::query_as("SELECT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row.0, 1);

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
