use coppice_migrations::{MIGRATOR, REWRITTEN_MIGRATIONS};
use sqlx::migrate::MigrateError;

use crate::db::pool::migrate_pool;
use crate::db::shared_test_pool;

async fn stored_checksum(pool: &sqlx::PgPool, version: i64) -> Vec<u8> {
    sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version = $1")
        .bind(version)
        .fetch_one(pool)
        .await
        .expect("stored checksum")
}

fn current_checksum(version: i64) -> Vec<u8> {
    MIGRATOR
        .iter()
        .find(|migration| migration.version == version)
        .expect("migration exists")
        .checksum
        .to_vec()
}

#[tokio::test]
async fn fixup_rewrites_known_old_checksums() {
    let pool = shared_test_pool().await.expect("pool");
    for (version, old) in REWRITTEN_MIGRATIONS {
        sqlx::query("UPDATE _sqlx_migrations SET checksum = decode($1, 'hex') WHERE version = $2")
            .bind(old)
            .bind(version)
            .execute(&pool)
            .await
            .expect("set old checksum");
    }

    migrate_pool(&pool).await.expect("migrate after fix-up");

    for (version, _) in REWRITTEN_MIGRATIONS {
        assert_eq!(
            stored_checksum(&pool, *version).await,
            current_checksum(*version)
        );
    }
}

#[tokio::test]
async fn fixup_leaves_unknown_checksums() {
    let pool = shared_test_pool().await.expect("pool");
    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = decode(repeat('00', 48), 'hex') WHERE version = 1",
    )
    .execute(&pool)
    .await
    .expect("set unknown checksum");

    let error = migrate_pool(&pool).await.expect_err("checksum mismatch");
    assert!(
        matches!(
            error.downcast_ref::<MigrateError>(),
            Some(MigrateError::VersionMismatch(1))
        ),
        "unexpected error: {error:?}"
    );
    assert_eq!(stored_checksum(&pool, 1).await, vec![0u8; 48]);
}

#[tokio::test]
async fn fresh_database_has_no_vector_extension() {
    let pool = shared_test_pool().await.expect("pool");
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_extension WHERE extname = 'vector'")
            .fetch_one(&pool)
            .await
            .expect("count vector extension");
    assert_eq!(count, 0);
}
