use coppice_server::domain::Id;
mod common;

use axum::http::StatusCode;
use coppice_server::middleware::session::parse_session_cookie;
use coppice_server::services::repo_git_service::{
    RepoGitError, RepoGitService, SyncDefaultBranchAction,
};
use http_body_util::BodyExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tower::ServiceExt;

async fn login_as(app: &axum::Router, email: &str, password: &str) -> (String, String) {
    let login = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(format!(
                    r#"{{"email":"{email}","password":"{password}"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);

    let set_cookie = login
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .expect("session cookie");
    let cookie_header = set_cookie.to_str().unwrap();
    let session_token = parse_session_cookie(cookie_header).expect("session token");
    let cookie = format!("coppice_session={session_token}");

    let body = login.into_body().collect().await.unwrap().to_bytes();
    let login_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let csrf_token = login_json["csrfToken"]
        .as_str()
        .expect("csrf token")
        .to_string();

    (cookie, csrf_token)
}

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit_file(cwd: &Path, name: &str, contents: &str, message: &str) {
    std::fs::write(cwd.join(name), contents).expect("write");
    git(cwd, &["add", name]);
    let out = Command::new("git")
        .current_dir(cwd)
        .args(["commit", "-m", message])
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .output()
        .expect("commit");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn rev_parse(cwd: &Path, rev: &str) -> String {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(["rev-parse", rev])
        .output()
        .expect("rev-parse");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Local checkout that is one commit ahead of a bare sibling (origin/main at base).
fn checkout_ahead_of_bare() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("tempdir");
    let bare = root.path().join("remote.git");
    let local = root.path().join("local");
    std::fs::create_dir_all(&bare).expect("mkdir bare");
    std::fs::create_dir_all(&local).expect("mkdir local");

    git(&bare, &["init", "--bare", "-b", "main"]);
    git(&local, &["init", "-b", "main"]);
    commit_file(&local, "README.md", "# test\n", "initial");
    git(
        &local,
        &["remote", "add", "origin", bare.to_str().expect("utf8")],
    );
    git(&local, &["push", "-u", "origin", "main"]);
    let base = rev_parse(&local, "HEAD");
    commit_file(&local, "extra.txt", "ahead\n", "local ahead");
    // Keep origin/main at the pushed base (local is ahead).
    git(&local, &["update-ref", "refs/remotes/origin/main", &base]);

    (root, local, bare)
}

/// Local checkout matching a bare sibling (origin/main at HEAD).
fn checkout_equal_to_bare() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("tempdir");
    let bare = root.path().join("remote.git");
    let local = root.path().join("local");
    std::fs::create_dir_all(&bare).expect("mkdir bare");
    std::fs::create_dir_all(&local).expect("mkdir local");

    git(&bare, &["init", "--bare", "-b", "main"]);
    git(&local, &["init", "-b", "main"]);
    commit_file(&local, "README.md", "# test\n", "initial");
    git(
        &local,
        &["remote", "add", "origin", bare.to_str().expect("utf8")],
    );
    git(&local, &["push", "-u", "origin", "main"]);

    (root, local, bare)
}

/// Local and remote each have one unique commit after a shared base.
fn checkout_diverged_from_bare() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("tempdir");
    let bare = root.path().join("remote.git");
    let local = root.path().join("local");
    std::fs::create_dir_all(&bare).expect("mkdir bare");
    std::fs::create_dir_all(&local).expect("mkdir local");

    git(&bare, &["init", "--bare", "-b", "main"]);
    git(&local, &["init", "-b", "main"]);
    commit_file(&local, "README.md", "# test\n", "initial");
    git(
        &local,
        &["remote", "add", "origin", bare.to_str().expect("utf8")],
    );
    git(&local, &["push", "-u", "origin", "main"]);
    let base = rev_parse(&local, "HEAD");

    commit_file(&local, "remote.txt", "remote ahead\n", "remote ahead");
    git(&local, &["push", "origin", "main"]);
    let tip = rev_parse(&local, "HEAD");
    git(&local, &["reset", "--hard", &base]);
    commit_file(&local, "local.txt", "local\n", "local diverge");
    git(&local, &["update-ref", "refs/remotes/origin/main", &tip]);

    (root, local, bare)
}

/// Local checkout that is one commit behind a bare sibling (origin/main at tip).
fn checkout_behind_of_bare() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("tempdir");
    let bare = root.path().join("remote.git");
    let local = root.path().join("local");
    std::fs::create_dir_all(&bare).expect("mkdir bare");
    std::fs::create_dir_all(&local).expect("mkdir local");

    git(&bare, &["init", "--bare", "-b", "main"]);
    git(&local, &["init", "-b", "main"]);
    commit_file(&local, "README.md", "# test\n", "initial");
    git(
        &local,
        &["remote", "add", "origin", bare.to_str().expect("utf8")],
    );
    git(&local, &["push", "-u", "origin", "main"]);
    let base = rev_parse(&local, "HEAD");

    // Advance both local and remote, then rewind local so it is strictly behind.
    commit_file(&local, "remote.txt", "remote ahead\n", "remote ahead");
    git(&local, &["push", "origin", "main"]);
    let tip = rev_parse(&local, "HEAD");
    git(&local, &["reset", "--hard", &base]);
    assert_eq!(rev_parse(&local, "HEAD"), base);
    assert_eq!(rev_parse(&local, "refs/remotes/origin/main"), tip);

    (root, local, bare)
}

