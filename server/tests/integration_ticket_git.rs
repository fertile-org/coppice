mod common;

use axum::http::StatusCode;
use std::path::{Path, PathBuf};
use std::process::Command;
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

fn git_abbrev_ref(dir: &Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("git rev-parse abbrev");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).trim().to_string()
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

fn git_status_porcelain(dir: &Path) -> String {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()
        .expect("git status");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn git_rebase_in_progress(dir: &Path) -> bool {
    dir.join(".git").join("rebase-merge").exists()
        || dir.join(".git").join("rebase-apply").exists()
        || {
            // worktrees use a .git file pointing at the gitdir
            let git_file = dir.join(".git");
            if git_file.is_file() {
                let contents = std::fs::read_to_string(&git_file).unwrap_or_default();
                if let Some(gitdir) = contents.strip_prefix("gitdir: ") {
                    let gitdir = PathBuf::from(gitdir.trim());
                    return gitdir.join("rebase-merge").exists()
                        || gitdir.join("rebase-apply").exists();
                }
            }
            false
        }
}

async fn setup_repo_ticket_and_worktree() -> (
    tempfile::TempDir,
    common::AgentTestEnv,
    axum::Router,
    String,
    String,
    String,
    PathBuf,
    PathBuf,
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
        ticket_id,
        local_path,
        worktree_path,
    )
}

#[tokio::test]
async fn rebase_branch_happy_path_advances_tip_and_comments() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_keep, _env, app, cookie, csrf, ticket_id, local_path, worktree_path) =
        setup_repo_ticket_and_worktree().await;

    let tip_before = git_rev_parse(&worktree_path, "HEAD");

    // Advance main in the main checkout while the worktree stays on the feature tip.
    std::fs::write(local_path.join("README.md"), "# test\nbase update\n").expect("write");
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&local_path)
        .output()
        .expect("git add");
    git_env_commit(&local_path, "base update");
    let main_sha = git_rev_parse(&local_path, "main");

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/rebase-branch"),
            r#"{"baseBranch":"main"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    let rebase = &body["rebase"];
    assert_eq!(rebase["baseBranch"], "main");
    assert_eq!(rebase["ontoRef"], "main");
    assert!(!rebase["ticketBranch"].as_str().unwrap().is_empty());
    let head_sha = rebase["headSha"].as_str().unwrap();
    assert_ne!(head_sha, tip_before);
    assert!(rebase["message"].as_str().unwrap().contains("Rebased"));

    let tip_after = git_rev_parse(&worktree_path, "HEAD");
    assert_eq!(tip_after, head_sha);
    // Rebased tip should contain the new main commit as an ancestor.
    let merge_base = git_rev_parse(&worktree_path, &format!("{main_sha}"));
    assert_eq!(merge_base, main_sha);
    let contains = Command::new("git")
        .args(["merge-base", "--is-ancestor", &main_sha, "HEAD"])
        .current_dir(&worktree_path)
        .status()
        .expect("merge-base");
    assert!(contains.success());
    assert!(git_status_porcelain(&worktree_path).trim().is_empty());

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
    let comments = comments.as_array().unwrap();
    assert!(
        comments
            .iter()
            .any(|c| { c["body"].as_str().unwrap_or("").contains("**Rebase:**") }),
        "expected rebase system comment, got {comments:?}"
    );
}

#[tokio::test]
async fn rebase_branch_rejects_dirty_worktree_without_rewriting() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_keep, _env, app, cookie, csrf, ticket_id, _local_path, worktree_path) =
        setup_repo_ticket_and_worktree().await;

    let tip_before = git_rev_parse(&worktree_path, "HEAD");
    std::fs::write(worktree_path.join("dirty.txt"), "uncommitted\n").expect("dirty write");

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/rebase-branch"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    let message = body["message"].as_str().unwrap_or("");
    assert!(
        message.contains("uncommitted"),
        "unexpected message: {message}"
    );

    assert_eq!(git_rev_parse(&worktree_path, "HEAD"), tip_before);
    assert!(!git_rebase_in_progress(&worktree_path));
    assert!(git_status_porcelain(&worktree_path).contains("dirty.txt"));
}

