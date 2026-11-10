mod common;

use axum::http::StatusCode;
use coppice_server::domain::git_state::GitOutcome;
use coppice_server::services::human_review_service::HumanReviewService;
use tower::ServiceExt;
use uuid::Uuid;

const SHA: &str = "abc1234abc1234abc1234abc1234abc1234abc12";

async fn setup_ticket() -> (
    std::sync::Arc<coppice_server::AppState>,
    Uuid,
    Uuid,
    common::AgentTestEnv,
) {
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let pool = state.db.clone().expect("pool");
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("user");
    (state, Uuid::parse_str(&ticket_id).unwrap(), user_id, env)
}

#[tokio::test]
async fn reacceptance_clears_git_outcome() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, ticket_id, user_id, _env) = setup_ticket().await;
    let pool = state.db.clone().expect("pool");
    let svc = HumanReviewService::new(&pool);
    svc.record_acceptance(ticket_id, Some(user_id), SHA)
        .await
        .unwrap();
    assert!(svc
        .set_git_outcome(
            ticket_id,
            Some(GitOutcome::Conflict),
            Some("conflict in a.txt")
        )
        .await
        .unwrap());
    let review = svc.get(ticket_id).await.unwrap().unwrap();
    assert_eq!(review.git_outcome, Some(GitOutcome::Conflict));
    assert_eq!(
        review.git_outcome_reason.as_deref(),
        Some("conflict in a.txt")
    );
    assert!(!review.accepted_by_coppice);

    let again = svc.record_acceptance(ticket_id, None, SHA).await.unwrap();
    assert_eq!(again.git_outcome, None);
    assert_eq!(again.git_outcome_reason, None);
    assert_eq!(again.merged_base_branch, None);
    assert!(again.accepted_by_coppice);
    assert_eq!(svc.get(ticket_id).await.unwrap().unwrap().git_outcome, None);
}

#[tokio::test]
async fn set_git_outcome_reports_change() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, ticket_id, user_id, _env) = setup_ticket().await;
    let pool = state.db.clone().expect("pool");
    let svc = HumanReviewService::new(&pool);
    svc.record_acceptance(ticket_id, Some(user_id), SHA)
        .await
        .unwrap();
    assert!(svc
        .set_git_outcome(ticket_id, Some(GitOutcome::MainDirty), None)
        .await
        .unwrap());
    assert!(!svc
        .set_git_outcome(ticket_id, Some(GitOutcome::MainDirty), None)
        .await
        .unwrap());
    assert!(svc.set_git_outcome(ticket_id, None, None).await.unwrap());
}

#[tokio::test]
async fn set_git_outcome_updates_reason_without_reporting_change() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, ticket_id, user_id, _env) = setup_ticket().await;
    let pool = state.db.clone().expect("pool");
    let svc = HumanReviewService::new(&pool);
    svc.record_acceptance(ticket_id, Some(user_id), SHA)
        .await
        .unwrap();
    assert!(svc
        .set_git_outcome(
            ticket_id,
            Some(GitOutcome::Conflict),
            Some("1 file conflicts with main")
        )
        .await
        .unwrap());
    assert!(!svc
        .set_git_outcome(
            ticket_id,
            Some(GitOutcome::Conflict),
            Some("2 files conflict with main")
        )
        .await
        .unwrap());
    let review = svc.get(ticket_id).await.unwrap().unwrap();
    assert_eq!(review.git_outcome, Some(GitOutcome::Conflict));
    assert_eq!(
        review.git_outcome_reason.as_deref(),
        Some("2 files conflict with main")
    );
}

async fn create_repo_json(
    app: &axum::Router,
    remote: Option<&str>,
    cookie: &str,
    csrf: &str,
) -> serde_json::Value {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut body = serde_json::json!({
        "name": "flow-repo",
        "localPath": dir.path().display().to_string(),
        "defaultBranch": "main",
    });
    if let Some(url) = remote {
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
    common::json_body(res).await
}

async fn patch_repo(
    app: &axum::Router,
    id: &str,
    body: serde_json::Value,
    cookie: &str,
    csrf: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/repos/{id}"),
            &body.to_string(),
            cookie,
            csrf,
        ))
        .await
        .unwrap()
}

