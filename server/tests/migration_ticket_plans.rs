#![cfg(feature = "embedded-test-db")]

mod common;

use std::path::PathBuf;
use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};
use time::OffsetDateTime;
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

fn ts(secs: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(secs).expect("timestamp")
}

#[tokio::test]
async fn migration_backfills_plans_approvals_and_appended_change_requests() {
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let db = MigrationDb::create().await.expect("fresh database");
    db.apply_before(53).await.expect("migrations before 053");

    let board_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let approved_id = Uuid::new_v4();
    let stale_id = Uuid::new_v4();
    let split_id = Uuid::new_v4();
    let revised_id = Uuid::new_v4();

    sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, 'Board', $2)")
        .bind(board_id)
        .bind(format!("board-{board_id}"))
        .execute(&db.pool)
        .await
        .expect("board");
    sqlx::query(
        r#"
        INSERT INTO agents (id, name, role, responsibilities, system_prompt, connector)
        VALUES ($1, 'PM', 'PM', '{}', 'prompt', 'mock')
        "#,
    )
    .bind(agent_id)
    .execute(&db.pool)
    .await
    .expect("agent");

    for (id, version) in [
        (approved_id, 1),
        (stale_id, 2),
        (split_id, 1),
        (revised_id, 1),
    ] {
        sqlx::query(
            r#"
            INSERT INTO tickets (
                id, board_id, title, description, status, created_by, content_version
            )
            VALUES ($1, $2, $3, '', 'ready', 'user', $4)
            "#,
        )
        .bind(id)
        .bind(board_id)
        .bind(format!("ticket-{id}"))
        .bind(version)
        .execute(&db.pool)
        .await
        .expect("ticket");
    }

    let approved_comment = Uuid::new_v4();
    let stale_old = Uuid::new_v4();
    let stale_new = Uuid::new_v4();
    let split_comment = Uuid::new_v4();
    let first_plan = Uuid::new_v4();
    let first_ask = Uuid::new_v4();
    let second_ask = Uuid::new_v4();
    let second_plan = Uuid::new_v4();

    async fn insert_comment(
        pool: &PgPool,
        id: Uuid,
        ticket: Uuid,
        agent_id: Uuid,
        body: &str,
        intent: &str,
        version: Option<i32>,
        at: i64,
    ) {
        sqlx::query(
            r#"
            INSERT INTO ticket_comments (
                id, ticket_id, author_type, author_id, body, intent,
                plan_content_version, created_at
            )
            VALUES ($1, $2, 'agent', $3, $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(ticket)
        .bind(agent_id)
        .bind(body)
        .bind(intent)
        .bind(version)
        .bind(ts(at))
        .execute(pool)
        .await
        .expect("comment");
    }

    insert_comment(&db.pool, approved_comment, approved_id, agent_id, "## Plan\n\nApproved.", "plan", Some(1), 1).await;
    insert_comment(&db.pool, stale_old, stale_id, agent_id, "## Plan\n\nOld.", "plan", Some(1), 1).await;
    insert_comment(&db.pool, stale_new, stale_id, agent_id, "## Plan\n\nNew.", "plan", Some(2), 2).await;
    insert_comment(&db.pool, split_comment, split_id, agent_id, "## Plan\n\nSplit.", "plan", Some(1), 1).await;
    insert_comment(&db.pool, first_plan, revised_id, agent_id, "## Plan\n\nFirst.", "plan", Some(1), 10).await;
    insert_comment(&db.pool, first_ask, revised_id, agent_id, "Use Postgres.", "review_feedback", None, 11).await;
    insert_comment(&db.pool, second_ask, revised_id, agent_id, "Split the API.", "review_feedback", None, 12).await;
    insert_comment(&db.pool, second_plan, revised_id, agent_id, "## Plan\n\nSecond.", "plan", Some(1), 13).await;

    sqlx::query(
        "UPDATE tickets SET approved_plan_comment_id = $2, approved_plan_content_version = $3 WHERE id = $1",
    )
    .bind(approved_id)
    .bind(approved_comment)
    .bind(1)
    .execute(&db.pool)
    .await
    .expect("approve current");
    sqlx::query(
        "UPDATE tickets SET approved_plan_comment_id = $2, approved_plan_content_version = $3 WHERE id = $1",
    )
    .bind(stale_id)
    .bind(stale_old)
    .bind(1)
    .execute(&db.pool)
    .await
    .expect("approve stale");
    sqlx::query(
        "UPDATE tickets SET approved_plan_comment_id = $2, approved_plan_content_version = $3 WHERE id = $1",
    )
    .bind(split_id)
    .bind(split_comment)
    .bind(1)
    .execute(&db.pool)
    .await
    .expect("approve split");

    sqlx::query(
        "INSERT INTO ticket_plan_splits (parent_ticket_id, plan_comment_id) VALUES ($1, $2)",
    )
    .bind(split_id)
    .bind(split_comment)
    .execute(&db.pool)
    .await
    .expect("split");

    for (run_id, trigger, at) in [
        (Uuid::new_v4(), first_ask, 11),
        (Uuid::new_v4(), second_ask, 12),
    ] {
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                trigger_comment_id, created_at
            )
            VALUES ($1, $2, $3, 'plan_ticket', 'succeeded', 'permissive', $4, $5)
            "#,
        )
        .bind(run_id)
        .bind(revised_id)
        .bind(agent_id)
        .bind(trigger)
        .bind(ts(at))
        .execute(&db.pool)
        .await
        .expect("plan run");
    }

    db.apply_version(53).await.expect("migration 053");

    let approved_plan: (i32, String, Option<Uuid>) = sqlx::query_as(
        r#"
        SELECT plan.revision, plan.body, ticket.approved_plan_id
        FROM tickets ticket
        JOIN ticket_plans plan ON plan.id = ticket.approved_plan_id
        WHERE ticket.id = $1
        "#,
    )
    .bind(approved_id)
    .fetch_one(&db.pool)
    .await
    .expect("approved plan");
    assert_eq!(approved_plan.0, 1);
    assert!(approved_plan.1.contains("Approved."));
    assert!(approved_plan.2.is_some());

    let stale: (Uuid, i32, Option<i32>) = sqlx::query_as(
        r#"
        SELECT ticket.approved_plan_id, ticket.approved_plan_content_version, latest.revision
        FROM tickets ticket
        JOIN ticket_plans latest ON latest.ticket_id = ticket.id
        WHERE ticket.id = $1
        ORDER BY latest.revision DESC
        LIMIT 1
        "#,
    )
    .bind(stale_id)
    .fetch_one(&db.pool)
    .await
    .expect("stale ticket");
    let latest_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM ticket_plans WHERE ticket_id = $1 ORDER BY revision DESC LIMIT 1",
    )
    .bind(stale_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_ne!(stale.0, latest_id, "approved pointer stays on the old revision");
    assert_eq!(stale.1, 1);
    assert_eq!(stale.2, Some(2));

    let split_plan: Option<Uuid> = sqlx::query_scalar(
        "SELECT plan_id FROM ticket_plan_splits WHERE parent_ticket_id = $1",
    )
    .bind(split_id)
    .fetch_one(&db.pool)
    .await
    .expect("split plan_id");
    assert!(split_plan.is_some());

    let requests: Vec<(i32, Option<String>)> = sqlx::query_as(
        "SELECT revision, change_request FROM ticket_plans WHERE ticket_id = $1 ORDER BY revision",
    )
    .bind(revised_id)
    .fetch_all(&db.pool)
    .await
    .expect("revised plans");
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].0, 1);
    assert_eq!(
        requests[0].1.as_deref(),
        Some("Use Postgres.\n\nSplit the API.")
    );
    assert_eq!(requests[1].0, 2);
    assert!(requests[1].1.is_none());

    let leftover: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ticket_comments WHERE ticket_id = $1 AND intent = 'plan'",
    )
    .bind(revised_id)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(leftover, 2, "legacy plan comments stay");

    let old_column: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM information_schema.columns
        WHERE table_name = 'tickets' AND column_name = 'approved_plan_comment_id'
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(old_column, 0);
    let version_kept: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM information_schema.columns
        WHERE table_name = 'tickets' AND column_name = 'approved_plan_content_version'
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(version_kept, 1);

    db.close().await;
}

