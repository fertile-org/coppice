//! Init wizard confirm creates a repo, a board, and a starter team, and does
//! not spawn a connector check or a run.
#![cfg(feature = "embedded-test-db")]

mod common;

use std::process::Command;

use coppice_connectors::sign_in::Readiness;
use coppice_connectors::{all, CLAUDE_CODE, CURSOR, KILO_CODE, MOCK};
use coppice_server::AppState;
use time::OffsetDateTime;
use tower::ServiceExt;

fn git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(dir.path())
        .output()
        .expect("git init");
    assert!(
        output.status.success(),
        "git init: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir
}

fn confirm_body(path: &std::path::Path, board_name: &str, preset_id: &str) -> String {
    confirm_with(path, board_name, preset_id, "claude-code", "Demo")
}

fn confirm_with(
    path: &std::path::Path,
    board_name: &str,
    preset_id: &str,
    connector_id: &str,
    repo_name: &str,
) -> String {
    serde_json::json!({
        "presetId": preset_id,
        "boardName": board_name,
        "connectorId": connector_id,
        "repository": {
            "name": repo_name,
            "localPath": path,
            "remoteUrl": "",
            "defaultBranch": "main"
        }
    })
    .to_string()
}

async fn table_count(pool: &sqlx::PgPool, table: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table}");
    sqlx::query_scalar(&sql)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|err| panic!("count {table}: {err}"))
}

async fn counts(pool: &sqlx::PgPool) -> (i64, i64, i64, i64, i64) {
    (
        table_count(pool, "repos").await,
        table_count(pool, "boards").await,
        table_count(pool, "agents").await,
        table_count(pool, "agent_runs").await,
        table_count(pool, "agent_jobs").await,
    )
}

fn store(state: &AppState, id: &str, readiness: Readiness) {
    state.connector_probes.store_readiness(id, readiness);
}

fn mark_claude_ready(state: &AppState) {
    for descriptor in all() {
        if descriptor.id == MOCK {
            continue;
        }
        let readiness = if descriptor.id == CLAUDE_CODE {
            Readiness::Ready
        } else {
            Readiness::NotOnPath
        };
        store(state, descriptor.id, readiness);
    }
}

#[tokio::test]
async fn hidden_outside_desktop_mode_and_creates_nothing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|_| {}).await;
    let pool = state.db.as_ref().expect("pool");
    mark_claude_ready(&state);
    let repo = git_repo();

    let preview = app
        .clone()
        .oneshot(common::json_request("GET", "/api/init", "", &cookie, &csrf))
        .await
        .unwrap();
    assert_eq!(preview.status(), axum::http::StatusCode::NOT_FOUND);

    let confirm = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_body(repo.path(), "Board", "starter"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(confirm.status(), axum::http::StatusCode::NOT_FOUND);
    assert_eq!(counts(pool).await, (0, 0, 0, 0, 0));
    assert_eq!(
        state.connector_probes.get(CLAUDE_CODE).unwrap().probed_at,
        OffsetDateTime::UNIX_EPOCH
    );
}

