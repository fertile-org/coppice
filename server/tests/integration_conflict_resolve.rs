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
    post_resolve_with(fx, None, None).await
}

async fn post_resolve_with(
    fx: &TicketGit,
    operation: Option<&str>,
    agent_id: Option<&str>,
) -> axum::response::Response {
    let mut body = serde_json::json!({
        "baseBranch": "main",
        "files": ["README.md"],
    });
    if let Some(operation) = operation {
        body["operation"] = serde_json::json!(operation);
    }
    if let Some(agent_id) = agent_id {
        body["agentId"] = serde_json::json!(agent_id);
    }
    fx.app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/resolve-conflict", fx.ticket_id),
            &body.to_string(),
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap()
}

async fn insert_run(
    fx: &TicketGit,
    agent_id: &str,
    job_type: &str,
    context_profile: &str,
    created_offset_secs: i64,
) {
    let pool = fx.state.db.clone().expect("pool");
    let created = time::OffsetDateTime::now_utc() + time::Duration::seconds(created_offset_secs);
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
            context_profile, created_at, started_at, ended_at
        )
        VALUES (
            $1, $2, $3, $4, 'succeeded', 'permissive-default', $5, $6, $6, $6
        )
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(uuid::Uuid::parse_str(&fx.ticket_id).unwrap())
    .bind(uuid::Uuid::parse_str(agent_id).unwrap())
    .bind(job_type)
    .bind(context_profile)
    .bind(created)
    .execute(&pool)
    .await
    .expect("insert run");
}

