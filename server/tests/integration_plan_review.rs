mod common;

use axum::http::StatusCode;
use coppice_server::copy::plan::{
    NO_PLANNER, PLAN_ALREADY_RUNNING, PLAN_APPROVED_NOTE, PLAN_CHANGES_DISCARDED, PLAN_REQUIRED,
    PLAN_STALE, WORK_NOT_STARTED,
};
use coppice_server::services::planning_service::PlanningService;
use coppice_server::services::worktree_service::compute_paths;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tower::ServiceExt;
use uuid::Uuid;

async fn message(res: axum::response::Response) -> String {
    let body: serde_json::Value = common::json_body(res).await;
    body["message"].as_str().unwrap_or_default().to_string()
}

async fn comments(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> Vec<serde_json::Value> {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/comments"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    body.as_array().cloned().unwrap_or_default()
}

async fn runs(
    app: &axum::Router,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> Vec<serde_json::Value> {
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
    body["runs"].as_array().cloned().unwrap_or_default()
}

fn plan_runs(runs: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    runs.iter()
        .filter(|run| run["jobType"].as_str() == Some("plan_ticket"))
        .collect()
}

#[tokio::test]
async fn drag_and_api_refuse_in_progress_without_an_approved_plan() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;

    let patch = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"in_progress"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(patch).await, PLAN_REQUIRED);

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "backlog");
}

#[tokio::test]
async fn skip_planning_goes_straight_from_ready_to_in_progress() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    common::skip_planning(&app, &ticket_id, &cookie, &csrf).await;

    let direct = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"in_progress"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(direct.status(), StatusCode::OK);

    let other = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::skip_planning(&app, &other, &cookie, &csrf).await;
    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{other}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let ticket = common::get_ticket(&app, &other, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "in_progress");
    assert!(plan_runs(&runs(&app, &other, &cookie, &csrf).await).is_empty());
    let notes = comments(&app, &other, &cookie, &csrf).await;
    assert!(notes.iter().any(|note| note["body"] == WORK_NOT_STARTED));
}

#[tokio::test]
async fn ready_starts_one_plan_run_for_the_assignee() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let engineer_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Backend", &cookie, &csrf)
            .await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &engineer_id, &cookie, &csrf).await;

    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let again = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::OK);

    let listed = runs(&app, &ticket_id, &cookie, &csrf).await;
    let plans = plan_runs(&listed);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0]["agentId"], engineer_id);
    assert_ne!(plans[0]["agentId"], pm_id);
}

#[tokio::test]
async fn ready_without_a_planner_stays_ready_and_says_why() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;

    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "ready");
    let notes = comments(&app, &ticket_id, &cookie, &csrf).await;
    assert!(notes.iter().any(|note| note["body"] == NO_PLANNER));
    assert!(plan_runs(&runs(&app, &ticket_id, &cookie, &csrf).await).is_empty());
}

#[tokio::test]
async fn ready_uses_the_assignee_when_there_is_no_pm() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let engineer_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Backend", &cookie, &csrf)
            .await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &engineer_id, &cookie, &csrf).await;

    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let listed = runs(&app, &ticket_id, &cookie, &csrf).await;
    let plans = plan_runs(&listed);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0]["agentId"], engineer_id);
}

const CANONICAL_PLAN: &str = "## Plan\n\n### Approach\nShip the parser first.\n\n### Steps\n- [ ] Add the column\n- [ ] Gate In Progress\n\n### Risks\nNone.\n\n### Out of scope\nNone.\n";

async fn post_plan(
    app: &axum::Router,
    ticket_id: &str,
    agent_id: Uuid,
    pool: &sqlx::PgPool,
    cookie: &str,
    csrf: &str,
) {
    PlanningService::new(pool)
        .record_plan(
            Uuid::parse_str(ticket_id).unwrap(),
            agent_id,
            CANONICAL_PLAN,
        )
        .await
        .expect("record plan");
    let moved = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"plan_review"}"#,
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(moved.status(), StatusCode::OK);
}

#[tokio::test]
async fn ask_for_changes_revises_the_plan_and_refuses_a_second_request() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &pm_id, &cookie, &csrf).await;
    let pool = state.db.as_ref().expect("db");
    post_plan(
        &app,
        &ticket_id,
        Uuid::parse_str(&pm_id).unwrap(),
        pool,
        &cookie,
        &csrf,
    )
    .await;

    let ask = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/plan-changes"),
            r#"{"comment":"Drop the migration."}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ask.status(), StatusCode::OK);
    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "plan_review");
    let thread = comments(&app, &ticket_id, &cookie, &csrf).await;
    assert!(thread
        .iter()
        .any(|note| note["body"] == "Drop the migration."));
    assert_eq!(
        plan_runs(&runs(&app, &ticket_id, &cookie, &csrf).await).len(),
        1
    );

    let again = app
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/plan-changes"),
            r#"{"comment":"Again."}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(again).await, PLAN_ALREADY_RUNNING);
}

