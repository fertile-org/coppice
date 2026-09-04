mod common;

use axum::http::StatusCode;
use coppice_server::middleware::session::parse_session_cookie;
use coppice_server::services::repo_git_service::RepoGitService;
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
        &[
            "remote",
            "add",
            "origin",
            bare.to_str().expect("utf8"),
        ],
    );
    git(&local, &["push", "-u", "origin", "main"]);
    let base = rev_parse(&local, "HEAD");
    commit_file(&local, "extra.txt", "ahead\n", "local ahead");
    // Keep origin/main at the pushed base (local is ahead).
    git(&local, &["update-ref", "refs/remotes/origin/main", &base]);

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
async fn default_branch_sync_requires_admin_and_csrf() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_root, local, _bare) = checkout_ahead_of_bare();
    let (state, app, admin_cookie, admin_csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;

    let repo_id = register_repo_with_remote(
        &app,
        &local.display().to_string(),
        Some("https://github.com/example/repo.git"),
        &admin_cookie,
        &admin_csrf,
    )
    .await;
    set_forge_token(&app, &repo_id, "ghs_testtoken", &admin_cookie, &admin_csrf).await;

    // CSRF required on POST
    let no_csrf = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/api/repos/{repo_id}/push-default-branch"))
                .header("content-type", "application/json")
                .header(axum::http::header::COOKIE, &admin_cookie)
                .body(axum::body::Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_csrf.status(), StatusCode::FORBIDDEN);

    // Member is forbidden
    let create_member = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/users",
            r#"{"email":"member@localhost","password":"secret123"}"#,
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_member.status(), StatusCode::CREATED);
    let (member_cookie, member_csrf) = login_as(&app, "member@localhost", "secret123").await;

    for uri in [
        format!("/api/repos/{repo_id}/default-branch-sync"),
        format!("/api/repos/{repo_id}/fetch"),
        format!("/api/repos/{repo_id}/push-default-branch"),
    ] {
        let method = if uri.ends_with("default-branch-sync") {
            "GET"
        } else {
            "POST"
        };
        let res = app
            .clone()
            .oneshot(common::json_request(
                method,
                &uri,
                "{}",
                &member_cookie,
                &member_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "{uri}");
    }

    let status = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{repo_id}/default-branch-sync"),
            "",
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(status).await;
    assert_eq!(body["defaultBranch"], "main");
    assert!(body["localSha"].as_str().is_some());
    assert!(body["remoteSha"].as_str().is_some());
    assert_eq!(body["aheadCount"], 1);
    assert_eq!(body["behindCount"], 0);
    assert_eq!(body["workingTreeClean"], true);
    assert_eq!(body["canPush"], true);
    assert_eq!(body["canFetch"], true);
    assert_eq!(body["pushEnabled"], true);
    assert_eq!(body["forgeTokenConfigured"], true);

    // Keep state alive for workers
    drop(state);
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
async fn push_blocked_when_missing_token_or_dirty_tree() {
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

    let push_no_token = app
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
    assert_eq!(push_no_token.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(push_no_token).await;
    assert!(
        body["message"].as_str().unwrap().to_lowercase().contains("token"),
        "{body}"
    );

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

    let repo_uuid = uuid::Uuid::parse_str(&repo_id).unwrap();
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
