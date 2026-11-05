mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

async fn setup_repo_ticket_and_worktree(
) -> (
    tempfile::TempDir,
    common::AgentTestEnv,
    axum::Router,
    String,
    String,
    String,
    String,
    String,
    String,
) {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    assert_eq!(
        state.config.agent.worktrees_path,
        env.worktrees.path().to_string_lossy()
    );
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let worktree_path = common::setup_worktree_with_commit(
        &local_path,
        env.worktrees.path(),
        "test-repo",
        &ticket_id,
    );
    (
        git_dir,
        env,
        app,
        cookie,
        csrf,
        repo_id,
        ticket_id,
        worktree_path.display().to_string(),
        board_id,
    )
}

async fn fetch_diff_head_sha(app: &axum::Router, repo_id: &str, worktree_path: &str, cookie: &str, csrf: &str) -> String {
    let encoded_path = percent_encode(worktree_path);
    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!(
                "/api/repos/{repo_id}/diff?worktreePath={encoded_path}&baseBranch=main"
            ),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body["headSha"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn list_worktrees_and_diff_for_repo() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, _env, app, cookie, csrf, repo_id, ticket_id, worktree_path, _board_id) =
        setup_repo_ticket_and_worktree().await;

    let worktrees_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{repo_id}/worktrees"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(worktrees_res.status(), StatusCode::OK);
    let worktrees_body: serde_json::Value = common::json_body(worktrees_res).await;
    let worktrees = worktrees_body["worktrees"].as_array().unwrap();
    assert_eq!(worktrees.len(), 1);
    assert_eq!(worktrees[0]["path"].as_str().unwrap(), worktree_path);
    assert_eq!(worktrees[0]["ticketId"].as_str().unwrap(), ticket_id);
    assert!(!worktrees[0]["headSha"].as_str().unwrap().is_empty());

    let encoded_path = percent_encode(&worktree_path);
    let diff_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!(
                "/api/repos/{repo_id}/diff?worktreePath={encoded_path}&baseBranch=main"
            ),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(diff_res.status(), StatusCode::OK);
    let diff: serde_json::Value = common::json_body(diff_res).await;
    assert_eq!(diff["baseBranch"], "main");
    assert!(!diff["baseSha"].as_str().unwrap().is_empty());
    assert!(!diff["headSha"].as_str().unwrap().is_empty());
    let files = diff["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "feature.txt");
    assert_eq!(files[0]["status"], "added");

    let file_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!(
                "/api/repos/{repo_id}/diff/file?worktreePath={encoded_path}&baseBranch=main&path=feature.txt"
            ),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(file_res.status(), StatusCode::OK);
    let patch: serde_json::Value = common::json_body(file_res).await;
    assert_eq!(patch["path"], "feature.txt");
    assert!(patch["patch"].as_str().unwrap().contains("new feature"));
}

#[tokio::test]
async fn submit_review_on_existing_ticket() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, _env, app, cookie, csrf, repo_id, ticket_id, worktree_path, _board_id) =
        setup_repo_ticket_and_worktree().await;
    let head_sha = fetch_diff_head_sha(&app, &repo_id, &worktree_path, &cookie, &csrf).await;

    let submit_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/code-reviews/submit",
            &serde_json::json!({
                "repoId": repo_id,
                "worktreePath": worktree_path,
                "baseBranch": "main",
                "headSha": head_sha,
                "ticketId": ticket_id,
                "summary": "Looks good overall.",
                "inlineComments": [{
                    "path": "feature.txt",
                    "line": 1,
                    "side": "new",
                    "body": "Nice addition"
                }]
            })
            .to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(submit_res.status(), StatusCode::CREATED);

    let submit_body: serde_json::Value = common::json_body(submit_res).await;
    assert_eq!(submit_body["ticketId"].as_str().unwrap(), ticket_id);
    assert_eq!(submit_body["ticketCreated"], false);
    assert!(!submit_body["commentId"].as_str().unwrap().is_empty());

    let comments_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/comments"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(comments_res.status(), StatusCode::OK);
    let comments: serde_json::Value = common::json_body(comments_res).await;
    assert_eq!(comments.as_array().unwrap().len(), 1);
    assert_eq!(comments[0]["intent"], "review_feedback");
    assert!(comments[0]["body"]
        .as_str()
        .unwrap()
        .contains("Looks good overall."));
    assert!(comments[0]["body"]
        .as_str()
        .unwrap()
        .contains("Nice addition"));
}

