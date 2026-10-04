use sqlx::PgPool;

use crate::db::pool::MIGRATOR;

/// SHA-384 checksums of migrations that were rewritten in place to drop pgvector.
/// Databases migrated before the rewrite still record these values.
const REWRITTEN_MIGRATIONS: &[(i64, &str)] = &[
    (
        1,
        "2fef5c46864881be63c0a3fba552c77843ec9b005138d8b90a3830bcb70186d76c065d5148fcacc092cd4ca1c8d73264",
    ),
    (
        13,
        "a161959585092fbf655fe12673fd85b62e70eb1d4570701cde655aa3ac571515495282d92322c75e7fb22299367157e9",
    ),
];

/// Replaces pre-rewrite checksums with the current ones so `MIGRATOR.run` accepts old databases.
/// Unknown checksums are left alone so sqlx still reports genuine mismatches.
pub(crate) async fn fix_rewritten_migration_checksums(pool: &PgPool) -> anyhow::Result<u64> {
    let has_migrations_table: bool =
        sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await?;
    if !has_migrations_table {
        return Ok(0);
    }

    let mut updated = 0;
    for (version, old_checksum) in REWRITTEN_MIGRATIONS {
        let Some(migration) = MIGRATOR.iter().find(|m| m.version == *version) else {
            continue;
        };
        updated += sqlx::query(
            r#"
            UPDATE _sqlx_migrations
            SET checksum = $1
            WHERE version = $2 AND checksum = decode($3, 'hex')
            "#,
        )
        .bind(migration.checksum.as_ref())
        .bind(version)
        .bind(old_checksum)
        .execute(pool)
        .await?
        .rows_affected();
    }
    Ok(updated)
}

#[cfg(all(test, feature = "embedded-test-db"))]
mod tests {
    use super::{MIGRATOR, REWRITTEN_MIGRATIONS};
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
            sqlx::query(
                "UPDATE _sqlx_migrations SET checksum = decode($1, 'hex') WHERE version = $2",
            )
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

        assert!(migrate_pool(&pool).await.is_err());
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
}