async fn register_repo_with_remote(
    app: &axum::Router,
    local_path: &str,
    remote_url: Option<&str>,
    cookie: &str,
    csrf: &str,
) -> String {
    let mut body = serde_json::json!({
        "name": "test-repo",
        "localPath": local_path,
        "defaultBranch": "main",
    });
    if let Some(url) = remote_url {
        body["remoteUrl"] = serde_json::json!(url);
    }
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/repos",
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = common::json_body(res).await;
    body["id"].as_str().unwrap().to_string()
}

async fn set_forge_token(app: &axum::Router, repo_id: &str, token: &str, cookie: &str, csrf: &str) {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/repos/{repo_id}/forge-token"),
            &serde_json::json!({ "token": token }).to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn push_disabled_blocks_push_but_fetch_still_allowed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_git_dir, local_path) = common::create_temp_git_checkout();

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = false;
        })
        .await;

    let repo_id = register_repo_with_remote(
        &app,
        &local_path.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;
    set_forge_token(&app, &repo_id, "ghs_testtoken", &cookie, &csrf).await;

    let sha = rev_parse(&local_path, "HEAD");
    git(
        &local_path,
        &["update-ref", "refs/remotes/origin/main", &sha],
    );
    commit_file(&local_path, "ahead.txt", "x\n", "ahead");

    let status = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{repo_id}/default-branch-sync"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(status).await;
    assert_eq!(body["canPush"], false);
    assert!(body["pushDisabledReason"]
        .as_str()
        .unwrap()
        .contains("push_enabled"));
    assert_eq!(body["canFetch"], true);

    let push = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/repos/{repo_id}/push-default-branch"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(push.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn push_blocked_when_missing_remote() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;
    let (_git, local) = common::create_temp_git_checkout();
    let repo_id =
        register_repo_with_remote(&app, &local.display().to_string(), None, &cookie, &csrf).await;
    set_forge_token(&app, &repo_id, "ghs_testtoken", &cookie, &csrf).await;

    let sha = rev_parse(&local, "HEAD");
    git(&local, &["update-ref", "refs/remotes/origin/main", &sha]);
    commit_file(&local, "ahead.txt", "x\n", "ahead");

    let status = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{repo_id}/default-branch-sync"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let body: serde_json::Value = common::json_body(status).await;
    assert_eq!(body["canPush"], false);
    assert_eq!(body["canFetch"], false);

    let push = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/repos/{repo_id}/push-default-branch"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(push.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(push).await;
    assert!(
        body["message"].as_str().unwrap().contains("remote"),
        "{body}"
    );
}