#[tokio::test]
async fn migration_approves_plans_for_stuck_split_children_only() {
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let db = MigrationDb::create().await.expect("fresh database");
    db.apply_before(55).await.expect("migrations before 055");

    let board_id = Uuid::new_v4();
    let parent = Uuid::new_v4();
    let stuck = Uuid::new_v4();
    let top_level = Uuid::new_v4();
    let has_plan = Uuid::new_v4();
    let skip = Uuid::new_v4();
    let ready = Uuid::new_v4();
    let archived = Uuid::new_v4();

    sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, 'Board', $2)")
        .bind(board_id)
        .bind(format!("board-{board_id}"))
        .execute(&db.pool)
        .await
        .expect("board");

    // (id, parent, status, skip_planning, archived, content_version, description)
    let rows: [(Uuid, Option<Uuid>, &str, bool, bool, i32, &str); 7] = [
        (parent, None, "backlog", false, false, 1, "Epic"),
        (stuck, Some(parent), "backlog", false, false, 3, "Stuck child deliverable"),
        (top_level, None, "backlog", false, false, 1, "Top-level"),
        (has_plan, Some(parent), "backlog", false, false, 1, "Already planned"),
        (skip, Some(parent), "backlog", true, false, 1, "Skip planning"),
        (ready, Some(parent), "ready", false, false, 1, "Already ready"),
        (archived, Some(parent), "backlog", false, true, 1, "Archived"),
    ];
    for (id, parent_id, status, skip_planning, archived, version, description) in rows {
        sqlx::query(
            r#"
            INSERT INTO tickets (
                id, board_id, title, description, status, created_by, content_version,
                parent_ticket_id, skip_planning, archived_at
            )
            VALUES ($1, $2, $3, $4, $5, 'user', $6, $7, $8,
                    CASE WHEN $9 THEN now() ELSE NULL END)
            "#,
        )
        .bind(id)
        .bind(board_id)
        .bind(format!("ticket-{id}"))
        .bind(description)
        .bind(status)
        .bind(version)
        .bind(parent_id)
        .bind(skip_planning)
        .bind(archived)
        .execute(&db.pool)
        .await
        .expect("ticket");
    }

    sqlx::query(
        r#"
        INSERT INTO ticket_plans (id, ticket_id, revision, body, content_version)
        VALUES ($1, $2, 1, 'Existing plan', 1)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(has_plan)
    .execute(&db.pool)
    .await
    .expect("existing plan");

    db.apply_version(55).await.expect("migration 055");

    let stuck_row: (Option<Uuid>, Option<i32>, Option<String>, i32) = sqlx::query_as(
        r#"
        SELECT ticket.approved_plan_id, ticket.approved_plan_content_version,
               plan.body, plan.content_version
        FROM tickets ticket
        JOIN ticket_plans plan ON plan.id = ticket.approved_plan_id
        WHERE ticket.id = $1
        "#,
    )
    .bind(stuck)
    .fetch_one(&db.pool)
    .await
    .expect("stuck child approved");
    assert_eq!(stuck_row.1, Some(3));
    assert_eq!(stuck_row.2.as_deref(), Some("Stuck child deliverable"));
    assert_eq!(stuck_row.3, 3);

    for untouched in [parent, top_level, has_plan, skip, ready, archived] {
        let approved: Option<Uuid> = sqlx::query_scalar(
            "SELECT approved_plan_id FROM tickets WHERE id = $1",
        )
        .bind(untouched)
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert!(approved.is_none(), "ticket {untouched} must stay unapproved");
    }

    let stuck_plans: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ticket_plans WHERE ticket_id = $1")
            .bind(stuck)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(stuck_plans, 1);

    db.close().await;
}
