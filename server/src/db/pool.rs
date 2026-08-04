use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn connect_and_migrate(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;
    migrate_pool(&pool).await?;
    Ok(pool)
}

/// Integration / lib DB tests: short timeouts so a missing Postgres fails in ~2s, not 30s.
pub async fn connect_and_migrate_for_tests(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await?;
    migrate_pool(&pool).await?;
    Ok(pool)
}

/// Fresh pool per call; the embedded default also clones an isolated database per pool.
/// Escape hatch: `COPPICE_TEST_USE_EXTERNAL_DB=1` uses the caller's shared database.
pub async fn shared_test_pool() -> anyhow::Result<PgPool> {
    #[cfg(feature = "embedded-test-db")]
    {
        if !crate::db::test_embed::use_external_test_db() {
            return crate::db::test_embed::embedded_test_pool().await;
        }
    }

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://coppice:coppice@127.0.0.1:5432/coppice".into());
    connect_and_migrate_for_tests(&database_url).await
}

pub(crate) async fn migrate_pool(pool: &PgPool) -> anyhow::Result<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

#[cfg(feature = "embedded-test-db")]
pub(crate) fn test_migration_fingerprint() -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    MIGRATOR.iter().fold(FNV_OFFSET_BASIS, |hash, migration| {
        migration
            .version
            .to_be_bytes()
            .iter()
            .chain(migration.checksum.as_ref())
            .fold(hash, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
            })
    })
}

/// Reset workspace tables for callers that need an explicit clean database.
pub async fn truncate_test_workspace(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        TRUNCATE
            knowledge_usage_logs,
            knowledge_embeddings,
            knowledge_jobs,
            knowledge_revisions,
            knowledge_items,
            notifications,
            ticket_mentions,
            attachments,
            ticket_comments,
            agent_jobs,
            agent_runs,
            tickets,
            repos,
            agents,
            projects,
            sessions,
            users
        RESTART IDENTITY CASCADE
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}