#[tokio::test]
async fn submit_review_creates_ticket_when_missing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, _env, app, cookie, csrf, repo_id, _ticket_id, worktree_path, board_id) =
        setup_repo_ticket_and_worktree().await;
    let head_sha = fetch_diff_head_sha(&app, &repo_id, &worktree_path, &cookie, &csrf).await;

    let submit_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/code-reviews/submit",
            &serde_json::json!({
                "repoId": repo_id,
                "worktreePath": worktree_path,
                "baseBranch": "main",
                "headSha": head_sha,
                "newTicket": {
                    "boardId": board_id,
                    "title": "Review follow-up",
                    "description": "Created from code review"
                },
                "summary": "Needs changes before merge.",
                "inlineComments": []
            })
            .to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(submit_res.status(), StatusCode::CREATED);

    let submit_body: serde_json::Value = common::json_body(submit_res).await;
    assert_eq!(submit_body["ticketCreated"], true);
    let new_ticket_id = submit_body["ticketId"].as_str().unwrap().to_string();

    let ticket = common::get_ticket(&app, &new_ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["title"], "Review follow-up");
    assert_eq!(ticket["repoId"].as_str().unwrap(), repo_id);
}

#[tokio::test]
async fn submit_review_move_to_in_progress() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, _env, app, cookie, csrf, repo_id, ticket_id, worktree_path, _board_id) =
        setup_repo_ticket_and_worktree().await;
    let head_sha = fetch_diff_head_sha(&app, &repo_id, &worktree_path, &cookie, &csrf).await;
    common::skip_planning(&app, &ticket_id, &cookie, &csrf).await;
    let engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let before = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(before["status"], "backlog");

    let submit_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/code-reviews/submit",
            &serde_json::json!({
                "repoId": repo_id,
                "worktreePath": worktree_path,
                "baseBranch": "main",
                "headSha": head_sha,
                "ticketId": ticket_id,
                "summary": "Approved with minor notes.",
                "inlineComments": [],
                "workflowAction": "move_to_in_progress",
                "reassignAgentId": engineer_id
            })
            .to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(submit_res.status(), StatusCode::CREATED);

    let after = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(after["status"], "in_progress");
    assert_eq!(after["assigneeAgentId"], engineer_id);
}

struct QuietReview {
    _git_dir: tempfile::TempDir,
    _worktrees: tempfile::TempDir,
    app: axum::Router,
    cookie: String,
    csrf: String,
    repo_id: String,
    ticket_id: String,
    worktree_path: String,
}

async fn setup_quiet_review(auto_start: bool) -> QuietReview {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let worktrees = tempfile::tempdir().expect("worktrees");
    let worktrees_path = worktrees.path().to_string_lossy().into_owned();
    let (_state, app, cookie, csrf) =
        common::bootstrap_and_login_with_state_config(move |config| {
            config.workflow.auto_start_runs = auto_start;
            config.agent.worktrees_path = worktrees_path;
        })
        .await;
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let worktree_path =
        common::setup_worktree_with_commit(&local_path, worktrees.path(), "test-repo", &ticket_id);
    QuietReview {
        _git_dir: git_dir,
        _worktrees: worktrees,
        app,
        cookie,
        csrf,
        repo_id,
        ticket_id,
        worktree_path: worktree_path.display().to_string(),
    }
}

async fn patch_ticket_status(fx: &QuietReview, body: &str) {
    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{}/status", fx.ticket_id),
            body,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    let status = res.status();
    let payload: serde_json::Value = common::json_body(res).await;
    assert_eq!(status, StatusCode::OK, "{payload}");
}

