mod common;

use axum::http::StatusCode;
use std::path::{Path, PathBuf};
use std::process::Command;
use tower::ServiceExt;

fn branch_exists(repo: &Path, ticket_id: uuid::Uuid) -> bool {
    let short = ticket_id.to_string().split('-').next().unwrap().to_string();
    let branch = format!("agent/TICKET-{short}");
    let output = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")])
        .current_dir(repo)
        .status()
        .expect("show-ref");
    output.success()
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

async fn setup() -> (
    tempfile::TempDir,
    common::AgentTestEnv,
    std::sync::Arc<coppice_server::AppState>,
    axum::Router,
    String,
    String,
    uuid::Uuid,
    PathBuf,
    PathBuf,
) {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let worktree = common::setup_worktree_with_commit(
        &local_path,
        env.worktrees.path(),
        "test-repo",
        &ticket_id,
    );
    (
        git_dir,
        env,
        state,
        app,
        cookie,
        csrf,
        ticket_id.parse().unwrap(),
        local_path,
        worktree,
    )
}

#[tokio::test]
async fn list_skips_system_dirs_and_returns_without_waiting_for_sizes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_git, env, _state, app, cookie, csrf, ticket_id, _repo, worktree) = setup().await;
    for name in ["chat", "plan-scratch", "plugin-builds", "knowledge-compaction"] {
        let dir = env.worktrees.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("keep.txt"), "x").unwrap();
    }

    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/worktrees",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    let list = body["worktrees"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["folderName"], folder_name(&worktree));
    assert_eq!(list[0]["ticketId"], ticket_id.to_string());
    assert!(list[0]["sizeBytes"].is_null());
    for name in ["chat", "plan-scratch", "plugin-builds", "knowledge-compaction"] {
        assert!(
            list.iter().all(|w| w["folderName"] != name),
            "listed system dir {name}"
        );
    }
}

#[tokio::test]
async fn agent_only_changes_are_not_uncommitted_and_finished_selects_clean_done() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_git, _env, state, app, cookie, csrf, ticket_id, _repo, worktree) = setup().await;
    let pool = state.db.clone().unwrap();
    std::fs::create_dir_all(worktree.join(".agent")).unwrap();
    std::fs::write(worktree.join(".agent").join("context.md"), "ctx").unwrap();

    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(ticket_id)
        .execute(&pool)
        .await
        .unwrap();

    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/worktrees",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let body: serde_json::Value = common::json_body(res).await;
    let row = &body["worktrees"][0];
    assert_eq!(row["hasUncommittedChanges"], false);
    assert_eq!(row["finished"], true);

    std::fs::write(worktree.join("dirty.txt"), "nope").unwrap();
    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/worktrees",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let body: serde_json::Value = common::json_body(res).await;
    let row = &body["worktrees"][0];
    assert_eq!(row["hasUncommittedChanges"], true);
    assert_eq!(row["finished"], false);
}

#[tokio::test]
async fn remove_keeps_branch_and_posts_comment_for_archived_and_unmatched() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_git, env, state, app, cookie, csrf, ticket_id, repo, worktree) = setup().await;
    let pool = state.db.clone().unwrap();

    // Mark archived in SQL so the archive cleanup task does not race this test.
    sqlx::query("UPDATE tickets SET archived_at = now() WHERE id = $1")
        .bind(ticket_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(worktree.exists());

    let unmatched = env.worktrees.path().join("TICKET-deadbeef-orphan");
    std::fs::create_dir_all(&unmatched).unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(&unmatched)
        .status()
        .unwrap();
    assert!(unmatched.join(".git").exists());

    let body = serde_json::json!({
        "folderNames": [folder_name(&worktree), folder_name(&unmatched)]
    });
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/worktrees/remove",
            &body.to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r["status"] == "removed"));
    assert!(!worktree.exists());
    assert!(!unmatched.exists());
    assert!(branch_exists(&repo, ticket_id));

    let comments: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)::bigint FROM ticket_comments
        WHERE ticket_id = $1 AND body LIKE '%Worktree removed%'
        "#,
    )
    .bind(ticket_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(comments.0 >= 1);
}

#[tokio::test]
async fn active_run_blocks_one_item_while_others_succeed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_git, env, state, app, cookie, csrf, ticket_id, repo, worktree) = setup().await;
    let pool = state.db.clone().unwrap();

    let board_id: uuid::Uuid = sqlx::query_scalar("SELECT board_id FROM tickets WHERE id = $1")
        .bind(ticket_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let other_id =
        common::create_test_ticket(&app, &board_id.to_string(), &cookie, &csrf).await;
    let repo_id = sqlx::query_scalar::<_, uuid::Uuid>("SELECT repo_id FROM tickets WHERE id = $1")
        .bind(ticket_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    common::set_ticket_repo(&app, &other_id, &repo_id.to_string(), &cookie, &csrf).await;
    let other_wt = common::setup_worktree_with_commit(
        &repo,
        env.worktrees.path(),
        "test-repo",
        &other_id,
    );

    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Busy Agent",
        &cookie,
        &csrf,
    )
    .await;
    sqlx::query(
        r#"
        INSERT INTO agent_runs (id, ticket_id, agent_id, job_type, status, sandbox_profile_id)
        VALUES ($1, $2, $3, 'implementation', 'running', 'permissive')
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(ticket_id)
    .bind(uuid::Uuid::parse_str(&agent_id).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let body = serde_json::json!({
        "folderNames": [folder_name(&worktree), folder_name(&other_wt)]
    });
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/worktrees/remove",
            &body.to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    let results = body["results"].as_array().unwrap();
    let busy = results
        .iter()
        .find(|r| r["folderName"] == folder_name(&worktree))
        .unwrap();
    let ok = results
        .iter()
        .find(|r| r["folderName"] == folder_name(&other_wt))
        .unwrap();
    assert_eq!(busy["status"], "active_run");
    assert_eq!(ok["status"], "removed");
    assert!(worktree.exists());
    assert!(!other_wt.exists());
    assert!(branch_exists(&repo, ticket_id));
}

#[tokio::test]
async fn list_survives_a_queued_chat_run_with_null_ticket_id() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_git, _env, state, app, cookie, csrf, _ticket_id, _repo, _worktree) = setup().await;
    let pool = state.db.clone().unwrap();

    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Chat Agent",
        &cookie,
        &csrf,
    )
    .await;
    let owner_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let session_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO chat_sessions (id, owner_user_id, agent_id, status)
        VALUES ($1, $2, $3, 'active')
        "#,
    )
    .bind(session_id)
    .bind(owner_id)
    .bind(uuid::Uuid::parse_str(&agent_id).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, chat_session_id, agent_id, job_type, status, sandbox_profile_id
        )
        VALUES ($1, NULL, $2, $3, 'chat_turn', 'queued', 'permissive')
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(session_id)
    .bind(uuid::Uuid::parse_str(&agent_id).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/worktrees",
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    assert!(body["worktrees"].as_array().unwrap().len() >= 1);
}

#[tokio::test]
async fn rejects_path_traversal_in_remove_body() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_git, _env, _state, app, cookie, csrf, _ticket_id, _repo, _worktree) = setup().await;
    let body = serde_json::json!({
        "folderNames": ["../etc", "TICKET-aaaaaaaa-repo/../x", "/tmp/evil"]
    });
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/worktrees/remove",
            &body.to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r["status"] == "error"));
}
