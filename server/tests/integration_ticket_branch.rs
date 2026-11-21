mod common;

use std::process::Command;
use std::time::Duration;

use axum::http::StatusCode;
use coppice_server::services::worktree_service::{
    build_ticket_branch, legacy_ticket_branch, resolve_stored_branch,
};
use tower::ServiceExt;
use uuid::Uuid;

async fn create_titled_ticket(
    app: &axum::Router,
    board_id: &str,
    title: &str,
    cookie: &str,
    csrf: &str,
) -> String {
    let body = serde_json::json!({
        "title": title,
        "description": "details",
    });
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/boards/{board_id}/tickets"),
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let ticket: serde_json::Value = common::json_body(res).await;
    let id = ticket["id"].as_str().unwrap().to_string();
    common::skip_planning(app, &id, cookie, csrf).await;
    id
}

async fn post_run_agent(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> StatusCode {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/run-agent"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    res.status()
}

async fn poll_succeeded(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> serde_json::Value {
    common::poll_runs_until_count(
        app,
        ticket_id,
        cookie,
        csrf,
        "succeeded run",
        Duration::from_secs(15),
        |runs| {
            runs.iter()
                .any(|run| run["status"].as_str() == Some("succeeded"))
        },
    )
    .await
    .into_iter()
    .find(|run| run["status"].as_str() == Some("succeeded"))
    .expect("succeeded run")
}

fn branch_exists(repo: &std::path::Path, branch: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")])
        .current_dir(repo)
        .output()
        .expect("git rev-parse")
        .status
        .success()
}

#[tokio::test]
async fn first_implementation_run_persists_title_slug_branch() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id =
        create_titled_ticket(&app, &board_id, "Fix login redirect", &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;

    let before = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert!(before["branchName"].is_null());

    assert_eq!(
        post_run_agent(&app, &ticket_id, &cookie, &csrf).await,
        StatusCode::CREATED
    );
    let run = poll_succeeded(&app, &ticket_id, &cookie, &csrf).await;

    let uuid: Uuid = ticket_id.parse().unwrap();
    let expected = build_ticket_branch(uuid, "Fix login redirect");
    assert_eq!(expected, format!("agent/{}-fix-login-redirect", &ticket_id[..8]));
    assert_eq!(run["branchName"].as_str(), Some(expected.as_str()));

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["branchName"].as_str(), Some(expected.as_str()));
    assert!(branch_exists(&local_path, &expected));
}

#[tokio::test]
async fn planning_run_leaves_branch_name_null() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &pm_id, &cookie, &csrf).await;

    let plan = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/plan-runs"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(plan.status(), StatusCode::OK);

    common::poll_ticket_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "plan review",
        Duration::from_secs(15),
        |ticket| ticket["status"] == "plan_review",
    )
    .await;

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert!(ticket["branchName"].is_null());
    let uuid: Uuid = ticket_id.parse().unwrap();
    assert!(!branch_exists(&local_path, &legacy_ticket_branch(uuid)));
    assert!(!branch_exists(
        &local_path,
        &build_ticket_branch(uuid, ticket["title"].as_str().unwrap_or(""))
    ));
}

#[tokio::test]
async fn title_edit_does_not_rename_stored_branch() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id =
        create_titled_ticket(&app, &board_id, "Fix login redirect", &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;

    assert_eq!(
        post_run_agent(&app, &ticket_id, &cookie, &csrf).await,
        StatusCode::CREATED
    );
    poll_succeeded(&app, &ticket_id, &cookie, &csrf).await;

    let uuid: Uuid = ticket_id.parse().unwrap();
    let original = build_ticket_branch(uuid, "Fix login redirect");

    let rename = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            r#"{"title":"Totally different title"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(rename.status(), StatusCode::OK);

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["title"], "Totally different title");
    assert_eq!(ticket["branchName"].as_str(), Some(original.as_str()));

    // Git helpers still resolve the original branch.
    let git = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/git-info"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(git.status(), StatusCode::OK);
    let git_body: serde_json::Value = common::json_body(git).await;
    assert_eq!(git_body["ticketBranch"].as_str(), Some(original.as_str()));
    assert!(branch_exists(&local_path, &original));
}

#[tokio::test]
async fn legacy_null_branch_name_still_resolves() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
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
    assert!(worktree.exists());

    let uuid: Uuid = ticket_id.parse().unwrap();
    let legacy = legacy_ticket_branch(uuid);
    assert_eq!(resolve_stored_branch(uuid, None), legacy);

    let git = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/git-info"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(git.status(), StatusCode::OK);
    let git_body: serde_json::Value = common::json_body(git).await;
    assert_eq!(git_body["ticketBranch"].as_str(), Some(legacy.as_str()));
}

#[tokio::test]
async fn patch_branch_name_rejected_after_worktree_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id =
        create_titled_ticket(&app, &board_id, "Fix login redirect", &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;

    // Before any run, a valid custom name is still allowed.
    let set = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            r#"{"branchName":"agent/custom-before-run"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(set.status(), StatusCode::OK);

    assert_eq!(
        post_run_agent(&app, &ticket_id, &cookie, &csrf).await,
        StatusCode::CREATED
    );
    poll_succeeded(&app, &ticket_id, &cookie, &csrf).await;

    let locked = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            r#"{"branchName":"agent/should-fail"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(locked.status(), StatusCode::CONFLICT);

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(
        ticket["branchName"].as_str(),
        Some("agent/custom-before-run")
    );
}
