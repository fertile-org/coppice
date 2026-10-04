use sqlx::PgPool;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../server/migrations");

/// SHA-384 checksums of migrations that were rewritten in place to drop pgvector.
/// Databases migrated before the rewrite still record these values.
pub const REWRITTEN_MIGRATIONS: &[(i64, &str)] = &[
    (
        1,
        "2fef5c46864881be63c0a3fba552c77843ec9b005138d8b90a3830bcb70186d76c065d5148fcacc092cd4ca1c8d73264",
    ),
    (
        13,
        "a161959585092fbf655fe12673fd85b62e70eb1d4570701cde655aa3ac571515495282d92322c75e7fb22299367157e9",
    ),
];

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    let fixed = fix_rewritten_migration_checksums(pool).await?;
    if fixed > 0 {
        tracing::info!(
            rows = fixed,
            "updated checksums of migrations rewritten to drop pgvector"
        );
    }
    MIGRATOR.run(pool).await?;
    Ok(())
}

/// Replaces pre-rewrite checksums with the current ones so `MIGRATOR.run` accepts old databases.
/// Unknown checksums are left alone so sqlx still reports genuine mismatches.
pub async fn fix_rewritten_migration_checksums(pool: &PgPool) -> anyhow::Result<u64> {
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