#[tokio::test]
async fn repo_git_flow_defaults_off() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let repo = create_repo_json(&app, None, &cookie, &csrf).await;
    assert_eq!(repo["mergeAfterAccept"], false);
    assert_eq!(repo["pushAfterMerge"], false);
}

#[tokio::test]
async fn repo_git_flow_patch_roundtrip() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;
    let repo = create_repo_json(
        &app,
        Some("https://github.com/example/repo.git"),
        &cookie,
        &csrf,
    )
    .await;
    let id = repo["id"].as_str().unwrap().to_string();
    assert!(repo["pushAfterMergeDisabledReason"].is_null());

    let res = patch_repo(
        &app,
        &id,
        serde_json::json!({ "mergeAfterAccept": true, "pushAfterMerge": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/repos/{id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let got: serde_json::Value = common::json_body(res).await;
    assert_eq!(got["mergeAfterAccept"], true);
    assert_eq!(got["pushAfterMerge"], true);
}

#[tokio::test]
async fn push_after_merge_rejected_without_remote() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("", |cfg| {
            cfg.git.push_enabled = true;
        })
        .await;
    let repo = create_repo_json(&app, None, &cookie, &csrf).await;
    assert!(repo["pushAfterMergeDisabledReason"].is_string());
    let id = repo["id"].as_str().unwrap().to_string();
    let res = patch_repo(
        &app,
        &id,
        serde_json::json!({ "pushAfterMerge": true }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

struct MergeFixture {
    _git_keep: tempfile::TempDir,
    _worktrees: tempfile::TempDir,
    state: std::sync::Arc<coppice_server::AppState>,
    app: axum::Router,
    cookie: String,
    csrf: String,
    board_id: String,
    ticket_id: String,
    local_path: std::path::PathBuf,
    worktree_path: std::path::PathBuf,
}

/// A repo-linked ticket with a worktree commit, accepted to Done.
async fn accepted_ticket_with_worktree() -> MergeFixture {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let worktrees = tempfile::tempdir().expect("worktrees");
    let path = worktrees.path().to_string_lossy().into_owned();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(move |config| {
        config.agent.worktrees_path = path;
    })
    .await;
    let worktrees_root = std::path::PathBuf::from(&state.config.agent.worktrees_path);
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let worktree_path =
        common::setup_worktree_with_commit(&local_path, &worktrees_root, "test-repo", &ticket_id);
    MergeFixture {
        _git_keep: git_dir,
        _worktrees: worktrees,
        state,
        app,
        cookie,
        csrf,
        board_id,
        ticket_id,
        local_path,
        worktree_path,
    }
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .current_dir(dir)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn post(fx: &MergeFixture, method: &str, uri: &str, body: &str) -> axum::response::Response {
    fx.app
        .clone()
        .oneshot(common::json_request(
            method, uri, body, &fx.cookie, &fx.csrf,
        ))
        .await
        .unwrap()
}

async fn accept(fx: &MergeFixture) {
    let res = post(
        fx,
        "PATCH",
        &format!("/api/tickets/{}/status", fx.ticket_id),
        r#"{"status":"wait_for_final_review"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = post(
        fx,
        "POST",
        &format!("/api/tickets/{}/final-approve", fx.ticket_id),
        "{}",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "final approve");
}

async fn merge_main(fx: &MergeFixture) -> axum::response::Response {
    post(
        fx,
        "POST",
        &format!("/api/tickets/{}/merge-branch", fx.ticket_id),
        r#"{"baseBranch":"main"}"#,
    )
    .await
}

async fn ticket_status(fx: &MergeFixture) -> String {
    let res = post(fx, "GET", &format!("/api/tickets/{}", fx.ticket_id), "").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body["status"].as_str().unwrap_or_default().to_string()
}

async fn review_of(
    fx: &MergeFixture,
) -> coppice_server::services::human_review_service::HumanReview {
    let pool = fx.state.db.clone().expect("pool");
    let id = Uuid::parse_str(&fx.ticket_id).unwrap();
    HumanReviewService::new(&pool)
        .get(id)
        .await
        .unwrap()
        .expect("review")
}

#[tokio::test]
async fn manual_merge_conflict_records_outcome() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = accepted_ticket_with_worktree().await;
    std::fs::write(fx.local_path.join("README.md"), "# test\nmain side\n").unwrap();
    git(&fx.local_path, &["add", "README.md"]);
    git(&fx.local_path, &["commit", "-m", "main conflict"]);
    std::fs::write(fx.worktree_path.join("README.md"), "# test\nfeature side\n").unwrap();
    git(&fx.worktree_path, &["add", "README.md"]);
    git(&fx.worktree_path, &["commit", "-m", "feature conflict"]);
    accept(&fx).await;

    let res = merge_main(&fx).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let review = review_of(&fx).await;
    assert_eq!(review.git_outcome, Some(GitOutcome::Conflict));
    assert_eq!(
        review.git_outcome_reason.as_deref(),
        Some("1 file conflicts with main")
    );
    assert_eq!(ticket_status(&fx).await, "done");
}

#[tokio::test]
async fn manual_merge_dirty_main_records_outcome() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = accepted_ticket_with_worktree().await;
    accept(&fx).await;
    std::fs::write(fx.local_path.join("README.md"), "# test\nuncommitted\n").unwrap();

    let res = merge_main(&fx).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(
        body["message"],
        "main repository has uncommitted changes — commit or stash before merging"
    );

    let review = review_of(&fx).await;
    assert_eq!(review.git_outcome, Some(GitOutcome::MainDirty));
    assert_eq!(
        review.git_outcome_reason.as_deref(),
        Some("Your main checkout has uncommitted changes")
    );
    assert_eq!(ticket_status(&fx).await, "done");

    // A successful retry clears the outcome.
    git(&fx.local_path, &["checkout", "--", "README.md"]);
    let res = merge_main(&fx).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(review_of(&fx).await.git_outcome, None);
}

/// `humanReview.gitState` from the ticket detail, or None before acceptance.
async fn detail_git_state(fx: &MergeFixture) -> Option<String> {
    let res = post(fx, "GET", &format!("/api/tickets/{}", fx.ticket_id), "").await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body["humanReview"]["gitState"].as_str().map(str::to_string)
}

/// `gitState` from git-info, which rechecks first.
async fn info_git_state(fx: &MergeFixture) -> String {
    let res = post(
        fx,
        "GET",
        &format!("/api/tickets/{}/git-info", fx.ticket_id),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body["gitState"].as_str().unwrap_or_default().to_string()
}

async fn wait_for_detail_state(fx: &MergeFixture, want: &str) {
    for _ in 0..100 {
        if detail_git_state(fx).await.as_deref() == Some(want) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!(
        "git state never became {want}: {:?}",
        detail_git_state(fx).await
    );
}

/// Wait for `count` `ticket.updated` events for the fixture's ticket. The merge
/// publishes one; its cleanup publishes another after recording its outcome.
async fn wait_for_ticket_updates(
    rx: &mut tokio::sync::broadcast::Receiver<coppice_server::events::bus::AppEvent>,
    fx: &MergeFixture,
    count: usize,
) {
    use coppice_server::events::bus::AppEvent;
    let want = Uuid::parse_str(&fx.ticket_id).unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut seen = 0;
    while seen < count {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(AppEvent::TicketUpdated { ticket_id, .. })) if ticket_id == want => seen += 1,
            Ok(Ok(_)) => {}
            other => panic!("expected {count} ticket updates, saw {seen}: {other:?}"),
        }
    }
}

#[tokio::test]
async fn board_list_exposes_git_state() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = accepted_ticket_with_worktree().await;
    accept(&fx).await;

    let res = post(
        &fx,
        "GET",
        &format!("/api/boards/{}/tickets", fx.board_id),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    let items = body
        .as_array()
        .or_else(|| body["tickets"].as_array())
        .expect("list");
    let item = items
        .iter()
        .find(|t| t["id"] == fx.ticket_id.as_str())
        .expect("ticket in list");
    assert_eq!(item["humanReview"]["gitState"], "not_merged");
    assert_eq!(item["humanReview"]["acceptedByCoppice"], false);
}

#[tokio::test]
async fn cleanup_publishes_ticket_update() {
    use coppice_server::events::bus::AppEvent;
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = accepted_ticket_with_worktree().await;
    accept(&fx).await;
    let mut rx = fx.state.event_bus.subscribe();

    assert_eq!(merge_main(&fx).await.status(), StatusCode::OK);

    // One update comes from the merge itself, a second from the cleanup.
    let want = Uuid::parse_str(&fx.ticket_id).unwrap();
    let mut updates = 0;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    while updates < 2 {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(AppEvent::TicketUpdated { ticket_id, .. })) if ticket_id == want => {
                updates += 1;
            }
            Ok(Ok(_)) => {}
            other => panic!("no ticket update after cleanup: {other:?}"),
        }
    }
    assert!(!fx.worktree_path.exists());
}

#[tokio::test]
async fn cleanup_kept_worktree_sets_outcome() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = accepted_ticket_with_worktree().await;
    accept(&fx).await;
    let mut rx = fx.state.event_bus.subscribe();
    assert_eq!(merge_main(&fx).await.status(), StatusCode::OK);
    // The merge's own cleanup removes the worktree and then publishes. Bring
    // one back with uncommitted work, then clean up again.
    wait_for_ticket_updates(&mut rx, &fx, 2).await;
    assert!(!fx.worktree_path.exists());
    let worktrees_root = std::path::PathBuf::from(&fx.state.config.agent.worktrees_path);
    common::setup_worktree_with_commit(&fx.local_path, &worktrees_root, "test-repo", &fx.ticket_id);
    std::fs::write(fx.worktree_path.join("scratch.txt"), "uncommitted\n").unwrap();

    let pool = fx.state.db.clone().expect("pool");
    let id = Uuid::parse_str(&fx.ticket_id).unwrap();
    coppice_server::services::worktree_cleanup::cleanup_ticket(
        &pool,
        &fx.state.event_bus,
        worktrees_root,
        id,
        false,
    )
    .await;
    assert_eq!(
        detail_git_state(&fx).await.as_deref(),
        Some("worktree_kept")
    );
    assert_eq!(info_git_state(&fx).await, "worktree_kept");

    std::fs::remove_dir_all(&fx.worktree_path).unwrap();
    assert_eq!(info_git_state(&fx).await, "merged");
    assert_eq!(detail_git_state(&fx).await.as_deref(), Some("merged"));
}

#[tokio::test]
async fn sync_push_clears_not_pushed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let bare = tempfile::tempdir().expect("bare");
    git(bare.path(), &["init", "--bare", "-b", "main"]);
    git(
        &local_path,
        &["remote", "add", "origin", bare.path().to_str().unwrap()],
    );
    git(&local_path, &["push", "-u", "origin", "main"]);
    let worktrees = tempfile::tempdir().expect("worktrees");
    let path = worktrees.path().to_string_lossy().into_owned();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(move |config| {
        config.agent.worktrees_path = path;
        config.git.push_enabled = true;
    })
    .await;
    let worktrees_root = std::path::PathBuf::from(&state.config.agent.worktrees_path);
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    // Metadata only: the checkout's own `origin` is the bare sibling.
    let patched = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/repos/{repo_id}"),
            r#"{"remoteUrl":"https://github.com/example/repo.git"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patched.status(), StatusCode::OK);
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let worktree_path =
        common::setup_worktree_with_commit(&local_path, &worktrees_root, "test-repo", &ticket_id);
    let fx = MergeFixture {
        _git_keep: git_dir,
        _worktrees: worktrees,
        state,
        app,
        cookie,
        csrf,
        board_id,
        ticket_id,
        local_path,
        worktree_path,
    };
    accept(&fx).await;
    let mut rx = fx.state.event_bus.subscribe();
    // The merge leaves main ahead of origin/main.
    assert_eq!(merge_main(&fx).await.status(), StatusCode::OK);
    wait_for_ticket_updates(&mut rx, &fx, 2).await;

    let pool = fx.state.db.clone().expect("pool");
    let id = Uuid::parse_str(&fx.ticket_id).unwrap();
    HumanReviewService::new(&pool)
        .set_git_outcome(id, Some(GitOutcome::NotPushed), Some("Not pushed"))
        .await
        .unwrap();
    assert_eq!(info_git_state(&fx).await, "not_pushed");

    let res = post(
        &fx,
        "POST",
        &format!("/api/repos/{repo_id}/push-default-branch"),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "push");
    assert_eq!(detail_git_state(&fx).await.as_deref(), Some("merged"));
    assert_eq!(review_of(&fx).await.git_outcome, None);
}