fn offer_agent_ids(conflict: &serde_json::Value) -> Vec<String> {
    conflict["conflict"]["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|agent| agent["id"].as_str().map(str::to_string))
        .collect()
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
    assert_eq!(conflict["conflict"]["defaultAgentId"], agent_id);
    assert_eq!(
        conflict["conflict"]["agents"][0]["askLabel"],
        coppice_server::copy::conflict::ask_to_resolve_label("Ada")
    );
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
        coppice_server::copy::conflict::NO_ELIGIBLE_AGENT
    );
    assert!(conflict["conflict"]["askLabel"].is_null());
    assert!(conflict["conflict"]["defaultAgentId"].is_null());
    assert_eq!(conflict["conflict"]["agents"], serde_json::json!([]));
    assert!(list_runs(&fx).await.is_empty());

    let res = post_resolve(&fx).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(
        body["message"],
        coppice_server::copy::conflict::NO_ELIGIBLE_AGENT
    );
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
    assert_eq!(
        conflict["conflict"]["askLabel"],
        coppice_server::copy::conflict::ask_to_resolve_label("Ada")
    );
    assert_eq!(
        conflict["conflict"]["agents"][0]["unavailableReason"],
        coppice_server::copy::conflict::connector_not_ready_reason("Ada")
    );

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

async fn accept_to_done(fx: &TicketGit) -> serde_json::Value {
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
    common::json_body(approve).await
}

async fn comments(fx: &TicketGit) -> Vec<serde_json::Value> {
    let res = fx
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
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body.as_array().cloned().unwrap_or_default()
}

async fn merge_main(fx: &TicketGit) -> axum::response::Response {
    fx.app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/merge-branch", fx.ticket_id),
            r#"{"baseBranch":"main"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap()
}

async fn post_resolve_merge(fx: &TicketGit) -> axum::response::Response {
    post_resolve_with(fx, Some("merge"), None).await
}

fn reopen_comment_for_readme() -> String {
    coppice_server::copy::conflict::merge_conflict_reopen_comment(
        "main",
        &["README.md".into()],
        "Ada",
    )
}

/// Done, accepted, and the merge into main conflicts on README.md.
async fn done_with_merge_conflict(fx: &TicketGit) -> serde_json::Value {
    diverge_readme(&fx.local_path, &fx.worktree_path);
    let approved = accept_to_done(fx).await;
    assert_eq!(approved["status"], "done");
    let res = merge_main(fx).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    common::json_body(res).await
}

async fn assert_still_done(fx: &TicketGit) {
    let ticket = ticket_json(fx).await;
    assert_eq!(ticket["status"], "done");
    assert_eq!(ticket["humanReview"]["stale"], false);
    assert_eq!(ticket["humanReview"]["merged"], false);
    let expected = reopen_comment_for_readme();
    let thread = comments(fx).await;
    assert!(
        !thread.iter().any(|comment| comment["body"] == expected),
        "no reopen comment while the ticket stays Done: {thread:?}"
    );
    assert!(list_runs(fx).await.is_empty(), "no run was started");
}

#[tokio::test]
async fn merge_conflict_on_done_stays_done_until_asked() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let agent_id = assign_ada(&fx).await;
    diverge_readme(&fx.local_path, &fx.worktree_path);
    let approved = accept_to_done(&fx).await;
    let approved_sha = git_rev_parse(&fx.worktree_path, "HEAD");
    assert_eq!(approved["status"], "done");
    assert_eq!(approved["assigneeAgentId"], agent_id);
    assert_eq!(approved["humanReview"]["headSha"], approved_sha);
    assert_eq!(approved["humanReview"]["stale"], false);
    let pool = fx.state.db.clone().expect("pool");
    let ticket_uuid = uuid::Uuid::parse_str(&fx.ticket_id).unwrap();
    let accepted_at_before: time::OffsetDateTime =
        sqlx::query_scalar("SELECT accepted_at FROM ticket_human_reviews WHERE ticket_id = $1")
            .bind(ticket_uuid)
            .fetch_one(&pool)
            .await
            .expect("acceptance row");
    let main_before = git_rev_parse(&fx.local_path, "HEAD");

    let res = merge_main(&fx).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(body["conflict"]["files"], serde_json::json!(["README.md"]));
    assert_eq!(body["conflict"]["operation"], "merge");
    assert_eq!(body["conflict"]["canAskAssignee"], true);
    assert_eq!(git_rev_parse(&fx.local_path, "HEAD"), main_before);
    assert_still_done(&fx).await;

    // Closing the dialog and merging again retries and shows the same offer.
    let again = merge_main(&fx).await;
    assert_eq!(again.status(), StatusCode::BAD_REQUEST);
    let again: serde_json::Value = common::json_body(again).await;
    assert_eq!(again["conflict"]["operation"], "merge");
    assert_eq!(again["conflict"]["canAskAssignee"], true);
    assert_eq!(git_rev_parse(&fx.local_path, "HEAD"), main_before);
    assert_still_done(&fx).await;

    let asked = post_resolve_merge(&fx).await;
    assert_eq!(asked.status(), StatusCode::OK);
    let asked: serde_json::Value = common::json_body(asked).await;
    let run_id = asked["run"]["id"].as_str().unwrap().to_string();
    assert_eq!(asked["run"]["agentId"], agent_id);
    assert_eq!(asked["run"]["jobType"], "work_on_ticket");

    let ticket = ticket_json(&fx).await;
    assert_eq!(ticket["status"], "in_progress");
    assert_eq!(ticket["assigneeAgentId"], agent_id);
    assert_eq!(ticket["humanReview"]["headSha"], approved_sha);
    assert_eq!(ticket["humanReview"]["stale"], true);
    assert_eq!(ticket["humanReview"]["merged"], false);
    let accepted_at_after: time::OffsetDateTime =
        sqlx::query_scalar("SELECT accepted_at FROM ticket_human_reviews WHERE ticket_id = $1")
            .bind(ticket_uuid)
            .fetch_one(&pool)
            .await
            .expect("acceptance row kept");
    assert_eq!(accepted_at_after, accepted_at_before);
    let exempt: bool = sqlx::query_scalar("SELECT review_cap_exempt FROM tickets WHERE id = $1")
        .bind(ticket_uuid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(exempt, "the reopened ticket may pass a full review cap");

    let thread = comments(&fx).await;
    assert!(
        thread.iter().any(|comment| comment["body"]
            .as_str()
            .unwrap_or("")
            .contains("Accepted: ticket moved to **Done**.")),
        "the earlier accept stays in the thread: {thread:?}"
    );
    let expected = reopen_comment_for_readme();
    let reopen = thread
        .iter()
        .find(|comment| comment["body"] == expected && comment["authorType"] == "system")
        .expect("conflict comment listing the files");
    let comment_id = reopen["id"].as_str().unwrap().to_string();

    let runs = list_runs(&fx).await;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["id"], run_id);
    let row: (String, Option<uuid::Uuid>) =
        sqlx::query_as("SELECT context_profile, trigger_comment_id FROM agent_runs WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&run_id).unwrap())
            .fetch_one(&pool)
            .await
            .expect("run row");
    assert_eq!(row.0, "full");
    assert_eq!(row.1.unwrap().to_string(), comment_id);

    let second = post_resolve_merge(&fx).await;
    assert_eq!(second.status(), StatusCode::OK);
    let second: serde_json::Value = common::json_body(second).await;
    assert_eq!(second["run"]["id"], run_id);
    assert_eq!(list_runs(&fx).await.len(), 1);

    let direct_done = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{}/status", fx.ticket_id),
            r#"{"status":"done"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(direct_done.status(), StatusCode::BAD_REQUEST);
    let early_accept = fx
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
    assert_eq!(early_accept.status(), StatusCode::BAD_REQUEST);

    let review = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{}/status", fx.ticket_id),
            r#"{"status":"in_review"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(review.status(), StatusCode::OK);
    let fresh = accept_to_done(&fx).await;
    assert_eq!(fresh["status"], "done");
    assert_eq!(fresh["humanReview"]["stale"], false);
    assert_eq!(fresh["humanReview"]["headSha"], approved_sha);
    let thread = comments(&fx).await;
    let accepts = thread
        .iter()
        .filter(|comment| {
            comment["body"]
                .as_str()
                .unwrap_or("")
                .contains("Accepted: ticket moved to **Done**.")
        })
        .count();
    assert_eq!(accepts, 2, "the earlier accept remains after the fresh one");
}

#[tokio::test]
async fn asking_on_a_done_merge_conflict_with_a_connector_not_ready_stays_done() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let agent_id = assign_ada(&fx).await;
    done_with_merge_conflict(&fx).await;
    let pool = fx.state.db.clone().expect("pool");
    sqlx::query("UPDATE agents SET connector = 'cursor' WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&agent_id).unwrap())
        .execute(&pool)
        .await
        .expect("switch connector");

    let res = post_resolve_merge(&fx).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(
        body["message"],
        coppice_server::copy::conflict::connector_not_ready_reason("Ada")
    );
    assert_still_done(&fx).await;
}

#[tokio::test]
async fn asking_on_a_done_merge_conflict_when_the_run_cannot_start_stays_done() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    assign_ada(&fx).await;
    done_with_merge_conflict(&fx).await;

    // The run start re-verifies the repository after the comment is posted.
    let moved = fx.local_path.with_extension("moved");
    std::fs::rename(&fx.local_path, &moved).expect("hide the checkout");
    let res = post_resolve_merge(&fx).await;
    std::fs::rename(&moved, &fx.local_path).expect("restore the checkout");
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("repository path is not ready"),
        "the run start must fail on the repository: {body}"
    );
    assert_still_done(&fx).await;
}

