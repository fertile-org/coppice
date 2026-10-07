mod common;

use axum::http::StatusCode;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tower::ServiceExt;

fn git_env_commit(dir: &Path, message: &str) {
    let output = Command::new("git")
        .args(["commit", "-m", message])
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .current_dir(dir)
        .output()
        .expect("git commit");
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_rev_parse(dir: &Path, rev: &str) -> String {
    let output = Command::new("git")
        .args(["rev-parse", rev])
        .current_dir(dir)
        .output()
        .expect("git rev-parse");
    assert!(output.status.success(), "rev-parse {rev} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn diverge_readme(local_path: &Path, worktree: &Path) {
    std::fs::write(local_path.join("README.md"), "# test\nmain side\n").expect("write main");
    let add = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(local_path)
        .output()
        .expect("git add");
    assert!(add.status.success());
    git_env_commit(local_path, "main conflict");

    std::fs::write(worktree.join("README.md"), "# test\nfeature side\n").expect("write wt");
    let add = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(worktree)
        .output()
        .expect("git add");
    assert!(add.status.success());
    git_env_commit(worktree, "feature conflict");
}

struct TicketGit {
    _git_keep: tempfile::TempDir,
    _worktrees: tempfile::TempDir,
    state: std::sync::Arc<coppice_server::AppState>,
    app: axum::Router,
    cookie: String,
    csrf: String,
    ticket_id: String,
    local_path: PathBuf,
    worktree_path: PathBuf,
}

/// No job workers. A resolve request stays queued, so a second request is still
/// looking at an active run.
async fn setup_ticket() -> TicketGit {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let worktrees = tempfile::tempdir().expect("worktrees");
    let path = worktrees.path().to_string_lossy().into_owned();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(move |config| {
        config.agent.worktrees_path = path;
    })
    .await;
    finish_ticket(git_dir, local_path, worktrees, state, app, cookie, csrf).await
}

/// Workers plus the mock `done` fixture. The returned tempdir is the one they use.
async fn setup_with_workers() -> TicketGit {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_state_and_workers("done", |_| {}).await;
    finish_ticket(git_dir, local_path, env.worktrees, state, app, cookie, csrf).await
}

async fn finish_ticket(
    git_dir: tempfile::TempDir,
    local_path: PathBuf,
    worktrees: tempfile::TempDir,
    state: std::sync::Arc<coppice_server::AppState>,
    app: axum::Router,
    cookie: String,
    csrf: String,
) -> TicketGit {
    let worktrees_root = PathBuf::from(&state.config.agent.worktrees_path);
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let worktree_path =
        common::setup_worktree_with_commit(&local_path, &worktrees_root, "test-repo", &ticket_id);
    TicketGit {
        _git_keep: git_dir,
        _worktrees: worktrees,
        state,
        app,
        cookie,
        csrf,
        ticket_id,
        local_path,
        worktree_path,
    }
}

async fn assign_ada(fx: &TicketGit) -> String {
    let agent_id = common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Ada",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    common::assign_agent_to_ticket(&fx.app, &fx.ticket_id, &agent_id, &fx.cookie, &fx.csrf).await;
    agent_id
}

async fn rebase_conflict(fx: &TicketGit) -> serde_json::Value {
    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/rebase-branch", fx.ticket_id),
            r#"{"baseBranch":"main"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    common::json_body(res).await
}

async fn list_runs(fx: &TicketGit) -> Vec<serde_json::Value> {
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
    body["runs"].as_array().cloned().unwrap_or_default()
}

async fn post_resolve(fx: &TicketGit) -> axum::response::Response {
    fx.app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/resolve-conflict", fx.ticket_id),
            r#"{"baseBranch":"main","files":["README.md"]}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap()
}

async fn ticket_json(fx: &TicketGit) -> serde_json::Value {
    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{}", fx.ticket_id),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    common::json_body(res).await
}

fn prompt_for_readme() -> String {
    coppice_server::copy::conflict::resolve_prompt("main", &["README.md".into()])
}

#[tokio::test]
async fn resolve_conflict_starts_one_run_and_ignores_a_second_request() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let agent_id = assign_ada(&fx).await;
    diverge_readme(&fx.local_path, &fx.worktree_path);
    let conflict = rebase_conflict(&fx).await;

    assert_eq!(
        conflict["message"],
        coppice_server::copy::conflict::conflict_message(
            coppice_server::copy::conflict::REBASE_ACTION,
            "main",
            &["README.md".into()],
        )
    );
    assert_eq!(conflict["conflict"]["canAskAssignee"], true);
    assert_eq!(
        conflict["conflict"]["askLabel"],
        coppice_server::copy::conflict::ask_to_resolve_label("Ada")
    );
    assert_eq!(
        conflict["conflict"]["rereviewNote"],
        coppice_server::copy::conflict::REREVIEW_NOTE
    );
    assert!(conflict["conflict"]["unavailableReason"].is_null());
    assert!(
        list_runs(&fx).await.is_empty(),
        "a conflict does not start a run"
    );

    let first = post_resolve(&fx).await;
    assert_eq!(first.status(), StatusCode::OK);
    let first: serde_json::Value = common::json_body(first).await;
    let run_id = first["run"]["id"].as_str().unwrap().to_string();
    assert_eq!(first["run"]["agentId"], agent_id);
    assert_eq!(first["run"]["jobType"], "work_on_ticket");
    assert_eq!(first["run"]["connector"], "mock");
    assert!(
        matches!(first["run"]["status"].as_str(), Some("queued" | "running")),
        "resolve must stay on the normal run path, got {}",
        first["run"]["status"]
    );

    let pool = fx.state.db.clone().expect("pool");
    let parsed = uuid::Uuid::parse_str(&run_id).unwrap();
    let row: (String, String, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT job_type, context_profile, trigger_comment_id FROM agent_runs WHERE id = $1",
    )
    .bind(parsed)
    .fetch_one(&pool)
    .await
    .expect("run row");
    assert_eq!(row.0, "work_on_ticket");
    assert_eq!(row.1, "human_agent");
    let trigger_id = row.2.expect("trigger comment");

    let comments = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{}/comments", fx.ticket_id),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    let comments: serde_json::Value = common::json_body(comments).await;
    let prompt = prompt_for_readme();
    let trigger = comments
        .as_array()
        .unwrap()
        .iter()
        .find(|comment| comment["id"].as_str() == Some(&trigger_id.to_string()))
        .expect("trigger comment in the thread");
    assert_eq!(trigger["body"], prompt);
    assert_eq!(trigger["authorType"], "system");

    let second = post_resolve(&fx).await;
    assert_eq!(second.status(), StatusCode::OK);
    let second: serde_json::Value = common::json_body(second).await;
    assert_eq!(second["run"]["id"], run_id);

    let runs = list_runs(&fx).await;
    assert_eq!(runs.len(), 1, "a second request must not start another run");
    assert_eq!(runs[0]["id"], run_id);
}

#[tokio::test]
async fn resolve_conflict_is_refused_without_an_assignee() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    diverge_readme(&fx.local_path, &fx.worktree_path);
    let conflict = rebase_conflict(&fx).await;
    assert_eq!(conflict["conflict"]["canAskAssignee"], false);
    assert_eq!(
        conflict["conflict"]["unavailableReason"],
        coppice_server::copy::conflict::NO_ASSIGNEE
    );
    assert!(conflict["conflict"]["askLabel"].is_null());
    assert!(list_runs(&fx).await.is_empty());

    let res = post_resolve(&fx).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(body["message"], coppice_server::copy::conflict::NO_ASSIGNEE);
    assert!(list_runs(&fx).await.is_empty());
}