async fn post_review(
    fx: &QuietReview,
    head_sha: &str,
    extra: serde_json::Value,
) -> axum::response::Response {
    let mut body = serde_json::json!({
        "repoId": fx.repo_id,
        "worktreePath": fx.worktree_path,
        "baseBranch": "main",
        "headSha": head_sha,
        "ticketId": fx.ticket_id,
        "summary": "Send this back.",
        "inlineComments": []
    });
    for (key, value) in extra.as_object().unwrap() {
        body[key] = value.clone();
    }
    fx.app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/code-reviews/submit",
            &body.to_string(),
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap()
}

async fn ticket_runs(fx: &QuietReview) -> Vec<serde_json::Value> {
    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{}/runs", fx.ticket_id),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body["runs"].as_array().unwrap().clone()
}

#[tokio::test]
async fn move_to_in_progress_clears_waiting_and_assigns_engineer() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_quiet_review(false).await;
    let head_sha =
        fetch_diff_head_sha(&fx.app, &fx.repo_id, &fx.worktree_path, &fx.cookie, &fx.csrf).await;
    common::skip_planning(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;
    let engineer_id = common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Ada",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    patch_ticket_status(
        &fx,
        &format!(
            r#"{{"status":"wait_for_final_review","substatus":"waiting_for_agent","substatusMetadata":{{"agentId":"{engineer_id}"}}}}"#
        ),
    )
    .await;
    let before = common::get_ticket(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;
    assert_eq!(before["status"], "wait_for_final_review");
    assert_eq!(before["substatus"], "waiting_for_agent");
    assert!(before["assigneeAgentId"].is_null());

    let res = post_review(
        &fx,
        &head_sha,
        serde_json::json!({
            "workflowAction": "move_to_in_progress",
            "reassignAgentId": engineer_id
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let after = common::get_ticket(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;
    assert_eq!(after["status"], "in_progress");
    assert_eq!(after["assigneeAgentId"], engineer_id);
    assert!(after["substatus"].is_null());
    assert!(after["substatusMetadata"].is_null());
}

#[tokio::test]
async fn move_to_in_progress_starts_a_work_run_when_auto_start_is_on() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_quiet_review(true).await;
    let head_sha =
        fetch_diff_head_sha(&fx.app, &fx.repo_id, &fx.worktree_path, &fx.cookie, &fx.csrf).await;
    common::skip_planning(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;
    let engineer_id = common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Ada",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    patch_ticket_status(
        &fx,
        &format!(
            r#"{{"status":"wait_for_final_review","substatus":"waiting_for_agent","substatusMetadata":{{"agentId":"{engineer_id}"}}}}"#
        ),
    )
    .await;

    let res = post_review(
        &fx,
        &head_sha,
        serde_json::json!({
            "workflowAction": "move_to_in_progress",
            "reassignAgentId": engineer_id
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let runs = ticket_runs(&fx).await;
    assert!(
        runs.iter().any(|run| {
            run["jobType"] == "work_on_ticket"
                && run["agentId"] == engineer_id
                && matches!(run["status"].as_str(), Some("queued" | "running"))
        }),
        "{runs:?}"
    );
}

#[tokio::test]
async fn move_to_in_progress_without_engineer_leaves_the_ticket_unchanged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_quiet_review(true).await;
    let head_sha =
        fetch_diff_head_sha(&fx.app, &fx.repo_id, &fx.worktree_path, &fx.cookie, &fx.csrf).await;
    let engineer_id = common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Ada",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    patch_ticket_status(
        &fx,
        &format!(
            r#"{{"status":"wait_for_final_review","substatus":"waiting_for_agent","substatusMetadata":{{"agentId":"{engineer_id}"}}}}"#
        ),
    )
    .await;
    let before = common::get_ticket(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;

    let res = post_review(
        &fx,
        &head_sha,
        serde_json::json!({ "workflowAction": "move_to_in_progress" }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(
        body["error"],
        "Pick an engineer before moving this ticket to In Progress."
    );

    let after = common::get_ticket(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;
    assert_eq!(after["status"], before["status"]);
    assert_eq!(after["substatus"], before["substatus"]);
    assert_eq!(after["assigneeAgentId"], before["assigneeAgentId"]);
    assert!(ticket_runs(&fx).await.is_empty());
}

#[tokio::test]
async fn reassign_starts_a_run_only_outside_review_columns() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_quiet_review(true).await;
    let head_sha =
        fetch_diff_head_sha(&fx.app, &fx.repo_id, &fx.worktree_path, &fx.cookie, &fx.csrf).await;
    common::skip_planning(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;
    let engineer_id = common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Ada",
        &fx.cookie,
        &fx.csrf,
    )
    .await;

    patch_ticket_status(&fx, r#"{"status":"in_progress"}"#).await;
    let res = post_review(
        &fx,
        &head_sha,
        serde_json::json!({
            "workflowAction": "reassign_engineer",
            "reassignAgentId": engineer_id
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let after = common::get_ticket(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;
    assert_eq!(after["status"], "in_progress");
    assert_eq!(after["assigneeAgentId"], engineer_id);
    let runs = ticket_runs(&fx).await;
    assert!(
        runs.iter().any(|run| {
            run["jobType"] == "work_on_ticket"
                && run["agentId"] == engineer_id
                && matches!(run["status"].as_str(), Some("queued" | "running"))
        }),
        "{runs:?}"
    );

    patch_ticket_status(
        &fx,
        &format!(
            r#"{{"status":"wait_for_final_review","substatus":"waiting_for_agent","substatusMetadata":{{"agentId":"{engineer_id}"}}}}"#
        ),
    )
    .await;
    let other_id = common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Ben",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let runs_before = ticket_runs(&fx).await.len();
    let res = post_review(
        &fx,
        &head_sha,
        serde_json::json!({
            "summary": "Reassign while it waits for review.",
            "workflowAction": "reassign_engineer",
            "reassignAgentId": other_id
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let waiting = common::get_ticket(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;
    assert_eq!(waiting["status"], "wait_for_final_review");
    assert_eq!(waiting["assigneeAgentId"], other_id);
    let runs_after = ticket_runs(&fx).await;
    assert_eq!(runs_after.len(), runs_before);
    assert!(runs_after.iter().all(|run| run["agentId"] != other_id));
}

#[tokio::test]
async fn move_to_in_progress_returns_the_plan_gate_message() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_quiet_review(true).await;
    let head_sha =
        fetch_diff_head_sha(&fx.app, &fx.repo_id, &fx.worktree_path, &fx.cookie, &fx.csrf).await;
    let engineer_id = common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Ada",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let before = common::get_ticket(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;
    assert_eq!(before["status"], "backlog");

    let res = post_review(
        &fx,
        &head_sha,
        serde_json::json!({
            "workflowAction": "move_to_in_progress",
            "reassignAgentId": engineer_id
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(body["error"], coppice_server::copy::plan::PLAN_REQUIRED);

    let after = common::get_ticket(&fx.app, &fx.ticket_id, &fx.cookie, &fx.csrf).await;
    assert_eq!(after["status"], "backlog");
    assert!(after["substatus"].is_null());
    assert!(after["assigneeAgentId"].is_null());
    assert!(ticket_runs(&fx).await.is_empty());
}

#[tokio::test]
async fn reject_invalid_worktree_path() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;

    let invalid_path = local_path.display().to_string();
    let submit_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/code-reviews/submit",
            &serde_json::json!({
                "repoId": repo_id,
                "worktreePath": invalid_path,
                "baseBranch": "main",
                "headSha": "abc1234",
                "ticketId": ticket_id,
                "summary": "Should fail.",
                "inlineComments": []
            })
            .to_string(),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(submit_res.status(), StatusCode::BAD_REQUEST);
}