#[tokio::test]
async fn not_pushed_is_kept_without_a_remote_tracking_ref() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    // This checkout has no `origin`, so there is no refs/remotes/origin/main.
    let fx = accepted_ticket_with_worktree().await;
    accept(&fx).await;
    let mut rx = fx.state.event_bus.subscribe();
    assert_eq!(merge_main(&fx).await.status(), StatusCode::OK);
    wait_for_ticket_updates(&mut rx, &fx, 2).await;

    let pool = fx.state.db.clone().expect("pool");
    let id = Uuid::parse_str(&fx.ticket_id).unwrap();
    HumanReviewService::new(&pool)
        .set_git_outcome(id, Some(GitOutcome::NotPushed), Some("Not pushed"))
        .await
        .unwrap();
    assert_eq!(info_git_state(&fx).await, "not_pushed");
    assert_eq!(
        review_of(&fx).await.git_outcome,
        Some(GitOutcome::NotPushed)
    );
}

#[tokio::test]
async fn cleanup_does_not_clear_other_outcomes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let fx = accepted_ticket_with_worktree().await;
    accept(&fx).await;
    let pool = fx.state.db.clone().expect("pool");
    let id = Uuid::parse_str(&fx.ticket_id).unwrap();
    HumanReviewService::new(&pool)
        .set_git_outcome(
            id,
            Some(GitOutcome::Conflict),
            Some("1 file conflicts with main"),
        )
        .await
        .unwrap();
    let root = std::path::PathBuf::from(&fx.state.config.agent.worktrees_path);
    coppice_server::services::worktree_cleanup::cleanup_ticket(
        &pool,
        &fx.state.event_bus,
        root,
        id,
        false,
    )
    .await;
    assert_eq!(review_of(&fx).await.git_outcome, Some(GitOutcome::Conflict));
}

