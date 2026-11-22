#![cfg(feature = "embedded-test-db")]
use uuid::Uuid;

mod common;

use std::path::PathBuf;
use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
struct MigrationDb {
    name: String,
    pool: PgPool,
}

impl MigrationDb {
    async fn create() -> anyhow::Result<Self> {
        let session = coppice_server::db::session_database_url().await?;
        let name = format!(
            "coppice_case_mig_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        );
        let mut admin = admin_connection(&session).await?;
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut admin)
            .await?;
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect_with(connect_options(&session, &name)?)
            .await?;
        Ok(Self { name, pool })
    }

    async fn apply_before(&self, version: i64) -> anyhow::Result<()> {
        for (file_version, path) in migration_files() {
            if file_version >= version {
                break;
            }
            apply_file(&self.pool, &path).await?;
        }
        Ok(())
    }

    async fn apply_version(&self, version: i64) -> anyhow::Result<()> {
        let path = migration_files()
            .into_iter()
            .find(|(file_version, _)| *file_version == version)
            .map(|(_, path)| path)
            .unwrap_or_else(|| panic!("migration {version} is missing"));
        apply_file(&self.pool, &path).await
    }

    async fn close(self) {
        self.pool.close().await;
        if let Ok(session) = coppice_server::db::session_database_url().await {
            if let Ok(mut admin) = admin_connection(&session).await {
                let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {}", self.name))
                    .execute(&mut admin)
                    .await;
            }
        }
    }
}

fn migration_files() -> Vec<(i64, PathBuf)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read migrations") {
        let path = entry.expect("migration entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("sql") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let version = name
            .split('_')
            .next()
            .and_then(|version| version.parse::<i64>().ok());
        if let Some(version) = version {
            files.push((version, path));
        }
    }
    files.sort_by_key(|(version, _)| *version);
    files
}

async fn apply_file(pool: &PgPool, path: &PathBuf) -> anyhow::Result<()> {
    let sql = std::fs::read_to_string(path)?;
    sqlx::raw_sql(&sql)
        .execute(pool)
        .await
        .map_err(|err| anyhow::anyhow!("migration {}: {err}", path.display()))?;
    Ok(())
}

async fn admin_connection(session_url: &str) -> anyhow::Result<PgConnection> {
    PgConnection::connect_with(&connect_options(session_url, "postgres")?)
        .await
        .map_err(Into::into)
}

fn connect_options(session_url: &str, database: &str) -> anyhow::Result<PgConnectOptions> {
    Ok(PgConnectOptions::from_str(session_url)?.database(database))
}

#[tokio::test]
async fn migration_backfills_legacy_branch_and_clears_never_ran() {
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let db = MigrationDb::create().await.expect("fresh database");
    db.apply_before(80).await.expect("migrations before 080");

    let board_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let ran_garbage = Uuid::parse_str("aaaaaaaa-0000-4000-8000-000000000001").unwrap();
    let ran_null = Uuid::parse_str("bbbbbbbb-0000-4000-8000-000000000002").unwrap();
    let never_ran = Uuid::parse_str("cccccccc-0000-4000-8000-000000000003").unwrap();
    let never_ran_null = Uuid::parse_str("dddddddd-0000-4000-8000-000000000004").unwrap();

    sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, 'branch mig', 'branch-mig')")
        .bind(board_id)
        .execute(&db.pool)
        .await
        .expect("board");
    sqlx::query(
        r#"
        INSERT INTO agents (id, name, role, responsibilities, system_prompt, connector)
        VALUES ($1, 'Worker', 'engineer', '{}', 'x', 'mock')
        "#,
    )
    .bind(agent_id)
    .execute(&db.pool)
    .await
    .expect("agent");

    for (id, title, branch) in [
        (ran_garbage, "ran garbage", Some("weird/free-form")),
        (ran_null, "ran null", None),
        (never_ran, "never ran", Some("also/weird")),
        (never_ran_null, "never ran null", None),
    ] {
        sqlx::query(
            r#"
            INSERT INTO tickets (id, board_id, title, description, status, branch_name, created_by)
            VALUES ($1, $2, $3, '', 'backlog', $4, 'test')
            "#,
        )
        .bind(id)
        .bind(board_id)
        .bind(title)
        .bind(branch)
        .execute(&db.pool)
        .await
        .expect("ticket");
    }

    for ticket_id in [ran_garbage, ran_null] {
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                context_profile, worktree_path, branch_name
            )
            VALUES ($1, $2, $3, 'work_on_ticket', 'succeeded', 'permissive-default',
                    'full', '/tmp/wt', 'agent/TICKET-old')
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(ticket_id)
        .bind(agent_id)
        .execute(&db.pool)
        .await
        .expect("run");
    }

    db.apply_version(80).await.expect("migration 080");

    let rows: Vec<(Uuid, Option<String>)> =
        sqlx::query_as("SELECT id, branch_name FROM tickets ORDER BY title")
            .fetch_all(&db.pool)
            .await
            .expect("tickets after");

    let by_id: std::collections::HashMap<_, _> = rows.into_iter().collect();
    assert_eq!(
        by_id.get(&ran_garbage).and_then(|v| v.as_deref()),
        Some("agent/TICKET-aaaaaaaa")
    );
    assert_eq!(
        by_id.get(&ran_null).and_then(|v| v.as_deref()),
        Some("agent/TICKET-bbbbbbbb")
    );
    assert_eq!(by_id.get(&never_ran).and_then(|v| v.as_deref()), None);
    assert_eq!(by_id.get(&never_ran_null).and_then(|v| v.as_deref()), None);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tickets WHERE branch_name IS NOT NULL")
        .fetch_one(&db.pool)
        .await
        .expect("count");
    assert_eq!(count, 2);

    db.close().await;
}
