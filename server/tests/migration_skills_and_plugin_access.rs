mod common;

use std::path::PathBuf;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool, Row};

/// Runs migration files on a fresh database so the data-migration tests can
/// insert pre-migration rows and then apply 037 and 038.
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

use std::str::FromStr;

#[tokio::test]
async fn migration_appends_skill_labels_and_skips_exact_duplicates() {
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let db = MigrationDb::create().await.expect("fresh database");
    db.apply_before(37).await.expect("migrations before 037");

    let agent_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agents (
            id, name, role, skills, responsibilities, system_prompt, connector
        )
        VALUES (
            $1, 'Ada', 'Engineer',
            ARRAY['Rust', 'SQL', 'Rust', 'Own the API'],
            ARRAY['Own the API', 'Rust'],
            'prompt', 'mock'
        )
        "#,
    )
    .bind(agent_id)
    .execute(&db.pool)
    .await
    .expect("insert agent");

    sqlx::query(
        r#"
        UPDATE agent_presets
        SET skills = ARRAY['planning', 'requirements', 'planning'],
            responsibilities = ARRAY['refine tickets', 'planning']
        WHERE key = 'pm'
        "#,
    )
    .execute(&db.pool)
    .await
    .expect("prepare pm preset");

    db.apply_version(37).await.expect("migration 037");

    let responsibilities: Vec<String> =
        sqlx::query_scalar("SELECT responsibilities FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_one(&db.pool)
            .await
            .expect("agent responsibilities");
    assert_eq!(
        responsibilities,
        vec![
            "Own the API".to_string(),
            "Rust".to_string(),
            "SQL".to_string(),
        ]
    );

    let pm: Vec<String> =
        sqlx::query_scalar("SELECT responsibilities FROM agent_presets WHERE key = 'pm'")
            .fetch_one(&db.pool)
            .await
            .expect("pm responsibilities");
    assert_eq!(
        pm,
        vec![
            "refine tickets".to_string(),
            "planning".to_string(),
            "requirements".to_string(),
        ]
    );

    let skills_columns: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM information_schema.columns
        WHERE table_name IN ('agents', 'agent_presets') AND column_name = 'skills'
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .expect("skills columns");
    assert_eq!(skills_columns, 0);

    db.close().await;
}

#[tokio::test]
async fn migration_keeps_per_agent_plugin_enables_as_an_explicit_list() {
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let db = MigrationDb::create().await.expect("fresh database");
    db.apply_before(38).await.expect("migrations before 038");

    let dir_id = uuid::Uuid::new_v4();
    let plugin_id = uuid::Uuid::new_v4();
    let kept = uuid::Uuid::new_v4();
    let other = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO plugin_dirs (id, path, position) VALUES ($1, $2, 0)")
        .bind(dir_id)
        .bind(format!("/tmp/{dir_id}"))
        .execute(&db.pool)
        .await
        .expect("plugin dir");
    sqlx::query(
        r#"
        INSERT INTO plugins (id, plugin_dir_id, rel_path, name, status, manifest)
        VALUES ($1, $2, 'sample', 'sample', 'ok', '{}')
        "#,
    )
    .bind(plugin_id)
    .bind(dir_id)
    .execute(&db.pool)
    .await
    .expect("plugin");
    for (id, name) in [(kept, "Kept"), (other, "Other")] {
        sqlx::query(
            r#"
            INSERT INTO agents (id, name, role, responsibilities, system_prompt, connector)
            VALUES ($1, $2, 'Engineer', '{}', 'prompt', 'mock')
            "#,
        )
        .bind(id)
        .bind(name)
        .execute(&db.pool)
        .await
        .expect("agent");
    }
    sqlx::query("INSERT INTO agent_plugins (agent_id, plugin_id) VALUES ($1, $2)")
        .bind(kept)
        .bind(plugin_id)
        .execute(&db.pool)
        .await
        .expect("assignment");

    db.apply_version(38).await.expect("migration 038");

    let access: String = sqlx::query_scalar("SELECT agent_access FROM plugins WHERE id = $1")
        .bind(plugin_id)
        .fetch_one(&db.pool)
        .await
        .expect("agent access");
    assert_eq!(access, "explicit");

    let assigned: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT agent_id FROM agent_plugins WHERE plugin_id = $1 ORDER BY agent_id",
    )
    .bind(plugin_id)
    .fetch_all(&db.pool)
    .await
    .expect("explicit agents");
    assert_eq!(assigned, vec![kept]);

    let all_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM plugins WHERE agent_access = 'all'")
            .fetch_one(&db.pool)
            .await
            .expect("all count");
    assert_eq!(all_count, 0);

    let row = sqlx::query(
        r#"
        SELECT column_default
        FROM information_schema.columns
        WHERE table_name = 'plugins' AND column_name = 'agent_access'
        "#,
    )
    .fetch_one(&db.pool)
    .await
    .expect("column default");
    let default_value: Option<String> = row.get("column_default");
    assert!(
        default_value.as_deref().unwrap_or("").contains("explicit"),
        "{default_value:?}"
    );

    db.close().await;
}