// --- The chain: merge → cleanup → push after accept ---

use coppice_server::services::git_chain::{self, ChainOutcome};

/// A repo whose checkout pushes to a bare sibling, plus one ticket with a
/// worktree commit. `remoteUrl` is metadata only; `origin` is the bare repo.
struct ChainFixture {
    fx: MergeFixture,
    _bare: tempfile::TempDir,
    bare_path: std::path::PathBuf,
    repo_id: String,
}

async fn chain_fixture(
    push_enabled: bool,
    merge_after_accept: bool,
    push_after_merge: bool,
) -> ChainFixture {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let bare = tempfile::tempdir().expect("bare");
    git(bare.path(), &["init", "--bare", "-b", "main"]);
    git(
        &local_path,
        &["remote", "add", "origin", bare.path().to_str().unwrap()],
    );
    git(&local_path, &["push", "-u", "origin", "main"]);
    let worktrees = tempfile::tempdir().expect("worktrees");
    let path = worktrees.path().to_string_lossy().into_owned();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(move |config| {
        config.agent.worktrees_path = path;
        config.git.push_enabled = push_enabled;
    })
    .await;
    let worktrees_root = std::path::PathBuf::from(&state.config.agent.worktrees_path);
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let res = patch_repo(
        &app,
        &repo_id,
        serde_json::json!({ "remoteUrl": "https://github.com/example/repo.git" }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = patch_repo(
        &app,
        &repo_id,
        serde_json::json!({
            "mergeAfterAccept": merge_after_accept,
            "pushAfterMerge": push_after_merge,
        }),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "set git flow switches");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let worktree_path =
        common::setup_worktree_with_commit(&local_path, &worktrees_root, "test-repo", &ticket_id);
    ChainFixture {
        fx: MergeFixture {
            _git_keep: git_dir,
            _worktrees: worktrees,
            state,
            app,
            cookie,
            csrf,
            board_id,
            ticket_id,
            local_path,
            worktree_path,
        },
        bare_path: bare.path().to_path_buf(),
        _bare: bare,
        repo_id,
    }
}

fn rev(dir: &std::path::Path, rev: &str) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", rev])
        .current_dir(dir)
        .output()
        .expect("git rev-parse");
    assert!(output.status.success(), "rev-parse {rev} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn current_branch(dir: &std::path::Path) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

async fn run_chain(fx: &MergeFixture) -> ChainOutcome {
    let id = Uuid::parse_str(&fx.ticket_id).unwrap();
    git_chain::run_after_accept(&fx.state, id).await
}

async fn wait_for_worktree_gone(path: &std::path::Path) {
    for _ in 0..200 {
        if !path.exists() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("worktree was never removed: {}", path.display());
}

async fn merge_comment_count(fx: &MergeFixture) -> i64 {
    let pool = fx.state.db.clone().expect("pool");
    let id = Uuid::parse_str(&fx.ticket_id).unwrap();
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM ticket_comments WHERE ticket_id = $1 AND body LIKE '**Merge:**%'",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .expect("count")
}

#[tokio::test]
async fn accept_with_switches_off_does_nothing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(true, false, false).await;
    let remote_before = rev(&cf.bare_path, "main");
    accept(&cf.fx).await;
    assert_eq!(run_chain(&cf.fx).await, ChainOutcome::Skipped);
    assert_eq!(
        detail_git_state(&cf.fx).await.as_deref(),
        Some("not_merged")
    );
    assert_eq!(rev(&cf.bare_path, "main"), remote_before);
    assert!(cf.fx.worktree_path.exists());
    assert_eq!(merge_comment_count(&cf.fx).await, 0);
}

#[tokio::test]
async fn accept_merges_and_pushes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(true, true, true).await;
    let remote_before = rev(&cf.bare_path, "main");
    accept(&cf.fx).await;
    let outcome = run_chain(&cf.fx).await;
    // The chain spawned by accept may have run first; either way the work is done.
    assert!(
        matches!(
            outcome,
            ChainOutcome::MergedAndPushed | ChainOutcome::Skipped
        ),
        "{outcome:?}"
    );
    wait_for_worktree_gone(&cf.fx.worktree_path).await;
    wait_for_detail_state(&cf.fx, "merged").await;
    let local = rev(&cf.fx.local_path, "main");
    assert_ne!(local, remote_before);
    assert_eq!(rev(&cf.bare_path, "main"), local);
    assert_eq!(ticket_status(&cf.fx).await, "done");
    assert_eq!(merge_comment_count(&cf.fx).await, 1);
}

#[tokio::test]
async fn accept_merge_only() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(true, true, false).await;
    let remote_before = rev(&cf.bare_path, "main");
    accept(&cf.fx).await;
    let outcome = run_chain(&cf.fx).await;
    assert!(
        matches!(outcome, ChainOutcome::Merged | ChainOutcome::Skipped),
        "{outcome:?}"
    );
    wait_for_worktree_gone(&cf.fx.worktree_path).await;
    wait_for_detail_state(&cf.fx, "merged").await;
    assert_ne!(rev(&cf.fx.local_path, "main"), remote_before);
    assert_eq!(rev(&cf.bare_path, "main"), remote_before);
}

#[tokio::test]
async fn chain_conflict_leaves_done_flagged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(true, true, true).await;
    let fx = &cf.fx;
    let remote_before = rev(&cf.bare_path, "main");
    std::fs::write(fx.local_path.join("README.md"), "# test\nmain side\n").unwrap();
    git(&fx.local_path, &["add", "README.md"]);
    git(&fx.local_path, &["commit", "-m", "main conflict"]);
    std::fs::write(fx.worktree_path.join("README.md"), "# test\nfeature side\n").unwrap();
    git(&fx.worktree_path, &["add", "README.md"]);
    git(&fx.worktree_path, &["commit", "-m", "feature conflict"]);
    accept(fx).await;
    let outcome = run_chain(fx).await;
    assert!(
        matches!(
            outcome,
            ChainOutcome::Stopped(GitOutcome::Conflict) | ChainOutcome::Skipped
        ),
        "{outcome:?}"
    );
    wait_for_detail_state(fx, "conflict").await;
    assert_eq!(ticket_status(fx).await, "done");
    assert_eq!(
        review_of(fx).await.git_outcome_reason.as_deref(),
        Some("1 file conflicts with main")
    );
    assert_eq!(rev(&cf.bare_path, "main"), remote_before);
    assert!(fx.worktree_path.exists());
}