#[tokio::test]
async fn approve_posts_the_note_and_the_review_checklist() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let pool = state.db.as_ref().expect("db");
    let pm_uuid = Uuid::parse_str(&pm_id).unwrap();
    post_plan(&app, &ticket_id, pm_uuid, pool, &cookie, &csrf).await;

    let edited = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            r#"{"title":"Renamed after the plan"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(edited.status(), StatusCode::OK);
    let stale = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/approve-plan"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::BAD_REQUEST);
    assert_eq!(message(stale).await, PLAN_STALE);

    post_plan(&app, &ticket_id, pm_uuid, pool, &cookie, &csrf).await;
    let approve = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/approve-plan"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);
    let approved = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(approved["status"], "in_progress");
    let notes = comments(&app, &ticket_id, &cookie, &csrf).await;
    assert!(notes.iter().any(|note| note["body"] == PLAN_APPROVED_NOTE));

    let review = app
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
    assert_eq!(review.status(), StatusCode::OK);
    let shown = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(
        shown["approvedPlan"]["steps"],
        serde_json::json!(["Add the column", "Gate In Progress"])
    );
}

#[tokio::test]
async fn a_planning_run_posts_the_plan_and_moves_to_plan_review() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &pm_id, &cookie, &csrf).await;

    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let ticket = common::poll_ticket_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "plan review",
        Duration::from_secs(15),
        |ticket| ticket["status"] == "plan_review",
    )
    .await;
    assert_eq!(ticket["status"], "plan_review");
    let thread = comments(&app, &ticket_id, &cookie, &csrf).await;
    let plan = thread
        .iter()
        .find(|note| note["intent"] == "plan")
        .expect("plan comment");
    let body = plan["body"].as_str().unwrap();
    assert!(body.contains("## Plan"));
    assert!(body.contains("- [ ] Add the column"));
    assert_eq!(
        plan_runs(&runs(&app, &ticket_id, &cookie, &csrf).await).len(),
        1
    );
    assert!(thread
        .iter()
        .all(|note| note["body"] != PLAN_CHANGES_DISCARDED));
}

fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

struct TicketGit {
    _repo_dir: tempfile::TempDir,
    repo: PathBuf,
    worktree: PathBuf,
    main_sha: String,
    branch_sha: String,
}

