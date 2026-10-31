mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

async fn get_file(
    app: &axum::Router,
    ticket_id: &str,
    path: &str,
    cookie: &str,
    csrf: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/files/content?path={path}"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap()
}

async fn body_bytes(res: axum::response::Response) -> Vec<u8> {
    axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

#[tokio::test]
async fn previews_files_from_repo_then_worktree() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let (_state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;

    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    std::fs::write(local_path.join("notes.txt"), "from repo checkout\n").unwrap();
    std::fs::write(local_path.join("pixel.png"), [0x89, b'P', b'N', b'G', 0, 0]).unwrap();
    std::fs::write(local_path.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();

    let res = get_file(&app, &ticket_id, "notes.txt", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["x-coppice-file-source"], "repo");
    assert_eq!(res.headers()["content-type"], "text/plain; charset=utf-8");
    assert_eq!(res.headers()["x-content-type-options"], "nosniff");
    assert_eq!(body_bytes(res).await, b"from repo checkout\n");

    let res = get_file(&app, &ticket_id, "./pixel.png", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "image/png");

    let res = get_file(&app, &ticket_id, "blob.bin", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let res = get_file(&app, &ticket_id, "missing.txt", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    for unsafe_path in ["..%2Fetc%2Fpasswd", "%2Fetc%2Fpasswd", ".git%2Fconfig"] {
        let res = get_file(&app, &ticket_id, unsafe_path, &cookie, &csrf).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{unsafe_path}");
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/hostname", local_path.join("escape.txt")).unwrap();
        let res = get_file(&app, &ticket_id, "escape.txt", &cookie, &csrf).await;
        assert_ne!(res.status(), StatusCode::OK);
    }

    // Once the ticket has a worktree, previews read the agent's copy.
    common::setup_worktree_with_commit(&local_path, env.worktrees.path(), "test-repo", &ticket_id);
    let res = get_file(&app, &ticket_id, "feature.txt", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["x-coppice-file-source"], "worktree");
}

fn query_encode(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn git(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .current_dir(dir)
        .output()
        .unwrap_or_else(|err| panic!("git {args:?} failed to start: {err}"))
}

fn git_commit(dir: &std::path::Path, message: &str) {
    let output = git(dir, &["commit", "-m", message]);
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn exact_nested_path_works_without_a_worktree() {
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

    std::fs::create_dir_all(local_path.join("web/src/components")).unwrap();
    std::fs::write(
        local_path.join("web/src/components/AppShell.test.tsx"),
        "export {}\n",
    )
    .unwrap();

    let res = get_file(
        &app,
        &ticket_id,
        &query_encode("web/src/components/AppShell.test.tsx"),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_bytes(res).await, b"export {}\n");
}

#[tokio::test]
async fn resolves_unique_suffix_and_rejects_ambiguous_or_partial_noise() {
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

    std::fs::create_dir_all(local_path.join("web/src/components")).unwrap();
    std::fs::create_dir_all(local_path.join("src")).unwrap();
    std::fs::write(
        local_path.join("web/src/components/AppShell.test.tsx"),
        "shell test\n",
    )
    .unwrap();
    std::fs::write(local_path.join("app.rs"), "root app\n").unwrap();
    std::fs::write(local_path.join("src/app.rs"), "src app\n").unwrap();

    let res = get_file(&app, &ticket_id, "AppShell.test.tsx", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()["x-coppice-file-path"],
        "web%2Fsrc%2Fcomponents%2FAppShell.test.tsx"
    );
    assert_eq!(body_bytes(res).await, b"shell test\n");

    let res = get_file(&app, &ticket_id, "app.rs", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["x-coppice-file-path"], "app.rs");
    assert_eq!(body_bytes(res).await, b"root app\n");

    let res = get_file(&app, &ticket_id, "src%2Fapp.rs", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["x-coppice-file-path"], "src%2Fapp.rs");
    assert_eq!(body_bytes(res).await, b"src app\n");

    let res = get_file(&app, &ticket_id, "pp.rs", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    std::fs::create_dir_all(local_path.join("desktop/src/components")).unwrap();
    std::fs::write(
        local_path.join("desktop/src/components/AppShell.test.tsx"),
        "other shell\n",
    )
    .unwrap();
    let res = get_file(&app, &ticket_id, "AppShell.test.tsx", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn suffix_match_skips_deleted_and_escaping_and_serves_unicode() {
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

    std::fs::write(local_path.join("gone.rs"), "deleted\n").unwrap();
    std::process::Command::new("git")
        .args(["add", "gone.rs"])
        .current_dir(&local_path)
        .output()
        .unwrap();
    git_commit(&local_path, "track gone.rs");
    std::fs::remove_file(local_path.join("gone.rs")).unwrap();

    let res = get_file(&app, &ticket_id, "gone.rs", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    std::fs::create_dir_all(local_path.join("src")).unwrap();
    std::fs::write(local_path.join("src/ok.txt"), "safe\n").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/hostname", local_path.join("src/escape.txt")).unwrap();
        std::os::unix::fs::symlink("/etc/hostname", local_path.join("escape.txt")).unwrap();
        let res = get_file(&app, &ticket_id, "escape.txt", &cookie, &csrf).await;
        assert_ne!(res.status(), StatusCode::OK);
        let res = get_file(&app, &ticket_id, "ok.txt", &cookie, &csrf).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()["x-coppice-file-path"], "src%2Fok.txt");
        assert_eq!(body_bytes(res).await, b"safe\n");
    }

    std::fs::create_dir_all(local_path.join("docs")).unwrap();
    std::fs::write(local_path.join("docs/café.rs"), "fn café() {}\n").unwrap();
    let res = get_file(&app, &ticket_id, &query_encode("café.rs"), &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()["x-coppice-file-path"],
        query_encode("docs/café.rs")
    );
    assert_eq!(body_bytes(res).await, "fn café() {}\n".as_bytes());

    for unsafe_path in ["..%2Fetc%2Fpasswd", "%2Fetc%2Fpasswd", ".git%2Fconfig"] {
        let res = get_file(&app, &ticket_id, unsafe_path, &cookie, &csrf).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{unsafe_path}");
    }
}

#[tokio::test]
async fn suffix_match_serves_a_conflicted_file_once() {
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

    std::fs::create_dir_all(local_path.join("src")).unwrap();
    std::fs::write(local_path.join("src/conflict.rs"), "base\n").unwrap();
    assert!(git(&local_path, &["add", "src/conflict.rs"]).status.success());
    git_commit(&local_path, "add conflict.rs");
    assert!(git(&local_path, &["checkout", "-b", "other"]).status.success());
    std::fs::write(local_path.join("src/conflict.rs"), "other\n").unwrap();
    assert!(git(&local_path, &["add", "src/conflict.rs"]).status.success());
    git_commit(&local_path, "other side");
    assert!(git(&local_path, &["checkout", "main"]).status.success());
    std::fs::write(local_path.join("src/conflict.rs"), "main\n").unwrap();
    assert!(git(&local_path, &["add", "src/conflict.rs"]).status.success());
    git_commit(&local_path, "main side");
    let merge = git(&local_path, &["merge", "other"]);
    assert!(
        !merge.status.success(),
        "expected a merge conflict: {}",
        String::from_utf8_lossy(&merge.stdout)
    );
    let listed = git(&local_path, &["ls-files", "-z", "--cached"]);
    let conflict_rows = listed
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| raw == b"src/conflict.rs")
        .count();
    assert!(
        conflict_rows > 1,
        "git ls-files should list the unmerged path more than once, got {conflict_rows}"
    );

    let res = get_file(&app, &ticket_id, "conflict.rs", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["x-coppice-file-path"], "src%2Fconflict.rs");
}

#[cfg(unix)]
#[tokio::test]
async fn suffix_match_drops_an_escaping_symlink_and_keeps_a_real_file() {
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

    std::fs::create_dir_all(local_path.join("sub")).unwrap();
    std::os::unix::fs::symlink("/etc/hostname", local_path.join("sub/leak.txt")).unwrap();
    let res = get_file(&app, &ticket_id, "leak.txt", &cookie, &csrf).await;
    assert_ne!(res.status(), StatusCode::OK);

    std::fs::create_dir_all(local_path.join("other")).unwrap();
    std::fs::write(local_path.join("other/leak.txt"), "inside\n").unwrap();
    let res = get_file(&app, &ticket_id, "leak.txt", &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["x-coppice-file-path"], "other%2Fleak.txt");
    assert_eq!(body_bytes(res).await, b"inside\n");
}