#[tokio::test]
async fn push_gate_off_records_not_pushed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(false, true, false).await;
    let fx = &cf.fx;
    // The API refuses this switch while push is off; a config change can
    // still close the gate after it was turned on.
    let pool = fx.state.db.clone().expect("pool");
    sqlx::query("UPDATE repos SET push_after_merge = true WHERE id = $1")
        .bind(Uuid::parse_str(&cf.repo_id).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let remote_before = rev(&cf.bare_path, "main");
    accept(fx).await;
    let outcome = run_chain(fx).await;
    assert!(
        matches!(
            outcome,
            ChainOutcome::Stopped(GitOutcome::NotPushed) | ChainOutcome::Skipped
        ),
        "{outcome:?}"
    );
    wait_for_detail_state(fx, "not_pushed").await;
    let review = review_of(fx).await;
    assert_eq!(review.git_outcome, Some(GitOutcome::NotPushed));
    assert!(review
        .git_outcome_reason
        .as_deref()
        .is_some_and(|r| !r.is_empty()));
    assert!(review.merged_at.is_some());
    assert_eq!(rev(&cf.bare_path, "main"), remote_before);
    assert_eq!(ticket_status(fx).await, "done");
}

#[tokio::test]
async fn manual_merge_into_other_branch_never_pushes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(true, false, true).await;
    let fx = &cf.fx;
    git(&fx.local_path, &["branch", "feature-x", "main"]);
    let remote_before = rev(&cf.bare_path, "main");
    accept(fx).await;
    let mut rx = fx.state.event_bus.subscribe();
    let res = post(
        fx,
        "POST",
        &format!("/api/tickets/{}/merge-branch", fx.ticket_id),
        r#"{"baseBranch":"feature-x"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    wait_for_ticket_updates(&mut rx, fx, 2).await;
    assert_eq!(rev(&cf.bare_path, "main"), remote_before);
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "refs/heads/feature-x"])
        .current_dir(&cf.bare_path)
        .output()
        .unwrap();
    assert!(!out.status.success(), "feature-x must not reach the remote");
    assert_eq!(detail_git_state(fx).await.as_deref(), Some("merged"));
    assert_eq!(review_of(fx).await.git_outcome, None);
}

