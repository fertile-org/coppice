mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use axum::http::StatusCode;
use coppice_server::services::ticket_git_service::TicketGitService;
use tower::ServiceExt;
use uuid::Uuid;

const MERGE_REFUSED: &str = "Not merged. This isn't the commit you reviewed.";

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

fn commit_file(dir: &Path, name: &str, body: &str, message: &str) {
    std::fs::write(dir.join(name), body).expect("write");
    let add = Command::new("git")
        .args(["add", name])
        .current_dir(dir)
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed");
    git_env_commit(dir, message);
}

struct Fixture {
    _git_keep: tempfile::TempDir,
    _env: common::AgentTestEnv,
    state: std::sync::Arc<coppice_server::AppState>,
    app: axum::Router,
    cookie: String,
    csrf: String,
    ticket_id: String,
    local_path: PathBuf,
    worktree_path: PathBuf,
}

async fn setup() -> Fixture {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
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
    Fixture {
        _git_keep: git_dir,
        _env: env,
        state,
        app,
        cookie,
        csrf,
        ticket_id,
        local_path,
        worktree_path,
    }
}

async fn set_wait_for_human_review(fx: &Fixture) {
    let res = fx
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
    assert_eq!(res.status(), StatusCode::OK);
}

async fn final_approve(fx: &Fixture) -> serde_json::Value {
    let res = fx
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
    assert_eq!(res.status(), StatusCode::OK, "final approve");
    common::json_body(res).await
}

async fn get_ticket(fx: &Fixture) -> serde_json::Value {
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

async fn post_comment(fx: &Fixture, body: &str, intent: &str) -> Uuid {
    let payload = serde_json::json!({ "body": body, "intent": intent });
    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/comments", fx.ticket_id),
            &payload.to_string(),
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "comment {intent}");
    let json = common::json_body(res).await;
    Uuid::parse_str(json["id"].as_str().unwrap()).unwrap()
}

fn uuid_list(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn approval_records_head_sha_and_evidence() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = setup().await;
    let review_comment = post_comment(&fx, "Looks good", "review_feedback").await;
    let qa_comment = post_comment(&fx, "Checks passed", "qa_passed").await;
    let progress = post_comment(&fx, "Still working", "progress_update").await;

    let agent_id = Uuid::parse_str(
        &common::create_agent_with_preset_key(
            &fx.app,
            "backend_engineer",
            "Backend Engineer",
            &fx.cookie,
            &fx.csrf,
        )
        .await,
    )
    .unwrap();
    let ticket_id = Uuid::parse_str(&fx.ticket_id).unwrap();
    let succeeded_run = Uuid::new_v4();
    let queued_run = Uuid::new_v4();
    let pool = fx.state.db.clone().expect("pool");
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
            context_profile, submitted_result, ended_at
        )
        VALUES
            ($1, $3, $4, 'work_on_ticket', 'succeeded', 'permissive-default', 'full', '{"summary":"checks passed"}', now()),
            ($2, $3, $4, 'work_on_ticket', 'queued', 'permissive-default', 'full', NULL, NULL)
        "#,
    )
    .bind(succeeded_run)
    .bind(queued_run)
    .bind(ticket_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert runs");

    set_wait_for_human_review(&fx).await;
    let head = git_rev_parse(&fx.worktree_path, "HEAD");
    let approved = final_approve(&fx).await;

    assert_eq!(approved["status"], "done");
    assert_eq!(approved["humanReview"]["headSha"], head);
    assert_eq!(approved["humanReview"]["shortSha"], &head[..7]);
    assert_eq!(approved["humanReview"]["stale"], false);
    let comment_ids = uuid_list(&approved["humanReview"]["commentIds"]);
    assert!(comment_ids.contains(&review_comment.to_string()));
    assert!(comment_ids.contains(&qa_comment.to_string()));
    assert!(!comment_ids.contains(&progress.to_string()));
    let run_ids = uuid_list(&approved["humanReview"]["runIds"]);
    assert_eq!(run_ids, vec![succeeded_run.to_string()]);
}

#[tokio::test]
async fn merge_succeeds_when_head_matches_approval() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = setup().await;
    set_wait_for_human_review(&fx).await;
    let head = git_rev_parse(&fx.worktree_path, "HEAD");
    final_approve(&fx).await;

    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/merge-branch", fx.ticket_id),
            r#"{"baseBranch":"main"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "merge");
    let body = common::json_body(res).await;
    assert!(body["merge"]["message"]
        .as_str()
        .unwrap()
        .contains("Merged"));

    let ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", &head, "HEAD"])
        .current_dir(&fx.local_path)
        .status()
        .expect("merge-base");
    assert!(ancestor.success(), "approved commit was not merged");
    assert_eq!(git_rev_parse(&fx.local_path, "HEAD"), head);
}