#[tokio::test]
async fn push_blocked_when_dirty_tree() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;

    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id = register_repo_with_remote(
        &app,
        &local_path.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;

    let sha = rev_parse(&local_path, "HEAD");
    git(
        &local_path,
        &["update-ref", "refs/remotes/origin/main", &sha],
    );
    commit_file(&local_path, "ahead.txt", "x\n", "ahead");

    set_forge_token(&app, &repo_id, "ghs_testtoken", &cookie, &csrf).await;
    std::fs::write(local_path.join("dirty.txt"), "dirty").expect("dirty");

    let status = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{repo_id}/default-branch-sync"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let body: serde_json::Value = common::json_body(status).await;
    assert_eq!(body["canPush"], false);
    assert_eq!(body["workingTreeClean"], false);
    assert!(body["pushDisabledReason"]
        .as_str()
        .unwrap()
        .contains("uncommitted"));

    let push_dirty = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/repos/{repo_id}/push-default-branch"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(push_dirty.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn successful_default_branch_push_via_bare_sibling() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_root, local, bare) = checkout_ahead_of_bare();
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;

    let repo_id = register_repo_with_remote(
        &app,
        &local.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;
    set_forge_token(&app, &repo_id, "ghs_testtoken", &cookie, &csrf).await;

    let repo_uuid = Id::parse_str(&repo_id).unwrap();
    let pool = state.db.as_ref().expect("db");
    let svc = RepoGitService::new(pool, true, &state.secret_store);
    let result = svc
        .push_default_branch_to_remote(repo_uuid, bare.to_str().unwrap())
        .await
        .expect("push to bare");

    assert_eq!(result.status.ahead_count, Some(0));
    assert_eq!(result.status.behind_count, Some(0));
    assert_eq!(result.status.can_push, false);
    assert_eq!(
        rev_parse(&bare, "refs/heads/main"),
        rev_parse(&local, "HEAD")
    );

    // HTTP status reflects post-push state
    let status = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{repo_id}/default-branch-sync"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(status).await;
    assert_eq!(body["aheadCount"], 0);
    assert_eq!(body["canPush"], false);
}

#[tokio::test]
async fn successful_default_branch_pull_via_bare_sibling() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_root, local, bare) = checkout_behind_of_bare();
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;

    let repo_id = register_repo_with_remote(
        &app,
        &local.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;
    set_forge_token(&app, &repo_id, "ghs_testtoken", &cookie, &csrf).await;

    let status = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{repo_id}/default-branch-sync"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(status).await;
    assert_eq!(body["aheadCount"], 0);
    assert_eq!(body["behindCount"], 1);
    assert_eq!(body["canPull"], true);
    assert_eq!(body["canPush"], false);
    assert!(
        body["pushDisabledReason"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("pull"),
        "{body}"
    );

    let repo_uuid = Id::parse_str(&repo_id).unwrap();
    let pool = state.db.as_ref().expect("db");
    let svc = RepoGitService::new(pool, true, &state.secret_store);
    let result = svc
        .pull_default_branch_from_remote(repo_uuid, bare.to_str().unwrap())
        .await
        .expect("pull from bare");

    assert_eq!(result.status.ahead_count, Some(0));
    assert_eq!(result.status.behind_count, Some(0));
    assert_eq!(result.status.can_pull, false);
    assert_eq!(
        rev_parse(&local, "HEAD"),
        rev_parse(&bare, "refs/heads/main")
    );

    let status = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{repo_id}/default-branch-sync"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(status).await;
    assert_eq!(body["behindCount"], 0);
    assert_eq!(body["canPull"], false);
}

#[tokio::test]
async fn pull_blocked_when_dirty_or_diverged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_root, local, bare) = checkout_behind_of_bare();
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;

    let repo_id = register_repo_with_remote(
        &app,
        &local.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;

    set_forge_token(&app, &repo_id, "ghs_testtoken", &cookie, &csrf).await;
    std::fs::write(local.join("dirty.txt"), "dirty").expect("dirty");

    let status = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{repo_id}/default-branch-sync"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let body: serde_json::Value = common::json_body(status).await;
    assert_eq!(body["canPull"], false);
    assert!(body["pullDisabledReason"]
        .as_str()
        .unwrap()
        .contains("uncommitted"));

    let pull_dirty = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/repos/{repo_id}/pull-default-branch"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(pull_dirty.status(), StatusCode::BAD_REQUEST);

    std::fs::remove_file(local.join("dirty.txt")).expect("clean dirty");

    // Diverge: local commit + remote tip still ahead of original base.
    commit_file(&local, "local.txt", "local\n", "local diverge");
    let status = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{repo_id}/default-branch-sync"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let body: serde_json::Value = common::json_body(status).await;
    assert_eq!(body["canPull"], false);
    let reason = body["pullDisabledReason"].as_str().unwrap();
    assert!(
        reason.contains("diverged") || reason.contains("ahead"),
        "{body}"
    );

    let repo_uuid = Id::parse_str(&repo_id).unwrap();
    let pool = state.db.as_ref().expect("db");
    let svc = RepoGitService::new(pool, true, &state.secret_store);
    let err = svc
        .pull_default_branch_from_remote(repo_uuid, bare.to_str().unwrap())
        .await
        .expect_err("diverged pull must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("diverged") || msg.contains("ahead") || msg.contains("fast-forward"),
        "{msg}"
    );
}