#[tokio::test]
async fn rebase_branch_conflict_aborts_and_leaves_clean_worktree() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_keep, _env, app, cookie, csrf, ticket_id, local_path, worktree_path) =
        setup_repo_ticket_and_worktree().await;

    // Divergent edits to README.md on main vs ticket branch.
    std::fs::write(local_path.join("README.md"), "# test\nmain side\n").expect("write main");
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&local_path)
        .output()
        .expect("git add");
    git_env_commit(&local_path, "main conflict");

    std::fs::write(worktree_path.join("README.md"), "# test\nfeature side\n").expect("write wt");
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&worktree_path)
        .output()
        .expect("git add");
    git_env_commit(&worktree_path, "feature conflict");

    let tip_before = git_rev_parse(&worktree_path, "HEAD");
    let branch_before = git_abbrev_ref(&worktree_path);
    let main_before = git_rev_parse(&local_path, "HEAD");
    let main_branch_before = git_abbrev_ref(&local_path);

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/rebase-branch"),
            r#"{"baseBranch":"main"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    let message = body["message"].as_str().unwrap_or("");
    assert_eq!(
        message,
        coppice_server::copy::conflict::conflict_message(
            coppice_server::copy::conflict::REBASE_ACTION,
            "main",
            &["README.md".into()],
        ),
        "unexpected message: {message}"
    );
    assert_eq!(body["conflict"]["files"], serde_json::json!(["README.md"]));
    assert_eq!(body["conflict"]["operation"], "rebase");
    assert_eq!(body["conflict"]["baseBranch"], "main");
    assert_eq!(body["conflict"]["canAskAssignee"], false);
    assert_eq!(
        body["conflict"]["unavailableReason"],
        coppice_server::copy::conflict::NO_ASSIGNEE
    );
    assert!(body["conflict"]["askLabel"].is_null());
    assert_eq!(
        body["conflict"]["rereviewNote"],
        coppice_server::copy::conflict::REREVIEW_NOTE
    );

    assert!(!git_rebase_in_progress(&worktree_path));
    assert!(
        git_status_porcelain(&worktree_path).trim().is_empty(),
        "worktree should be clean after abort: {}",
        git_status_porcelain(&worktree_path)
    );
    // Tip and branch are back to the pre-rebase commit (abort restores them).
    assert_eq!(git_rev_parse(&worktree_path, "HEAD"), tip_before);
    assert_eq!(git_abbrev_ref(&worktree_path), branch_before);
    assert_eq!(git_rev_parse(&local_path, "HEAD"), main_before);
    assert_eq!(git_abbrev_ref(&local_path), main_branch_before);
    assert_eq!(count_runs(&app, &ticket_id, &cookie, &csrf).await, 0);
}

async fn count_runs(app: &axum::Router, ticket_id: &str, cookie: &str, csrf: &str) -> usize {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/runs"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body["runs"].as_array().unwrap().len()
}

#[tokio::test]
async fn merge_branch_conflict_aborts_and_leaves_branch_unchanged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_keep, _env, app, cookie, csrf, ticket_id, local_path, worktree_path) =
        setup_repo_ticket_and_worktree().await;

    std::fs::write(local_path.join("README.md"), "# test\nmain side\n").expect("write main");
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&local_path)
        .output()
        .expect("git add");
    git_env_commit(&local_path, "main conflict");

    std::fs::write(worktree_path.join("README.md"), "# test\nfeature side\n").expect("write wt");
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&worktree_path)
        .output()
        .expect("git add");
    git_env_commit(&worktree_path, "feature conflict");

    let status = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"wait_for_final_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let approve = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/final-approve"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK, "final approve");

    let tip_before = git_rev_parse(&worktree_path, "HEAD");
    let main_before = git_rev_parse(&local_path, "HEAD");
    let main_branch_before = git_abbrev_ref(&local_path);
    let worktree_branch_before = git_abbrev_ref(&worktree_path);

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/merge-branch"),
            r#"{"baseBranch":"main"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(
        body["message"],
        coppice_server::copy::conflict::conflict_message(
            coppice_server::copy::conflict::MERGE_ACTION,
            "main",
            &["README.md".into()],
        )
    );
    assert_eq!(body["conflict"]["files"], serde_json::json!(["README.md"]));
    assert_eq!(body["conflict"]["operation"], "merge");
    assert!(!local_path.join(".git").join("MERGE_HEAD").exists());
    assert_eq!(git_rev_parse(&local_path, "HEAD"), main_before);
    assert_eq!(git_abbrev_ref(&local_path), main_branch_before);
    assert!(git_status_porcelain(&local_path).trim().is_empty());
    assert_eq!(git_rev_parse(&worktree_path, "HEAD"), tip_before);
    assert_eq!(git_abbrev_ref(&worktree_path), worktree_branch_before);
    assert!(git_status_porcelain(&worktree_path).trim().is_empty());
    assert_eq!(count_runs(&app, &ticket_id, &cookie, &csrf).await, 0);

    let ticket = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let ticket: serde_json::Value = common::json_body(ticket).await;
    assert_eq!(ticket["status"], "done", "a conflict leaves the ticket in Done");
    assert!(ticket["assigneeAgentId"].is_null());
    assert_eq!(ticket["humanReview"]["stale"], false);
    assert_eq!(ticket["humanReview"]["merged"], false);
    assert_eq!(body["conflict"]["canAskAssignee"], false);
    assert_eq!(
        body["conflict"]["unavailableReason"],
        coppice_server::copy::conflict::NO_ASSIGNEE
    );

    let comments = app
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
    let comments: serde_json::Value = common::json_body(comments).await;
    let expected = coppice_server::copy::conflict::merge_conflict_reopen_comment(
        "main",
        &["README.md".into()],
    );
    assert!(!comments
        .as_array()
        .unwrap()
        .iter()
        .any(|comment| comment["body"] == expected));
}