#[tokio::test]
async fn chain_is_serialized_per_repo() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(true, true, true).await;
    let fx = &cf.fx;
    let worktrees_root = std::path::PathBuf::from(&fx.state.config.agent.worktrees_path);
    let second_id = common::create_test_ticket(&fx.app, &fx.board_id, &fx.cookie, &fx.csrf).await;
    common::set_ticket_repo(&fx.app, &second_id, &cf.repo_id, &fx.cookie, &fx.csrf).await;
    let second_worktree = common::setup_worktree_with_commit(
        &fx.local_path,
        &worktrees_root,
        "test-repo",
        &second_id,
    );
    std::fs::write(second_worktree.join("second.txt"), "second\n").unwrap();
    git(&second_worktree, &["add", "second.txt"]);
    git(&second_worktree, &["commit", "-m", "second"]);
    let second = MergeFixture {
        _git_keep: tempfile::tempdir().unwrap(),
        _worktrees: tempfile::tempdir().unwrap(),
        state: fx.state.clone(),
        app: fx.app.clone(),
        cookie: fx.cookie.clone(),
        csrf: fx.csrf.clone(),
        board_id: fx.board_id.clone(),
        ticket_id: second_id.clone(),
        local_path: fx.local_path.clone(),
        worktree_path: second_worktree.clone(),
    };
    accept(fx).await;
    accept(&second).await;

    let first_id = Uuid::parse_str(&fx.ticket_id).unwrap();
    let second_uuid = Uuid::parse_str(&second_id).unwrap();
    let (a, b) = tokio::join!(
        tokio::spawn({
            let state = fx.state.clone();
            async move { git_chain::run_after_accept(&state, first_id).await }
        }),
        tokio::spawn({
            let state = fx.state.clone();
            async move { git_chain::run_after_accept(&state, second_uuid).await }
        }),
    );
    a.unwrap();
    b.unwrap();

    wait_for_detail_state(fx, "merged").await;
    wait_for_detail_state(&second, "merged").await;
    assert_eq!(merge_comment_count(fx).await, 1);
    assert_eq!(merge_comment_count(&second).await, 1);
    let local = rev(&fx.local_path, "main");
    assert_eq!(rev(&cf.bare_path, "main"), local);
    assert!(fx.local_path.join("second.txt").exists());
    assert!(fx.local_path.join("feature.txt").exists());
}

