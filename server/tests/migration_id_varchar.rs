#![cfg(feature = "embedded-test-db")]

mod common;

use std::path::PathBuf;
use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use uuid::Uuid;

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

async fn assert_no_uuid_columns(pool: &PgPool) {
    let uuid_cols: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT table_name, column_name, data_type
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND (data_type = 'uuid' OR udt_name = '_uuid')
        ORDER BY table_name, column_name
        "#,
    )
    .fetch_all(pool)
    .await
    .expect("information_schema");
    assert!(
        uuid_cols.is_empty(),
        "uuid columns remain: {uuid_cols:?}"
    );
}

#[tokio::test]
async fn migrated_schema_has_no_uuid_columns() {
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let pool = coppice_server::db::shared_test_pool()
        .await
        .expect("embedded test database");
    assert_no_uuid_columns(&pool).await;
}

#[tokio::test]
async fn migration_082_upgrades_uuid_rows_fks_and_checks() {
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let db = MigrationDb::create().await.expect("fresh database");
    db.apply_before(82).await.expect("migrations before 082");

    let board_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let ticket_id = Uuid::parse_str("aaaaaaaa-0000-4000-8000-000000000001").unwrap();
    let child_a = Uuid::parse_str("bbbbbbbb-0000-4000-8000-000000000002").unwrap();
    let child_b = Uuid::parse_str("cccccccc-0000-4000-8000-000000000003").unwrap();
    let comment_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let attachment_id = Uuid::new_v4();

    sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, 'id mig', 'id-mig')")
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

    sqlx::query(
        r#"
        INSERT INTO tickets (
            id, board_id, title, description, status, created_by, attachment_ids
        )
        VALUES ($1, $2, 'parent', '', 'backlog', 'test', $3)
        "#,
    )
    .bind(ticket_id)
    .bind(board_id)
    .bind(&[attachment_id][..])
    .execute(&db.pool)
    .await
    .expect("ticket");

    for (id, title) in [(child_a, "child a"), (child_b, "child b")] {
        sqlx::query(
            r#"
            INSERT INTO tickets (id, board_id, title, description, status, parent_ticket_id, created_by)
            VALUES ($1, $2, $3, '', 'backlog', $4, 'test')
            "#,
        )
        .bind(id)
        .bind(board_id)
        .bind(title)
        .bind(ticket_id)
        .execute(&db.pool)
        .await
        .expect("child");
    }

    sqlx::query(
        r#"
        INSERT INTO ticket_plan_splits (parent_ticket_id, child_order)
        VALUES ($1, $2)
        "#,
    )
    .bind(ticket_id)
    .bind(&[child_a, child_b][..])
    .execute(&db.pool)
    .await
    .expect("child_order");

    sqlx::query(
        r#"
        INSERT INTO ticket_comments (id, ticket_id, author_type, author_id, body)
        VALUES ($1, $2, 'agent', $3, 'hello')
        "#,
    )
    .bind(comment_id)
    .bind(ticket_id)
    .bind(agent_id)
    .execute(&db.pool)
    .await
    .expect("comment");

    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, agent_id, job_type, status, sandbox_profile_id, context_profile
        )
        VALUES ($1, $2, $3, 'work_on_ticket', 'succeeded', 'permissive-default', 'full')
        "#,
    )
    .bind(run_id)
    .bind(ticket_id)
    .bind(agent_id)
    .execute(&db.pool)
    .await
    .expect("run");

    db.apply_version(82).await.expect("migration 082");

    assert_no_uuid_columns(&db.pool).await;

    let (title, attachment_ids): (String, Vec<String>) = sqlx::query_as(
        "SELECT title, attachment_ids FROM tickets WHERE id = $1",
    )
    .bind(ticket_id.to_string())
    .fetch_one(&db.pool)
    .await
    .expect("ticket after");
    assert_eq!(title, "parent");
    assert_eq!(attachment_ids, vec![attachment_id.to_string()]);

    let child_order: Vec<String> = sqlx::query_scalar(
        "SELECT child_order FROM ticket_plan_splits WHERE parent_ticket_id = $1",
    )
    .bind(ticket_id.to_string())
    .fetch_one(&db.pool)
    .await
    .expect("child_order after");
    assert_eq!(
        child_order,
        vec![child_a.to_string(), child_b.to_string()]
    );

    let comment_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ticket_comments WHERE ticket_id = $1")
            .bind(ticket_id.to_string())
            .fetch_one(&db.pool)
            .await
            .expect("comments");
    assert_eq!(comment_count, 1);

    let run_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM agent_runs WHERE ticket_id = $1")
            .bind(ticket_id.to_string())
            .fetch_one(&db.pool)
            .await
            .expect("runs");
    assert_eq!(run_count, 1);

    // ON DELETE CASCADE from ticket → comments still works
    sqlx::query("DELETE FROM tickets WHERE id = $1")
        .bind(ticket_id.to_string())
        .execute(&db.pool)
        .await
        .expect("delete parent");
    let orphan_comments: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ticket_comments WHERE id = $1")
            .bind(comment_id.to_string())
            .fetch_one(&db.pool)
            .await
            .expect("orphan comments");
    assert_eq!(orphan_comments, 0);

    // CHECK rejects a bad PK id
    let bad = sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, 'bad', 'bad-id')")
        .bind("NOT-A-VALID-ID")
        .execute(&db.pool)
        .await;
    assert!(bad.is_err(), "CHECK should reject bad board id");

    // Short id is accepted on a PK
    sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, 'short', 'short-id')")
        .bind("abc123xyz789")
        .execute(&db.pool)
        .await
        .expect("short id PK");

    db.close().await;
}