#[tokio::test]
async fn merge_conflict_run_context_lists_the_conflicting_files() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_with_workers().await;
    assign_ada(&fx).await;
    done_with_merge_conflict(&fx).await;
    assert!(
        list_runs(&fx).await.is_empty(),
        "the conflict alone starts nothing"
    );

    let asked = post_resolve_merge(&fx).await;
    assert_eq!(asked.status(), StatusCode::OK);

    let comment = reopen_comment_for_readme();
    let deadline = Instant::now() + Duration::from_secs(30);
    let context_path = fx.worktree_path.join(".agent").join("context.md");
    loop {
        if let Ok(context) = std::fs::read_to_string(&context_path) {
            assert!(
                context.contains(&comment),
                "context.md should carry the conflict comment:\n{context}"
            );
            assert!(
                context.contains("**Status:** in_progress"),
                "context.md should show In Progress:\n{context}"
            );
            break;
        }
        let runs = list_runs(&fx).await;
        let status = runs
            .first()
            .and_then(|run| run["status"].as_str())
            .unwrap_or("");
        if matches!(status, "failed" | "blocked" | "cancelled") {
            panic!(
                "conflict run ended as {status}: {}",
                runs[0]["errorMessage"]
            );
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for context.md; runs={runs:?}");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
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

#[tokio::test]
async fn unassigned_done_ticket_preselects_last_implementer() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let tech_lead_id = common::create_agent_with_preset_key(
        &fx.app,
        "tech_lead",
        "Technical Lead",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let frontend_id = common::create_agent_with_preset_key(
        &fx.app,
        "frontend_engineer",
        "Frontend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let qc_id =
        common::create_agent_with_preset_key(&fx.app, "qc", "QC Engineer", &fx.cookie, &fx.csrf)
            .await;
    insert_run(&fx, &frontend_id, "work_on_ticket", "full", -30).await;
    insert_run(&fx, &qc_id, "work_on_ticket", "full", -5).await;
    insert_run(&fx, &frontend_id, "respond_to_mention", "full", -1).await;
    insert_run(&fx, &tech_lead_id, "respond_to_mention", "human_agent", 0).await;

    let conflict = done_with_merge_conflict(&fx).await;
    assert_eq!(conflict["conflict"]["defaultAgentId"], frontend_id);
    assert_eq!(
        conflict["conflict"]["askLabel"],
        coppice_server::copy::conflict::ask_to_resolve_label("Frontend Engineer")
    );
    assert_eq!(conflict["conflict"]["canAskAssignee"], true);
    let ids = offer_agent_ids(&conflict);
    assert!(ids.contains(&frontend_id));
    assert!(ids.contains(&tech_lead_id));
    assert!(!ids.contains(&qc_id));

    let asked = post_resolve_merge(&fx).await;
    assert_eq!(asked.status(), StatusCode::OK);
    let asked: serde_json::Value = common::json_body(asked).await;
    assert_eq!(asked["run"]["agentId"], frontend_id);
    let ticket = ticket_json(&fx).await;
    assert_eq!(ticket["status"], "in_progress");
    assert_eq!(ticket["assigneeAgentId"], frontend_id);
    let pool = fx.state.db.clone().expect("pool");
    let row: (String, Option<uuid::Uuid>) =
        sqlx::query_as("SELECT context_profile, trigger_comment_id FROM agent_runs WHERE id = $1")
            .bind(uuid::Uuid::parse_str(asked["run"]["id"].as_str().unwrap()).unwrap())
            .fetch_one(&pool)
            .await
            .expect("run row");
    assert_eq!(row.0, "full");
    let expected = coppice_server::copy::conflict::merge_conflict_reopen_comment(
        "main",
        &["README.md".into()],
        "Frontend Engineer",
    );
    let thread = comments(&fx).await;
    assert!(thread.iter().any(|comment| comment["body"] == expected));
}

#[tokio::test]
async fn picking_another_agent_assigns_and_runs_that_agent() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let frontend_id = common::create_agent_with_preset_key(
        &fx.app,
        "frontend_engineer",
        "Frontend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let backend_id = common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Backend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    insert_run(&fx, &frontend_id, "work_on_ticket", "full", -10).await;
    let conflict = done_with_merge_conflict(&fx).await;
    assert_eq!(conflict["conflict"]["defaultAgentId"], frontend_id);

    let asked = post_resolve_with(&fx, Some("merge"), Some(&backend_id)).await;
    assert_eq!(asked.status(), StatusCode::OK);
    let asked: serde_json::Value = common::json_body(asked).await;
    assert_eq!(asked["run"]["agentId"], backend_id);
    let ticket = ticket_json(&fx).await;
    assert_eq!(ticket["status"], "in_progress");
    assert_eq!(ticket["assigneeAgentId"], backend_id);
}

#[tokio::test]
async fn no_implementer_run_preselects_tech_lead_and_success_reaches_in_review() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_with_workers().await;
    let tech_lead_id = common::create_agent_with_preset_key(
        &fx.app,
        "tech_lead",
        "Technical Lead",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let _frontend_id = common::create_agent_with_preset_key(
        &fx.app,
        "frontend_engineer",
        "Frontend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let conflict = done_with_merge_conflict(&fx).await;
    assert_eq!(conflict["conflict"]["defaultAgentId"], tech_lead_id);
    assert_eq!(
        conflict["conflict"]["askLabel"],
        coppice_server::copy::conflict::ask_to_resolve_label("Technical Lead")
    );

    let asked = post_resolve_merge(&fx).await;
    assert_eq!(asked.status(), StatusCode::OK);
    let (run, ticket) = wait_until_back_in_review(&fx).await;
    assert_eq!(run["status"], "succeeded", "error: {}", run["errorMessage"]);
    assert_eq!(run["agentId"], tech_lead_id);
    assert_eq!(ticket["status"], "in_review");
}

#[tokio::test]
async fn tech_lead_with_custom_role_resolve_reaches_in_review() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_with_workers().await;
    let tech_lead_id = common::create_agent_with_preset_key(
        &fx.app,
        "tech_lead",
        "Technical Lead",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let pool = fx.state.db.clone().expect("pool");
    sqlx::query("UPDATE agents SET name = 'Architect', role = 'Architect' WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&tech_lead_id).unwrap())
        .execute(&pool)
        .await
        .expect("rename tech lead role");

    let conflict = done_with_merge_conflict(&fx).await;
    assert_eq!(conflict["conflict"]["defaultAgentId"], tech_lead_id);
    assert_eq!(
        conflict["conflict"]["askLabel"],
        coppice_server::copy::conflict::ask_to_resolve_label("Architect")
    );

    let asked = post_resolve_merge(&fx).await;
    assert_eq!(asked.status(), StatusCode::OK);
    let (run, ticket) = wait_until_back_in_review(&fx).await;
    assert_eq!(run["status"], "succeeded", "error: {}", run["errorMessage"]);
    assert_eq!(run["agentId"], tech_lead_id);
    assert_eq!(ticket["status"], "in_review");
}

#[tokio::test]
async fn asking_a_not_ready_agent_leaves_the_ticket_unchanged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let frontend_id = common::create_agent_with_preset_key(
        &fx.app,
        "frontend_engineer",
        "Frontend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let backend_id = common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Backend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    insert_run(&fx, &frontend_id, "work_on_ticket", "full", -10).await;
    let pool = fx.state.db.clone().expect("pool");
    sqlx::query("UPDATE agents SET connector = 'cursor' WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&frontend_id).unwrap())
        .execute(&pool)
        .await
        .expect("switch connector");

    let conflict = done_with_merge_conflict(&fx).await;
    assert_eq!(conflict["conflict"]["defaultAgentId"], frontend_id);
    assert_eq!(conflict["conflict"]["canAskAssignee"], true);
    assert_eq!(
        conflict["conflict"]["unavailableReason"],
        coppice_server::copy::conflict::connector_not_ready_reason("Frontend Engineer")
    );
    let backend = conflict["conflict"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["id"] == backend_id)
        .expect("backend in picker");
    assert!(backend["unavailableReason"].is_null());

    let res = post_resolve_with(&fx, Some("merge"), Some(&frontend_id)).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(
        body["message"],
        coppice_server::copy::conflict::connector_not_ready_reason("Frontend Engineer")
    );
    let ticket = ticket_json(&fx).await;
    assert_eq!(ticket["status"], "done");
    assert!(ticket["assigneeAgentId"].is_null());
    assert!(
        list_runs(&fx)
            .await
            .iter()
            .all(|run| run["status"] == "succeeded"),
        "no resolve run was started"
    );
}

#[tokio::test]
async fn asking_while_another_agent_is_active_returns_conflict() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let frontend_id = common::create_agent_with_preset_key(
        &fx.app,
        "frontend_engineer",
        "Frontend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    let backend_id = common::create_agent_with_preset_key(
        &fx.app,
        "backend_engineer",
        "Backend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    done_with_merge_conflict(&fx).await;
    let first = post_resolve_with(&fx, Some("merge"), Some(&frontend_id)).await;
    assert_eq!(first.status(), StatusCode::OK);
    let first: serde_json::Value = common::json_body(first).await;
    let run_id = first["run"]["id"].as_str().unwrap().to_string();

    let second = post_resolve_with(&fx, Some("merge"), Some(&backend_id)).await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let body: serde_json::Value = common::json_body(second).await;
    assert_eq!(
        body["message"],
        coppice_server::copy::conflict::other_agent_active_reason("Frontend Engineer")
    );
    let runs = list_runs(&fx).await;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["id"], run_id);
}

#[tokio::test]
async fn failed_reopen_start_leaves_unassigned_done_ticket() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let frontend_id = common::create_agent_with_preset_key(
        &fx.app,
        "frontend_engineer",
        "Frontend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    insert_run(&fx, &frontend_id, "work_on_ticket", "full", -10).await;
    done_with_merge_conflict(&fx).await;

    let moved = fx.local_path.with_extension("moved");
    std::fs::rename(&fx.local_path, &moved).expect("hide the checkout");
    let res = post_resolve_merge(&fx).await;
    std::fs::rename(&moved, &fx.local_path).expect("restore the checkout");
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let ticket = ticket_json(&fx).await;
    assert_eq!(ticket["status"], "done");
    assert!(ticket["assigneeAgentId"].is_null());
    assert!(
        list_runs(&fx)
            .await
            .iter()
            .all(|run| run["status"] == "succeeded"),
        "no resolve run was started"
    );
}

#[tokio::test]
async fn assigning_an_agent_to_a_done_ticket_starts_no_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (git_dir, local_path) = common::create_temp_git_checkout();
    let worktrees = tempfile::tempdir().expect("worktrees");
    let path = worktrees.path().to_string_lossy().into_owned();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(move |config| {
        config.agent.worktrees_path = path;
        config.workflow.auto_start_runs = true;
    })
    .await;
    let fx = finish_ticket(git_dir, local_path, worktrees, state, app, cookie, csrf).await;
    let agent_id = common::create_agent_with_preset_key(
        &fx.app,
        "frontend_engineer",
        "Frontend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    accept_to_done(&fx).await;
    assert_eq!(ticket_json(&fx).await["status"], "done");
    assert!(list_runs(&fx).await.is_empty());

    common::assign_agent_to_ticket(&fx.app, &fx.ticket_id, &agent_id, &fx.cookie, &fx.csrf).await;
    assert_eq!(ticket_json(&fx).await["status"], "done");
    assert_eq!(ticket_json(&fx).await["assigneeAgentId"], agent_id);
    assert!(
        list_runs(&fx).await.is_empty(),
        "assigning on Done must not start a run"
    );
}

#[tokio::test]
async fn rebase_resolve_does_not_assign_the_picked_agent() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let frontend_id = common::create_agent_with_preset_key(
        &fx.app,
        "frontend_engineer",
        "Frontend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    diverge_readme(&fx.local_path, &fx.worktree_path);
    let conflict = rebase_conflict(&fx).await;
    assert_eq!(conflict["conflict"]["defaultAgentId"], frontend_id);

    let asked = post_resolve_with(&fx, None, Some(&frontend_id)).await;
    assert_eq!(asked.status(), StatusCode::OK);
    let asked: serde_json::Value = common::json_body(asked).await;
    assert_eq!(asked["run"]["agentId"], frontend_id);
    let ticket = ticket_json(&fx).await;
    assert!(ticket["assigneeAgentId"].is_null());
    assert_ne!(ticket["status"], "done");
}

#[tokio::test]
async fn refusing_an_ineligible_agent_starts_nothing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let fx = setup_ticket().await;
    let pm_id =
        common::create_agent_with_preset_key(&fx.app, "pm", "PM", &fx.cookie, &fx.csrf).await;
    let _frontend_id = common::create_agent_with_preset_key(
        &fx.app,
        "frontend_engineer",
        "Frontend Engineer",
        &fx.cookie,
        &fx.csrf,
    )
    .await;
    done_with_merge_conflict(&fx).await;
    let res = post_resolve_with(&fx, Some("merge"), Some(&pm_id)).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(
        body["message"],
        coppice_server::copy::conflict::AGENT_NOT_ELIGIBLE
    );
    assert_still_done(&fx).await;
}