#[tokio::test]
async fn sync_pushes_when_ahead() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_root, local, bare) = checkout_ahead_of_bare();
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;

    let repo_id = register_repo_with_remote(
        &app,
        &local.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;

    let repo_uuid = Id::parse_str(&repo_id).unwrap();
    let pool = state.db.as_ref().expect("db");
    let svc = RepoGitService::new(pool, true, &state.secret_store);
    let result = svc
        .sync_default_branch_with_remote(repo_uuid, bare.to_str().unwrap())
        .await
        .expect("sync push");

    assert_eq!(result.action, SyncDefaultBranchAction::Pushed);
    assert_eq!(result.status.ahead_count, Some(0));
    assert_eq!(result.status.behind_count, Some(0));
    assert_eq!(
        rev_parse(&bare, "refs/heads/main"),
        rev_parse(&local, "HEAD")
    );

    // HTTP sync uses origin when no forge token is set.
    let (_root2, local2, _bare2) = checkout_ahead_of_bare();
    let repo_id2 = register_repo_with_remote(
        &app,
        &local2.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;
    let http = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/repos/{repo_id2}/sync"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(http.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(http).await;
    assert_eq!(body["action"], "pushed");
    assert_eq!(body["status"]["aheadCount"], 0);
    assert_eq!(body["status"]["behindCount"], 0);
}

#[tokio::test]
async fn sync_pulls_when_behind() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_root, local, bare) = checkout_behind_of_bare();
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;

    let repo_id = register_repo_with_remote(
        &app,
        &local.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;

    let repo_uuid = Id::parse_str(&repo_id).unwrap();
    let pool = state.db.as_ref().expect("db");
    let svc = RepoGitService::new(pool, true, &state.secret_store);
    let result = svc
        .sync_default_branch_with_remote(repo_uuid, bare.to_str().unwrap())
        .await
        .expect("sync pull");

    assert_eq!(result.action, SyncDefaultBranchAction::Pulled);
    assert_eq!(result.status.ahead_count, Some(0));
    assert_eq!(result.status.behind_count, Some(0));
    assert_eq!(
        rev_parse(&local, "HEAD"),
        rev_parse(&bare, "refs/heads/main")
    );
}

#[tokio::test]
async fn sync_up_to_date_when_equal() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_root, local, bare) = checkout_equal_to_bare();
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;

    let repo_id = register_repo_with_remote(
        &app,
        &local.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;

    let repo_uuid = Id::parse_str(&repo_id).unwrap();
    let pool = state.db.as_ref().expect("db");
    let svc = RepoGitService::new(pool, true, &state.secret_store);
    let result = svc
        .sync_default_branch_with_remote(repo_uuid, bare.to_str().unwrap())
        .await
        .expect("sync equal");

    assert_eq!(result.action, SyncDefaultBranchAction::UpToDate);
    assert_eq!(result.status.ahead_count, Some(0));
    assert_eq!(result.status.behind_count, Some(0));
}

#[tokio::test]
async fn sync_errors_when_diverged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_root, local, bare) = checkout_diverged_from_bare();
    let before = rev_parse(&local, "HEAD");
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;

    let repo_id = register_repo_with_remote(
        &app,
        &local.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;

    let repo_uuid = Id::parse_str(&repo_id).unwrap();
    let pool = state.db.as_ref().expect("db");
    let svc = RepoGitService::new(pool, true, &state.secret_store);
    let err = svc
        .sync_default_branch_with_remote(repo_uuid, bare.to_str().unwrap())
        .await
        .expect_err("diverged sync must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("diverged") || msg.contains("behind") || msg.contains("resolve"),
        "{msg}"
    );
    assert_eq!(rev_parse(&local, "HEAD"), before);
}

#[tokio::test]
async fn sync_errors_when_push_disabled_and_ahead() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_root, local, bare) = checkout_ahead_of_bare();
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = false;
        })
        .await;

    let repo_id = register_repo_with_remote(
        &app,
        &local.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;

    let repo_uuid = Id::parse_str(&repo_id).unwrap();
    let pool = state.db.as_ref().expect("db");
    let svc = RepoGitService::new(pool, false, &state.secret_store);
    let err = svc
        .sync_default_branch_with_remote(repo_uuid, bare.to_str().unwrap())
        .await
        .expect_err("push disabled");
    assert!(
        matches!(err, RepoGitError::PushDisabled) || err.to_string().contains("push_enabled"),
        "{err}"
    );

    let http = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/repos/{repo_id}/sync"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(http.status(), StatusCode::FORBIDDEN);
}