#[tokio::test]
async fn chain_restores_checked_out_branch() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(true, true, false).await;
    let fx = &cf.fx;
    git(&fx.local_path, &["checkout", "-b", "dev"]);
    accept(fx).await;
    run_chain(fx).await;
    wait_for_detail_state(fx, "merged").await;
    assert_eq!(current_branch(&fx.local_path), "dev");
}

/// Done with an acceptance row, without `AcceptService::accept`, so no chain
/// is spawned and the test sees `run_after_accept`'s own outcome.
async fn done_without_spawned_chain(fx: &MergeFixture) {
    let pool = fx.state.db.clone().expect("pool");
    let id = Uuid::parse_str(&fx.ticket_id).unwrap();
    let res = post(
        fx,
        "PATCH",
        &format!("/api/tickets/{}/status", fx.ticket_id),
        r#"{"status":"wait_for_final_review"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let head = rev(&fx.worktree_path, "HEAD");
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("user");
    HumanReviewService::new(&pool)
        .record_acceptance(id, Some(user_id), &head)
        .await
        .unwrap();
    coppice_server::services::ticket_service::TicketService::new(&pool)
        .accept_to_done(id)
        .await
        .unwrap();
    assert_eq!(ticket_status(fx).await, "done");
}

#[tokio::test]
async fn chain_outcome_merged_and_pushed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(true, true, true).await;
    done_without_spawned_chain(&cf.fx).await;
    assert_eq!(run_chain(&cf.fx).await, ChainOutcome::MergedAndPushed);
    assert_eq!(rev(&cf.bare_path, "main"), rev(&cf.fx.local_path, "main"));
    // A second run is a no-op.
    assert_eq!(run_chain(&cf.fx).await, ChainOutcome::Skipped);
    assert_eq!(merge_comment_count(&cf.fx).await, 1);
}