fn snapshot_ticket_git(git: &TicketGit) {
    assert_eq!(git_stdout(&git.repo, &["rev-parse", "main"]), git.main_sha);
    let branch = git_stdout(&git.worktree, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(
        git_stdout(&git.repo, &["rev-parse", &branch]),
        git.branch_sha
    );
    assert_eq!(
        git_stdout(&git.worktree, &["rev-parse", "HEAD"]),
        git.branch_sha
    );
    assert!(git.worktree.join("feature.txt").is_file());
    assert!(!git.worktree.join("plan-leak.txt").exists());
    assert!(!git.repo.join("plan-leak.txt").exists());
    assert!(git_stdout(&git.worktree, &["status", "--porcelain"]).is_empty());
    assert!(git_stdout(&git.repo, &["status", "--porcelain"]).is_empty());
    let listed = git_stdout(&git.repo, &["worktree", "list"]);
    assert!(!listed.contains("plan-scratch"));
}

async fn open_planned_repo(
    app: &axum::Router,
    state: &coppice_server::AppState,
    preset_source: &str,
    cookie: &str,
    csrf: &str,
) -> (String, TicketGit) {
    let board_id = common::create_test_board(app, cookie, csrf).await;
    let (repo_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(app, &local_path.display().to_string(), cookie, csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(app, "backend_engineer", "Planner", cookie, csrf)
            .await;
    let pool = state.db.as_ref().expect("db");
    sqlx::query("UPDATE agents SET preset_source = $2 WHERE id = $1")
        .bind(Uuid::parse_str(&agent_id).expect("agent uuid"))
        .bind(preset_source)
        .execute(pool)
        .await
        .expect("point planner at the plan fixture");
    let ticket_id = common::create_unplanned_ticket(app, &board_id, cookie, csrf).await;
    common::set_ticket_repo(app, &ticket_id, &repo_id, cookie, csrf).await;
    common::assign_agent_to_ticket(app, &ticket_id, &agent_id, cookie, csrf).await;
    let worktrees = PathBuf::from(&state.config.agent.worktrees_path);
    let worktree =
        common::setup_worktree_with_commit(&local_path, &worktrees, "test-repo", &ticket_id);
    let paths = compute_paths(
        &worktrees,
        "test-repo",
        Uuid::parse_str(&ticket_id).expect("ticket uuid"),
    );
    let git = TicketGit {
        main_sha: git_stdout(&local_path, &["rev-parse", "main"]),
        branch_sha: git_stdout(&local_path, &["rev-parse", &paths.branch_name]),
        worktree,
        repo: local_path,
        _repo_dir: repo_dir,
    };
    assert_ne!(git.main_sha, git.branch_sha);
    (ticket_id, git)
}

async fn scratch_for(
    app: &axum::Router,
    worktrees: &Path,
    ticket_id: &str,
    cookie: &str,
    csrf: &str,
) -> PathBuf {
    let listed = common::poll_runs_until_count(
        app,
        ticket_id,
        cookie,
        csrf,
        "plan run id",
        Duration::from_secs(15),
        |runs| plan_runs(runs).len() == 1,
    )
    .await;
    let run_id = plan_runs(&listed)[0]["id"].as_str().expect("run id");
    worktrees.join("plan-scratch").join(run_id)
}

async fn wait_for_path(label: &str, timeout: Duration, present: bool, path: &Path) {
    let deadline = Instant::now() + timeout;
    loop {
        if path.exists() == present {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {label}; exists={}", path.exists());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn assert_discard_note(app: &axum::Router, ticket_id: &str, cookie: &str, csrf: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let notes = comments(app, ticket_id, cookie, csrf).await;
        if notes
            .iter()
            .any(|note| note["body"] == PLAN_CHANGES_DISCARDED)
        {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for the discarded-changes note");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn plan_run_discards_scratch_writes_and_leaves_the_ticket_untouched() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let (ticket_id, git) = open_planned_repo(&app, &state, "plan_writer", &cookie, &csrf).await;
    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let ticket = common::poll_ticket_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "plan review after a writing plan run",
        Duration::from_secs(20),
        |ticket| ticket["status"] == "plan_review",
    )
    .await;
    assert_eq!(ticket["status"], "plan_review");
    let worktrees = PathBuf::from(&state.config.agent.worktrees_path);
    let scratch = scratch_for(&app, &worktrees, &ticket_id, &cookie, &csrf).await;
    assert!(!scratch.starts_with(&git.worktree));
    wait_for_path("scratch removal", Duration::from_secs(15), false, &scratch).await;
    assert_discard_note(&app, &ticket_id, &cookie, &csrf).await;
    let listed = runs(&app, &ticket_id, &cookie, &csrf).await;
    let plan = plan_runs(&listed);
    assert_eq!(plan.len(), 1);
    assert!(plan[0]["worktreePath"].is_null());
    snapshot_ticket_git(&git);
}

#[tokio::test]
async fn plan_run_removes_scratch_when_the_run_fails() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let (ticket_id, git) =
        open_planned_repo(&app, &state, "plan_writer_fail", &cookie, &csrf).await;
    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let listed = common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "failed plan run",
        Duration::from_secs(20),
        |runs| {
            plan_runs(runs)
                .iter()
                .any(|run| run["status"].as_str() == Some("failed"))
        },
    )
    .await;
    let run_id = plan_runs(&listed)[0]["id"].as_str().expect("run id");
    let scratch = PathBuf::from(&state.config.agent.worktrees_path)
        .join("plan-scratch")
        .join(run_id);
    wait_for_path(
        "failed scratch removal",
        Duration::from_secs(15),
        false,
        &scratch,
    )
    .await;
    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "ready");
    assert_discard_note(&app, &ticket_id, &cookie, &csrf).await;
    snapshot_ticket_git(&git);
}

#[tokio::test]
async fn plan_run_removes_scratch_when_the_run_is_cancelled() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let (ticket_id, git) =
        open_planned_repo(&app, &state, "plan_writer_slow", &cookie, &csrf).await;
    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let worktrees = PathBuf::from(&state.config.agent.worktrees_path);
    let scratch = scratch_for(&app, &worktrees, &ticket_id, &cookie, &csrf).await;
    let leak = scratch.join("plan-leak.txt");
    wait_for_path("scratch write", Duration::from_secs(15), true, &leak).await;
    let run_id = scratch.file_name().unwrap().to_string_lossy().to_string();
    let stop = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/agent-runs/{run_id}/stop"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(stop.status(), StatusCode::OK);
    wait_for_path(
        "cancelled scratch removal",
        Duration::from_secs(15),
        false,
        &scratch,
    )
    .await;
    let listed = common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "cancelled plan run",
        Duration::from_secs(15),
        |runs| {
            plan_runs(runs)
                .iter()
                .any(|run| run["status"].as_str() == Some("cancelled"))
        },
    )
    .await;
    assert_eq!(plan_runs(&listed)[0]["status"], "cancelled");
    assert_discard_note(&app, &ticket_id, &cookie, &csrf).await;
    snapshot_ticket_git(&git);
}