#[tokio::test]
async fn rebase_branch_missing_worktree_returns_bad_request() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let _repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &_repo_id, &cookie, &csrf).await;

    // Create the ticket branch in the main checkout but no worktree.
    let branch = format!(
        "agent/TICKET-{}",
        ticket_id.split('-').next().unwrap_or("ticket")
    );
    let output = Command::new("git")
        .args(["branch", &branch, "main"])
        .current_dir(&local_path)
        .output()
        .expect("git branch");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/rebase-branch"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    let message = body["message"].as_str().unwrap_or("");
    assert!(
        message.to_lowercase().contains("worktree"),
        "unexpected message: {message}"
    );
}

async fn setup_finished_ticket() -> (
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
    let worktree_path = common::setup_worktree_with_commit(
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
        worktree_path,
    )
}

fn branch_exists(repo: &Path, ticket_id: uuid::Uuid) -> bool {
    let short = ticket_id.to_string();
    let short = short.split('-').next().unwrap();
    Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/agent/TICKET-{short}"),
        ])
        .current_dir(repo)
        .output()
        .expect("git rev-parse")
        .status
        .success()
}

async fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if done() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    done()
}

async fn archive(app: &axum::Router, ticket_id: uuid::Uuid, cookie: &str, csrf: &str) {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/archive"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn archiving_removes_the_worktree_and_the_ticket_branch() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_git, _env, _state, app, cookie, csrf, ticket_id, repo, worktree) =
        setup_finished_ticket().await;
    assert!(worktree.exists());
    assert!(branch_exists(&repo, ticket_id));

    archive(&app, ticket_id, &cookie, &csrf).await;

    assert!(wait_until(|| !worktree.exists()).await, "worktree left behind");
    assert!(wait_until(|| !branch_exists(&repo, ticket_id)).await, "branch left behind");
}

#[tokio::test]
async fn cleanup_keeps_a_dirty_worktree_and_says_so() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_git, _env, state, app, cookie, csrf, ticket_id, repo, worktree) =
        setup_finished_ticket().await;
    std::fs::write(worktree.join("unsaved.txt"), "work in progress").unwrap();

    archive(&app, ticket_id, &cookie, &csrf).await;

    let pool = state.db.clone().unwrap();
    let mut posted = false;
    for _ in 0..200 {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ticket_comments WHERE ticket_id = $1 AND body = $2",
        )
        .bind(ticket_id)
        .bind(coppice_server::services::worktree_cleanup::KEPT_UNCOMMITTED_WORKTREE)
        .fetch_one(&pool)
        .await
        .unwrap();
        if count == 1 {
            posted = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(posted, "no note about the kept worktree");
    assert!(worktree.join("unsaved.txt").exists());
    assert!(branch_exists(&repo, ticket_id));
}

#[tokio::test]
async fn an_active_run_blocks_cleanup_and_manual_remove() {
    use coppice_server::services::ticket_git_service::WorktreeCleanup;
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_git, env, state, app, cookie, csrf, ticket_id, repo, worktree) =
        setup_finished_ticket().await;
    let pool = state.db.clone().unwrap();
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Busy Agent",
        &cookie,
        &csrf,
    )
    .await;
    let run_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (id, ticket_id, agent_id, job_type, status, sandbox_profile_id)
        VALUES ($1, $2, $3, 'implementation', 'running', 'permissive')
        "#,
    )
    .bind(run_id)
    .bind(ticket_id)
    .bind(uuid::Uuid::parse_str(&agent_id).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let removed = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/remove-worktree"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(removed.status(), StatusCode::CONFLICT);

    let outcome = coppice_server::services::worktree_cleanup::cleanup_ticket(
        &pool,
        env.worktrees.path().to_path_buf(),
        ticket_id,
        false,
    )
    .await;
    assert_eq!(outcome, WorktreeCleanup::SkippedActiveRun);
    assert!(worktree.exists());
    assert!(branch_exists(&repo, ticket_id));
}

#[tokio::test]
async fn the_sweep_cleans_merged_tickets_and_repeats_safely() {
    use coppice_server::services::ticket_git_service::WorktreeCleanup;
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_git, env, state, _app, _cookie, _csrf, ticket_id, repo, worktree) =
        setup_finished_ticket().await;
    let pool = state.db.clone().unwrap();
    sqlx::query(
        r#"
        INSERT INTO ticket_human_reviews (ticket_id, head_sha, merged_sha, merged_at)
        VALUES ($1, 'abc', 'abc', now())
        "#,
    )
    .bind(ticket_id)
    .execute(&pool)
    .await
    .unwrap();

    let removed = coppice_server::services::worktree_cleanup::sweep(
        &pool,
        env.worktrees.path().to_path_buf(),
    )
    .await
    .unwrap();
    assert_eq!(removed, 1);
    assert!(!worktree.exists());
    assert!(!branch_exists(&repo, ticket_id));

    let again = coppice_server::services::worktree_cleanup::cleanup_ticket(
        &pool,
        env.worktrees.path().to_path_buf(),
        ticket_id,
        true,
    )
    .await;
    assert_eq!(again, WorktreeCleanup::NothingToDo);
}