#[tokio::test]
async fn checking_and_not_ready_create_nothing_and_do_not_probe() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.auth.desktop_mode = true;
    })
    .await;
    let pool = state.db.as_ref().expect("pool");
    store(&state, CLAUDE_CODE, Readiness::Ready);
    let repo = git_repo();

    let preview = app
        .clone()
        .oneshot(common::json_request("GET", "/api/init", "", &cookie, &csrf))
        .await
        .unwrap();
    assert_eq!(preview.status(), axum::http::StatusCode::OK);
    let preview_json = common::json_body(preview).await;
    assert_eq!(preview_json["status"], "checking");
    assert!(preview_json["connectors"].as_array().unwrap().is_empty());

    let confirm = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_body(repo.path(), "Board", "starter"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(confirm.status(), axum::http::StatusCode::CONFLICT);
    let body = common::json_body(confirm).await;
    assert_eq!(body["code"], "checking");
    assert_eq!(counts(pool).await, (0, 0, 0, 0, 0));
    assert!(state.connector_probes.get(CURSOR).is_none());
    assert_eq!(
        state.connector_probes.get(CLAUDE_CODE).unwrap().probed_at,
        OffsetDateTime::UNIX_EPOCH
    );

    for descriptor in all() {
        if descriptor.id == MOCK {
            continue;
        }
        let readiness = if descriptor.id == KILO_CODE {
            Readiness::Found
        } else {
            Readiness::NotOnPath
        };
        store(&state, descriptor.id, readiness);
    }
    let none = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_body(repo.path(), "Board", "starter"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(none.status(), axum::http::StatusCode::CONFLICT);
    assert_eq!(common::json_body(none).await["code"], "none_ready");
    assert_eq!(counts(pool).await, (0, 0, 0, 0, 0));
    assert_eq!(
        state.connector_probes.get(KILO_CODE).unwrap().readiness,
        Readiness::Found
    );
}

#[tokio::test]
async fn confirm_creates_repo_board_and_starter_team_without_a_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.auth.desktop_mode = true;
    })
    .await;
    let pool = state.db.as_ref().expect("pool");
    mark_claude_ready(&state);
    let repo = git_repo();

    let preview = app
        .clone()
        .oneshot(common::json_request("GET", "/api/init", "", &cookie, &csrf))
        .await
        .unwrap();
    let preview_json = common::json_body(preview).await;
    assert_eq!(preview_json["presetId"], "starter");
    assert_eq!(preview_json["status"], "ready");
    assert_eq!(preview_json["connectors"][0]["id"], "claude-code");
    assert_eq!(preview_json["connectors"][0]["displayName"], "Claude Code");
    assert_eq!(preview_json["connectors"].as_array().unwrap().len(), 1);
    let members = preview_json["members"].as_array().expect("members");
    assert_eq!(
        members
            .iter()
            .map(|member| member["presetKey"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["pm", "tech_lead", "backend_engineer"]
    );

    let missing = tempfile::tempdir().expect("not a git checkout");
    let rejected = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_body(missing.path(), "Board", "starter"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(common::json_body(rejected).await["code"], "not_git");
    assert_eq!(counts(pool).await, (0, 0, 0, 0, 0));

    let unknown = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_body(repo.path(), "Board", "other"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(unknown.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(common::json_body(unknown).await["code"], "unknown_preset");
    assert_eq!(counts(pool).await, (0, 0, 0, 0, 0));

    let created = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_body(repo.path(), "Demo", "starter"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    let created_json = common::json_body(created).await;
    assert_eq!(created_json["connectorId"], "claude-code");
    assert_eq!(created_json["connectorDisplayName"], "Claude Code");
    assert_eq!(created_json["agents"].as_array().unwrap().len(), 3);
    assert_eq!(counts(pool).await, (1, 1, 3, 0, 0));
    assert_eq!(
        state.connector_probes.get(CLAUDE_CODE).unwrap().probed_at,
        OffsetDateTime::UNIX_EPOCH
    );

    let rows = sqlx::query(
        r#"
        SELECT name, preset_source, connector, model, model_provider, system_prompt
        FROM agents
        "#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut keys = Vec::new();
    for row in &rows {
        use sqlx::Row;
        let key: String = row.get("preset_source");
        let connector: String = row.get("connector");
        let model: Option<String> = row.get("model");
        let model_provider: Option<String> = row.get("model_provider");
        let prompt: String = row.get("system_prompt");
        let name: String = row.get("name");
        assert_eq!(connector, "claude-code");
        assert!(model.is_none());
        assert!(model_provider.is_none());
        let template = state.agent_templates.get(&key).expect("template");
        assert_eq!(&prompt, template);
        assert!(!prompt.is_empty());
        if key == "pm" {
            assert_eq!(name, "PM");
        }
        keys.push(key);
    }
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "backend_engineer".to_string(),
            "pm".to_string(),
            "tech_lead".to_string()
        ]
    );

    let again = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_with(repo.path(), "Other", "starter", "claude-code", "Renamed"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(again.status(), axum::http::StatusCode::CREATED);
    let again_json = common::json_body(again).await;
    assert_eq!(again_json["repositoryId"], created_json["repositoryId"]);
    assert_eq!(counts(pool).await, (1, 2, 6, 0, 0));
    let stored_name: String = sqlx::query_scalar("SELECT name FROM repos")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(stored_name, "Demo");

    let same_path_board = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_body(repo.path(), "Demo", "starter"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(same_path_board.status(), axum::http::StatusCode::CONFLICT);
    assert_eq!(
        common::json_body(same_path_board).await["code"],
        "duplicate_board"
    );
    assert_eq!(counts(pool).await, (1, 2, 6, 0, 0));

    let other = git_repo();
    let duplicate_board = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_body(other.path(), "Demo", "starter"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(duplicate_board.status(), axum::http::StatusCode::CONFLICT);
    assert_eq!(
        common::json_body(duplicate_board).await["code"],
        "duplicate_board"
    );
    assert_eq!(counts(pool).await, (1, 2, 6, 0, 0));
}

#[tokio::test]
async fn picking_a_later_ready_cli_creates_the_team_on_it() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.auth.desktop_mode = true;
    })
    .await;
    let pool = state.db.as_ref().expect("pool");
    for descriptor in all() {
        if descriptor.id == MOCK {
            continue;
        }
        let readiness = if descriptor.id == CURSOR || descriptor.id == CLAUDE_CODE {
            Readiness::Ready
        } else {
            Readiness::NotOnPath
        };
        store(&state, descriptor.id, readiness);
    }
    let repo = git_repo();

    let preview = app
        .clone()
        .oneshot(common::json_request("GET", "/api/init", "", &cookie, &csrf))
        .await
        .unwrap();
    let preview_json = common::json_body(preview).await;
    assert_eq!(
        preview_json["connectors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|connector| connector["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["cursor", "claude-code"]
    );

    let rejected = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_with(repo.path(), "Board", "starter", "kilo-code", "Demo"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(common::json_body(rejected).await["code"], "validation");
    assert_eq!(counts(pool).await, (0, 0, 0, 0, 0));

    let created = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/init",
            &confirm_with(repo.path(), "Board", "starter", "claude-code", "Demo"),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    let created_json = common::json_body(created).await;
    assert_eq!(created_json["connectorId"], "claude-code");
    assert_eq!(created_json["connectorDisplayName"], "Claude Code");
    let connectors: Vec<String> = sqlx::query_scalar("SELECT connector FROM agents")
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(connectors.len(), 3);
    assert!(connectors
        .iter()
        .all(|connector| connector == "claude-code"));
    assert_eq!(counts(pool).await, (1, 1, 3, 0, 0));
}
