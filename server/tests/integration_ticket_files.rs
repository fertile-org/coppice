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
