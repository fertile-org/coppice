mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use axum::http::StatusCode;
use axum::Router;
use coppice_server::services::human_review_service::HumanReviewService;
use coppice_server::services::worktree_service::compute_paths;
use coppice_server::AppState;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

struct Split {
    state: Arc<AppState>,
    app: Router,
    cookie: String,
    csrf: String,
    board_id: String,
    parent_id: Uuid,
    children: Vec<Uuid>,
}

async fn post(
    app: &Router,
    path: &str,
    body: &str,
    cookie: &str,
    csrf: &str,
) -> axum::http::Response<axum::body::Body> {
    app.clone()
        .oneshot(common::json_request("POST", path, body, cookie, csrf))
        .await
        .unwrap()
}

async fn approved_split(titles: &[&str]) -> Split {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Planner", &cookie, &csrf)
            .await;
    let parent = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let parent_id = Uuid::parse_str(&parent).unwrap();
    let agent = Uuid::parse_str(&agent_id).unwrap();
    let pool = state.db.clone().unwrap();
    let status = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{parent}/status"),
            r#"{"status":"plan_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK, "plan review");

    let plan_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO ticket_plans (id, ticket_id, revision, body, content_version)
        VALUES ($1, $2, 1, '## Plan', 1)
        "#,
    )
    .bind(plan_id)
    .bind(parent_id)
    .execute(&pool)
    .await
    .unwrap();

    let splits: Vec<Value> = titles
        .iter()
        .map(|title| json!({ "title": title, "description": format!("Do {title}.") }))
        .collect();
    let pending = json!({
        "recommendedByAgentId": agent,
        "recommendedAt": "2026-01-01T00:00:00Z",
        "fromPlan": true,
        "splits": splits,
    });
    sqlx::query("UPDATE tickets SET pending_split_recommendation = $2 WHERE id = $1")
        .bind(parent_id)
        .bind(pending)
        .execute(&pool)
        .await
        .unwrap();

    let approved = post(
        &app,
        &format!("/api/tickets/{parent}/approve-plan"),
        "{}",
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(approved.status(), StatusCode::OK, "approve plan");

    let children: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id FROM tickets
        WHERE parent_ticket_id = $1
        ORDER BY array_position(
            (SELECT child_order FROM ticket_plan_splits WHERE parent_ticket_id = $1),
            id
        )
        "#,
    )
    .bind(parent_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(children.len(), titles.len());
    Split {
        state,
        app,
        cookie,
        csrf,
        board_id,
        parent_id,
        children,
    }
}