#[tokio::test]
async fn resolve_conflict_is_refused_when_connector_is_not_ready() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let agent_id = assign_ada(&fx).await;
    let pool = fx.state.db.clone().expect("pool");
    sqlx::query("UPDATE agents SET connector = 'cursor' WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&agent_id).unwrap())
        .execute(&pool)
        .await
        .expect("switch connector");

    diverge_readme(&fx.local_path, &fx.worktree_path);
    let conflict = rebase_conflict(&fx).await;
    assert_eq!(conflict["conflict"]["canAskAssignee"], false);
    assert_eq!(
        conflict["conflict"]["unavailableReason"],
        coppice_server::copy::conflict::connector_not_ready_reason("Ada")
    );
    assert!(conflict["conflict"]["askLabel"].is_null());

    let res = post_resolve(&fx).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(
        body["message"],
        coppice_server::copy::conflict::connector_not_ready_reason("Ada")
    );
    assert!(list_runs(&fx).await.is_empty());
}

#[tokio::test]
async fn successful_resolve_run_stales_acceptance_and_returns_to_in_review() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_with_workers().await;
    assign_ada(&fx).await;
    diverge_readme(&fx.local_path, &fx.worktree_path);

    let status = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{}/status", fx.ticket_id),
            r#"{"status":"wait_for_final_review"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let approve = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/final-approve", fx.ticket_id),
            "{}",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK, "final approve");
    let approved_sha = git_rev_parse(&fx.worktree_path, "HEAD");

    let conflict = rebase_conflict(&fx).await;
    assert_eq!(conflict["conflict"]["operation"], "rebase");
    assert_eq!(git_rev_parse(&fx.worktree_path, "HEAD"), approved_sha);
    let before = ticket_json(&fx).await;
    assert_eq!(before["status"], "done");
    assert_eq!(before["humanReview"]["stale"], false);
    assert_eq!(before["humanReview"]["headSha"], approved_sha);
    assert!(
        list_runs(&fx).await.is_empty(),
        "aborting the conflict does not start a run"
    );

    std::fs::write(fx.worktree_path.join("resolved.txt"), "resolved\n").expect("dirty file");

    let started = post_resolve(&fx).await;
    assert_eq!(started.status(), StatusCode::OK);
    common::json_body(started).await;

    let (run, ticket) = wait_until_back_in_review(&fx).await;
    assert_eq!(run["status"], "succeeded", "error: {}", run["errorMessage"]);
    assert_eq!(run["jobType"], "work_on_ticket");
    assert_eq!(list_runs(&fx).await.len(), 1);
    assert_ne!(git_rev_parse(&fx.worktree_path, "HEAD"), approved_sha);
    assert_eq!(ticket["status"], "in_review");
    assert_eq!(ticket["humanReview"]["stale"], true);
    assert_eq!(ticket["humanReview"]["headSha"], approved_sha);

    let context = std::fs::read_to_string(fx.worktree_path.join(".agent").join("context.md"))
        .expect("context.md");
    let prompt = prompt_for_readme();
    assert!(
        context.contains(prompt.trim()),
        "context.md should carry the resolve prompt:\n{context}"
    );
}

async fn wait_until_back_in_review(fx: &TicketGit) -> (serde_json::Value, serde_json::Value) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last_run;
    let mut last_ticket;
    loop {
        let runs = list_runs(fx).await;
        last_run = runs.first().cloned().unwrap_or(serde_json::Value::Null);
        last_ticket = ticket_json(fx).await;
        let status = last_run["status"].as_str().unwrap_or("");
        if status == "failed" || status == "blocked" || status == "cancelled" {
            panic!(
                "resolve run ended as {status}: {}",
                last_run["errorMessage"]
            );
        }
        if status == "succeeded"
            && last_ticket["status"] == "in_review"
            && last_ticket["humanReview"]["stale"] == true
        {
            return (last_run, last_ticket);
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for in-review: run={last_run} ticket={last_ticket}");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