#[tokio::test]
async fn chain_outcome_not_pushed_when_gate_is_off() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(false, true, false).await;
    let pool = cf.fx.state.db.clone().expect("pool");
    sqlx::query("UPDATE repos SET push_after_merge = true WHERE id = $1")
        .bind(Uuid::parse_str(&cf.repo_id).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let remote_before = rev(&cf.bare_path, "main");
    done_without_spawned_chain(&cf.fx).await;
    assert_eq!(
        run_chain(&cf.fx).await,
        ChainOutcome::Stopped(GitOutcome::NotPushed)
    );
    assert_eq!(rev(&cf.bare_path, "main"), remote_before);
    assert_eq!(
        review_of(&cf.fx).await.git_outcome_reason.as_deref(),
        Some("git.push_enabled is false in server config")
    );
}

#[tokio::test]
async fn manual_merge_into_default_branch_pushes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    // Merge after accept off, so the merge is the manual one.
    let cf = chain_fixture(true, false, true).await;
    let remote_before = rev(&cf.bare_path, "main");
    accept(&cf.fx).await;
    assert_eq!(merge_main(&cf.fx).await.status(), StatusCode::OK);
    let local = rev(&cf.fx.local_path, "main");
    assert_ne!(local, remote_before);
    assert_eq!(rev(&cf.bare_path, "main"), local);
    wait_for_worktree_gone(&cf.fx.worktree_path).await;
    wait_for_detail_state(&cf.fx, "merged").await;
}

#[tokio::test]
async fn merge_comment_failure_still_cleans_up_and_pushes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let cf = chain_fixture(true, true, true).await;
    let pool = cf.fx.state.db.clone().expect("pool");
    // Fail only this ticket's merge comment.
    sqlx::query(
        r#"
        CREATE OR REPLACE FUNCTION fail_merge_comment() RETURNS trigger AS $$
        BEGIN
            RAISE EXCEPTION 'merge comment refused';
        END;
        $$ LANGUAGE plpgsql
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "CREATE TRIGGER fail_merge_comment BEFORE INSERT ON ticket_comments FOR EACH ROW \
         WHEN (NEW.ticket_id = '{}' AND NEW.body LIKE '**Merge:**%') \
         EXECUTE FUNCTION fail_merge_comment()",
        cf.fx.ticket_id
    ))
    .execute(&pool)
    .await
    .unwrap();

    done_without_spawned_chain(&cf.fx).await;
    let outcome = run_chain(&cf.fx).await;

    sqlx::query("DROP TRIGGER fail_merge_comment ON ticket_comments")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION fail_merge_comment()")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(outcome, ChainOutcome::MergedAndPushed);
    assert_eq!(merge_comment_count(&cf.fx).await, 0);
    assert_eq!(rev(&cf.bare_path, "main"), rev(&cf.fx.local_path, "main"));
    wait_for_worktree_gone(&cf.fx.worktree_path).await;
    wait_for_detail_state(&cf.fx, "merged").await;
}