#[tokio::test]
async fn merge_refuses_a_different_sha() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = setup().await;
    set_wait_for_human_review(&fx).await;
    let approved_sha = git_rev_parse(&fx.worktree_path, "HEAD");
    let main_before = git_rev_parse(&fx.local_path, "main");
    final_approve(&fx).await;
    commit_file(
        &fx.worktree_path,
        "later.txt",
        "after review\n",
        "after review",
    );
    assert_ne!(git_rev_parse(&fx.worktree_path, "HEAD"), approved_sha);

    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/merge-branch", fx.ticket_id),
            r#"{"baseBranch":"main"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = common::json_body(res).await;
    assert_eq!(body["message"], MERGE_REFUSED);
    assert_eq!(git_rev_parse(&fx.local_path, "main"), main_before);

    let ticket = get_ticket(&fx).await;
    assert_eq!(ticket["status"], "in_review");
    assert_eq!(ticket["humanReview"]["stale"], true);
    assert_eq!(ticket["humanReview"]["headSha"], approved_sha);
}

#[tokio::test]
async fn new_commit_invalidates_review_and_returns_to_in_review() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = setup().await;
    set_wait_for_human_review(&fx).await;
    let approved_sha = git_rev_parse(&fx.worktree_path, "HEAD");
    final_approve(&fx).await;
    commit_file(
        &fx.worktree_path,
        "follow-up.txt",
        "new work\n",
        "follow-up",
    );

    let pool = fx.state.db.clone().expect("pool");
    let changed =
        TicketGitService::new(&pool, PathBuf::from(&fx.state.config.agent.worktrees_path))
            .note_branch_head(Uuid::parse_str(&fx.ticket_id).unwrap())
            .await
            .expect("note branch head");
    assert!(changed);

    let ticket = get_ticket(&fx).await;
    assert_eq!(ticket["status"], "in_review");
    assert_eq!(ticket["humanReview"]["stale"], true);
    assert_eq!(ticket["humanReview"]["headSha"], approved_sha);
    assert_ne!(
        git_rev_parse(&fx.worktree_path, "HEAD"),
        ticket["humanReview"]["headSha"].as_str().unwrap()
    );
}

#[tokio::test]
async fn merged_ticket_is_not_marked_stale_when_branch_moves() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = setup().await;
    set_wait_for_human_review(&fx).await;
    let approved_sha = git_rev_parse(&fx.worktree_path, "HEAD");
    final_approve(&fx).await;

    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/merge-branch", fx.ticket_id),
            r#"{"baseBranch":"main"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "merge");

    commit_file(
        &fx.worktree_path,
        "after-merge.txt",
        "later commit\n",
        "after merge",
    );
    assert_ne!(git_rev_parse(&fx.worktree_path, "HEAD"), approved_sha);

    let pool = fx.state.db.clone().expect("pool");
    let changed =
        TicketGitService::new(&pool, PathBuf::from(&fx.state.config.agent.worktrees_path))
            .note_branch_head(Uuid::parse_str(&fx.ticket_id).unwrap())
            .await
            .expect("note branch head");
    assert!(!changed);

    let ticket = get_ticket(&fx).await;
    assert_eq!(ticket["status"], "done");
    assert_eq!(ticket["humanReview"]["stale"], false);
    assert_eq!(ticket["humanReview"]["headSha"], approved_sha);
    assert_eq!(
        stored_merged_sha(&fx).await.as_deref(),
        Some(approved_sha.as_str())
    );
}

#[tokio::test]
async fn user_merge_comment_does_not_keep_review_fresh() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = setup().await;
    set_wait_for_human_review(&fx).await;
    let approved_sha = git_rev_parse(&fx.worktree_path, "HEAD");
    final_approve(&fx).await;
    post_comment(&fx, "**Merge:** into main", "progress_update").await;
    assert!(stored_merged_sha(&fx).await.is_none());

    commit_file(
        &fx.worktree_path,
        "after-comment.txt",
        "later commit\n",
        "after comment",
    );
    let pool = fx.state.db.clone().expect("pool");
    let changed =
        TicketGitService::new(&pool, PathBuf::from(&fx.state.config.agent.worktrees_path))
            .note_branch_head(Uuid::parse_str(&fx.ticket_id).unwrap())
            .await
            .expect("note branch head");
    assert!(changed);

    let ticket = get_ticket(&fx).await;
    assert_eq!(ticket["status"], "in_review");
    assert_eq!(ticket["humanReview"]["stale"], true);
    assert_eq!(ticket["humanReview"]["headSha"], approved_sha);
    assert!(stored_merged_sha(&fx).await.is_none());
}

async fn stored_merged_sha(fx: &Fixture) -> Option<String> {
    let pool = fx.state.db.clone().expect("pool");
    sqlx::query_scalar(
        "SELECT merged_sha FROM ticket_human_reviews WHERE ticket_id = $1",
    )
    .bind(Uuid::parse_str(&fx.ticket_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("merged_sha")
}

#[tokio::test]
async fn merge_without_acceptance_is_refused() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = setup().await;
    set_wait_for_human_review(&fx).await;
    let main_before = git_rev_parse(&fx.local_path, "main");

    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/merge-branch", fx.ticket_id),
            r#"{"baseBranch":"main"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = common::json_body(res).await;
    assert_eq!(body["message"], MERGE_REFUSED);
    assert_eq!(git_rev_parse(&fx.local_path, "main"), main_before);
    let ticket = get_ticket(&fx).await;
    assert_eq!(ticket["status"], "wait_for_final_review");
    assert!(ticket["humanReview"].is_null());
}