fn status_of(ticket: &Value, id: Uuid) -> String {
    ticket["splitProgress"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|child| child["id"] == id.to_string())
        .unwrap()["status"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn child_status(split: &Split, id: Uuid) -> String {
    let ticket = common::get_ticket(
        &split.app,
        &split.parent_id.to_string(),
        &split.cookie,
        &split.csrf,
    )
    .await;
    status_of(&ticket, id)
}

async fn mark_accepted(split: &Split, child: Uuid) {
    let pool = split.state.db.clone().unwrap();
    sqlx::query("UPDATE tickets SET status = 'done' WHERE id = $1")
        .bind(child)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ticket_human_reviews (ticket_id, head_sha) VALUES ($1, 'abc123')")
        .bind(child)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn approving_a_split_plan_starts_child_one_and_reports_progress() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let split = approved_split(&["One", "Two"]).await;
    let parent = common::get_ticket(
        &split.app,
        &split.parent_id.to_string(),
        &split.cookie,
        &split.csrf,
    )
    .await;
    assert_eq!(parent["status"], "in_progress");
    let progress = &parent["splitProgress"];
    assert_eq!(progress["autoStart"], true);
    assert_eq!(progress["activeChildId"], split.children[0].to_string());
    assert_eq!(progress["nextActor"], "agent");
    assert_eq!(progress["children"][0]["title"], "One");
    assert_eq!(progress["children"][0]["status"], "ready");
    assert_eq!(progress["children"][0]["position"], 1);
    assert_eq!(progress["children"][1]["title"], "Two");
    assert_eq!(progress["children"][1]["status"], "backlog");

    let listed = split
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/boards/{}/tickets", split.board_id),
            "",
            &split.cookie,
            &split.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed: Value = common::json_body(listed).await;
    let cards = listed.as_array().unwrap();
    let parent_card = cards
        .iter()
        .find(|card| card["id"] == split.parent_id.to_string())
        .unwrap();
    assert_eq!(
        parent_card["splitProgress"]["activeChildId"],
        split.children[0].to_string()
    );
    assert_eq!(
        parent_card["splitProgress"]["children"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(parent_card["splitProgress"]["nextActor"], "agent");
    let child_card = cards
        .iter()
        .find(|card| card["id"] == split.children[0].to_string())
        .unwrap();
    assert_eq!(
        child_card["splitProgress"]["parentTicketId"],
        split.parent_id.to_string()
    );
    assert_eq!(child_card["splitProgress"]["position"], 1);
    assert_eq!(
        child_card["splitProgress"]["activeChildId"],
        split.children[0].to_string()
    );

    // A fetch of one child must still see every sibling, or position and
    // count collapse to 1 of 1.
    let second = common::get_ticket(
        &split.app,
        &split.children[1].to_string(),
        &split.cookie,
        &split.csrf,
    )
    .await;
    let second_progress = &second["splitProgress"];
    assert_eq!(second_progress["position"], 2);
    assert_eq!(second_progress["count"], 2);
    assert_eq!(second_progress["children"][0]["position"], 1);
    assert_eq!(second_progress["children"][1]["id"], split.children[1].to_string());
    assert_eq!(second_progress["children"][1]["position"], 2);
}

#[tokio::test]
async fn pause_blocked_and_merged_elsewhere_control_auto_start() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let split = approved_split(&["One", "Two", "Three"]).await;
    let parent = split.parent_id;
    let paused = post(
        &split.app,
        &format!("/api/tickets/{parent}/split/pause"),
        "{}",
        &split.cookie,
        &split.csrf,
    )
    .await;
    assert_eq!(paused.status(), StatusCode::OK);
    let paused: Value = common::json_body(paused).await;
    assert_eq!(paused["splitProgress"]["autoStart"], false);

    mark_accepted(&split, split.children[0]).await;
    let elsewhere = post(
        &split.app,
        &format!("/api/tickets/{parent}/split/merged-elsewhere"),
        "{}",
        &split.cookie,
        &split.csrf,
    )
    .await;
    let elsewhere_status = elsewhere.status();
    let elsewhere: Value = common::json_body(elsewhere).await;
    assert_eq!(
        elsewhere_status,
        StatusCode::OK,
        "merged elsewhere: {elsewhere}"
    );
    assert_eq!(elsewhere["splitProgress"]["nextActor"], "you");
    let pool = split.state.db.clone().unwrap();
    let merged: bool = sqlx::query_scalar(
        "SELECT merged_at IS NOT NULL FROM ticket_human_reviews WHERE ticket_id = $1",
    )
    .bind(split.children[0])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(merged, "merged elsewhere records the merge");
    assert_eq!(child_status(&split, split.children[1]).await, "backlog");

    let resumed = post(
        &split.app,
        &format!("/api/tickets/{parent}/split/resume"),
        "{}",
        &split.cookie,
        &split.csrf,
    )
    .await;
    assert_eq!(resumed.status(), StatusCode::OK);
    assert_eq!(child_status(&split, split.children[1]).await, "ready");

    let blocked = split
        .app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{}/status", split.children[1]),
            r#"{"status":"blocked"}"#,
            &split.cookie,
            &split.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::OK, "block child 2");
    HumanReviewService::new(&pool)
        .record_merge(split.children[0], "abc123")
        .await
        .unwrap();
    assert_eq!(child_status(&split, split.children[2]).await, "backlog");

    let started = post(
        &split.app,
        &format!("/api/tickets/{parent}/split/start-next"),
        "{}",
        &split.cookie,
        &split.csrf,
    )
    .await;
    assert_eq!(started.status(), StatusCode::OK, "start next");
    assert_eq!(child_status(&split, split.children[2]).await, "ready");
    assert_eq!(child_status(&split, split.children[1]).await, "blocked");
}

#[tokio::test]
async fn an_archived_child_is_skipped() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let split = approved_split(&["One", "Two", "Three"]).await;
    let archived = post(
        &split.app,
        &format!("/api/tickets/{}/archive", split.children[1]),
        "{}",
        &split.cookie,
        &split.csrf,
    )
    .await;
    assert_eq!(archived.status(), StatusCode::OK);
    mark_accepted(&split, split.children[0]).await;
    HumanReviewService::new(split.state.db.as_ref().unwrap())
        .record_merge(split.children[0], "abc123")
        .await
        .unwrap();
    assert_eq!(child_status(&split, split.children[2]).await, "ready");
}

struct GitSplit {
    split: Split,
    _git_keep: tempfile::TempDir,
    local_path: PathBuf,
    _env: common::AgentTestEnv,
}

async fn approved_split_with_repo(titles: &[&str]) -> GitSplit {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Planner", &cookie, &csrf)
            .await;
    let agent = Uuid::parse_str(&agent_id).unwrap();
    let pool = state.db.clone().unwrap();
    sqlx::query("UPDATE agents SET role = 'PM', preset_source = 'pm' WHERE id = $1")
        .bind(agent)
        .execute(&pool)
        .await
        .unwrap();

    let parent = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &parent, &repo_id, &cookie, &csrf).await;
    let parent_id = Uuid::parse_str(&parent).unwrap();
    let status = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{parent}/status"),
            r#"{"status":"plan_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);

    sqlx::query(
        r#"
        INSERT INTO ticket_plans (id, ticket_id, revision, body, content_version)
        VALUES ($1, $2, 1, '## Plan', 1)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(parent_id)
    .execute(&pool)
    .await
    .unwrap();
    let splits: Vec<Value> = titles
        .iter()
        .map(|title| json!({ "title": title, "description": format!("Do {title}.") }))
        .collect();
    sqlx::query("UPDATE tickets SET pending_split_recommendation = $2 WHERE id = $1")
        .bind(parent_id)
        .bind(json!({
            "recommendedByAgentId": agent,
            "recommendedAt": "2026-01-01T00:00:00Z",
            "fromPlan": true,
            "splits": splits,
        }))
        .execute(&pool)
        .await
        .unwrap();
    let approved = post(
        &app,
        &format!("/api/tickets/{parent}/approve-plan"),
        "{}",
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(approved.status(), StatusCode::OK, "approve");
    let children: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id FROM tickets WHERE parent_ticket_id = $1
        ORDER BY created_at, id
        "#,
    )
    .bind(parent_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    GitSplit {
        split: Split {
            state,
            app,
            cookie,
            csrf,
            board_id,
            parent_id,
            children,
        },
        _git_keep: git_dir,
        local_path,
        _env: env,
    }
}

fn worktree_with_file(git_dir: &Path, worktrees: &Path, ticket_id: Uuid, file: &str) -> PathBuf {
    let paths = compute_paths(worktrees, "test-repo", ticket_id);
    std::fs::create_dir_all(&paths.worktree_dir).unwrap();
    let added = Command::new("git")
        .args([
            "worktree",
            "add",
            "-B",
            &paths.branch_name,
            paths.worktree_dir.to_str().unwrap(),
            "main",
        ])
        .current_dir(git_dir)
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    std::fs::write(paths.worktree_dir.join(file), "change\n").unwrap();
    let add = Command::new("git")
        .args(["add", file])
        .current_dir(&paths.worktree_dir)
        .output()
        .unwrap();
    assert!(add.status.success(), "git add");
    let commit = Command::new("git")
        .args(["commit", "-m", file])
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .current_dir(&paths.worktree_dir)
        .output()
        .unwrap();
    assert!(
        commit.status.success(),
        "commit: {}",
        String::from_utf8_lossy(&commit.stderr)
    );
    paths.worktree_dir
}

async fn accept_and_merge(git: &GitSplit, child: Uuid) {
    let id = child.to_string();
    let moved = git
        .split
        .app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{id}/status"),
            r#"{"status":"wait_for_final_review"}"#,
            &git.split.cookie,
            &git.split.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(moved.status(), StatusCode::OK, "wait for review");
    let accepted = post(
        &git.split.app,
        &format!("/api/tickets/{id}/final-approve"),
        "{}",
        &git.split.cookie,
        &git.split.csrf,
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::OK, "accept");
    let merged = post(
        &git.split.app,
        &format!("/api/tickets/{id}/merge-branch"),
        r#"{"baseBranch":"main"}"#,
        &git.split.cookie,
        &git.split.csrf,
    )
    .await;
    let merged_status = merged.status();
    let merged: Value = common::json_body(merged).await;
    assert_eq!(merged_status, StatusCode::OK, "merge {merged}");
}

#[tokio::test]
async fn merge_button_starts_the_next_child_and_the_last_starts_verification() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let git = approved_split_with_repo(&["One", "Two"]).await;
    let worktrees = PathBuf::from(&git.split.state.config.agent.worktrees_path);
    assert_eq!(
        child_status(&git.split, git.split.children[0]).await,
        "ready"
    );
    assert_eq!(
        child_status(&git.split, git.split.children[1]).await,
        "backlog"
    );

    worktree_with_file(
        &git.local_path,
        &worktrees,
        git.split.children[0],
        "one.txt",
    );
    accept_and_merge(&git, git.split.children[0]).await;
    assert_eq!(
        child_status(&git.split, git.split.children[1]).await,
        "ready"
    );

    worktree_with_file(
        &git.local_path,
        &worktrees,
        git.split.children[1],
        "two.txt",
    );
    accept_and_merge(&git, git.split.children[1]).await;

    let pool = git.split.state.db.clone().unwrap();
    let runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM agent_runs WHERE ticket_id = $1 AND job_type = 'verify_parent'",
    )
    .bind(git.split.parent_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(runs, 1, "last merge starts parent verification");
}
