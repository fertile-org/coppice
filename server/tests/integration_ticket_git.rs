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
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
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
    assert!(rebase["message"]
        .as_str()
        .unwrap()
        .contains("Rebased"));

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
        comments.iter().any(|c| {
            c["body"]
                .as_str()
                .unwrap_or("")
                .contains("**Rebase:**")
        }),
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
    assert!(
        message.to_lowercase().contains("conflict") || message.contains("README"),
        "unexpected message: {message}"
    );

    assert!(!git_rebase_in_progress(&worktree_path));
    assert!(
        git_status_porcelain(&worktree_path).trim().is_empty(),
        "worktree should be clean after abort: {}",
        git_status_porcelain(&worktree_path)
    );
    // Tip should be back to the pre-rebase commit (abort restores it).
    assert_eq!(git_rev_parse(&worktree_path, "HEAD"), tip_before);
}

#[tokio::test]
async fn rebase_branch_missing_worktree_returns_bad_request() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (git_dir, local_path) = common::create_temp_git_checkout();
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let _repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
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
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    drop(git_dir);

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
