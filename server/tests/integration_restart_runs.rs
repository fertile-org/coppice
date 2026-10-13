//! After a crash, the next launch waits for a person. The worktree stays.
#![cfg(feature = "embedded-test-db")]

mod common;

use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use coppice_server::copy::restart::{
    comment_for, ACTION_CANCEL, ACTION_DISCARD, ACTION_RESUME, COMMENT_NONE,
};
use coppice_server::serve::sweep_orphaned_runs;
use coppice_server::services::comment_service::CommentService;
use coppice_server::services::run_service::RunService;
use tower::ServiceExt;
use uuid::Uuid;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?} in {}", dir.display());
}

struct LinkedWorktree {
    _root: tempfile::TempDir,
    main: std::path::PathBuf,
    worktree: std::path::PathBuf,
}

fn linked_worktree(dirty_files: &[&str]) -> LinkedWorktree {
    let root = tempfile::tempdir().expect("temp");
    let main = root.path().join("main");
    let worktree = root.path().join("wt");
    std::fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-b", "main"]);
    std::fs::write(main.join("README.md"), "hello\n").unwrap();
    git(&main, &["add", "README.md"]);
    git(&main, &["commit", "-m", "init"]);
    git(
        &main,
        &[
            "worktree",
            "add",
            "-b",
            "agent/ticket",
            worktree.to_str().unwrap(),
        ],
    );
    std::fs::create_dir_all(worktree.join(".agent")).unwrap();
    std::fs::write(worktree.join(".agent").join("context.md"), "runtime\n").unwrap();
    for name in dirty_files {
        std::fs::write(worktree.join(name), "changed\n").unwrap();
    }
    LinkedWorktree {
        _root: root,
        main,
        worktree,
    }
}

fn head_sha(dir: &Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("rev-parse");
    assert!(output.status.success(), "rev-parse in {}", dir.display());
    String::from_utf8(output.stdout)
        .expect("utf-8")
        .trim()
        .to_string()
}

async fn insert_running_run(
    pool: &sqlx::PgPool,
    ticket_id: Uuid,
    agent_id: Uuid,
    worktree: &Path,
    start_sha: &str,
) -> Uuid {
    let run_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
            worktree_path, start_sha
        )
        VALUES ($1, $2, $3, 'work_on_ticket', 'running', 'permissive-default', $4, $5)
        "#,
    )
    .bind(run_id)
    .bind(ticket_id)
    .bind(agent_id)
    .bind(worktree.to_str().unwrap())
    .bind(start_sha)
    .execute(pool)
    .await
    .expect("insert run");
    sqlx::query("UPDATE tickets SET status = 'in_progress' WHERE id = $1")
        .bind(ticket_id)
        .execute(pool)
        .await
        .expect("status");
    run_id
}

struct ServerChild {
    child: Child,
    port: u16,
    log_path: std::path::PathBuf,
}

impl Drop for ServerChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("port");
    listener.local_addr().expect("addr").port()
}

fn spawn_server(database_url: &str, log_dir: &Path) -> ServerChild {
    let port = free_port();
    let log_path = log_dir.join(format!("server-{port}.log"));
    let log = File::create(&log_path).expect("log");
    let bin = env!("CARGO_BIN_EXE_coppice-server");
    let child = Command::new(bin)
        .current_dir(log_dir)
        .env("COPPICE_DATABASE__URL", database_url)
        .env("COPPICE_SERVER__PORT", port.to_string())
        .env("COPPICE_STORAGE__ARTIFACTS_DIR", log_dir.join("artifacts"))
        .env("COPPICE_PLUGINS__DIR", log_dir.join("plugins"))
        .env("COPPICE_SECRETS__MASTER_KEY", "restart-test-master-key")
        .env("RUST_LOG", "info")
        .stdout(Stdio::from(log.try_clone().expect("clone log")))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("spawn coppice-server");
    ServerChild {
        child,
        port,
        log_path,
    }
}

fn health_ok(port: u16) -> bool {
    let address = format!("127.0.0.1:{port}");
    let Ok(mut stream) =
        TcpStream::connect_timeout(&address.parse().expect("addr"), Duration::from_millis(200))
    else {
        return false;
    };
    stream.set_read_timeout(Some(Duration::from_secs(1))).ok();
    if stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    buf.contains("200") && buf.contains("ok")
}

fn wait_health(server: &mut ServerChild) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if health_ok(server.port) {
            return;
        }
        if let Some(status) = server.child.try_wait().expect("try_wait") {
            panic!(
                "server exited {status} before ready\n{}",
                std::fs::read_to_string(&server.log_path).unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!(
        "server did not become ready\n{}",
        std::fs::read_to_string(&server.log_path).unwrap_or_default()
    );
}

fn database_url_for(session_url: &str, name: &str) -> String {
    let (base, _) = session_url
        .rsplit_once('/')
        .expect("database url has a name");
    format!("{base}/{name}")
}

#[tokio::test]
async fn sweep_keeps_the_worktree_and_waits_for_a_choice() {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let agent_id =
        common::create_test_agent_from_preset(&app, "Restart Agent", &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;
    let tree = linked_worktree(&["notes.txt", "extra.txt"]);
    let repo_id =
        common::register_test_repo(&app, tree.main.to_str().unwrap(), &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;

    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    let agent_uuid = Uuid::parse_str(&agent_id).unwrap();
    let run_id = insert_running_run(
        pool,
        ticket_uuid,
        agent_uuid,
        &tree.worktree,
        &head_sha(&tree.worktree),
    )
    .await;

    sweep_orphaned_runs(&state).await;

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "blocked");
    assert!(tree.worktree.join("notes.txt").exists());
    assert!(tree.worktree.join(".agent").join("context.md").exists());

    let comments = CommentService::new(pool)
        .list_by_ticket(ticket_uuid)
        .await
        .expect("comments");
    assert_eq!(
        comments
            .iter()
            .filter(|comment| comment.body == comment_for(2))
            .count(),
        1,
        "{comments:?}"
    );
    let runs = RunService::new(pool)
        .list_for_ticket(ticket_uuid)
        .await
        .expect("runs");
    assert_eq!(runs.len(), 1, "restart must not start another run");
    assert_eq!(
        runs[0].run.error_message.as_deref(),
        Some("interrupted: server restarted during run")
    );

    let restart = common::json_body(
        app.clone()
            .oneshot(common::json_request(
                "GET",
                &format!("/api/tickets/{ticket_id}/restart"),
                "",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(restart["pending"], true);
    assert_eq!(restart["changedFiles"], 2);
    assert_eq!(restart["resumeLabel"], ACTION_RESUME);
    assert_eq!(restart["otherLabel"], ACTION_DISCARD);
    assert_eq!(restart["confirm"], "{{restart.discard_confirm:2}}");

    let active = common::json_body(
        app.clone()
            .oneshot(common::json_request(
                "GET",
                "/api/agent-runs/active",
                "",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(active["active"], false);

    let discarded = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/restart/discard"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(discarded.status(), StatusCode::OK);
    assert_eq!(
        std::fs::read_to_string(tree.worktree.join("README.md")).unwrap(),
        "hello\n"
    );
    assert!(!tree.worktree.join("notes.txt").exists());
    assert!(!tree.worktree.join("extra.txt").exists());
    assert!(tree.worktree.join(".agent").join("context.md").exists());
    assert!(tree.worktree.exists());

    let after = common::json_body(
        app.clone()
            .oneshot(common::json_request(
                "GET",
                &format!("/api/tickets/{ticket_id}/restart"),
                "",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(after["pending"], false);
    let _ = run_id;
}

#[tokio::test]
async fn clean_restart_offers_resume_and_cancel_without_confirm() {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Clean Agent", &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;
    let tree = linked_worktree(&[]);
    let repo_id =
        common::register_test_repo(&app, tree.main.to_str().unwrap(), &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    let agent_uuid = Uuid::parse_str(&agent_id).unwrap();
    insert_running_run(
        pool,
        ticket_uuid,
        agent_uuid,
        &tree.worktree,
        &head_sha(&tree.worktree),
    )
    .await;
    sweep_orphaned_runs(&state).await;

    let comments = CommentService::new(pool)
        .list_by_ticket(ticket_uuid)
        .await
        .expect("comments");
    assert!(comments.iter().any(|comment| comment.body == COMMENT_NONE));

    let restart = common::json_body(
        app.clone()
            .oneshot(common::json_request(
                "GET",
                &format!("/api/tickets/{ticket_id}/restart"),
                "",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(restart["otherLabel"], ACTION_CANCEL);
    assert!(restart.get("confirm").is_none() || restart["confirm"].is_null());

    let resumed = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/restart/resume"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(resumed.status(), StatusCode::OK);
    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "in_progress");
    assert!(tree.worktree.join("README.md").exists());
    let runs = RunService::new(pool)
        .list_for_ticket(ticket_uuid)
        .await
        .expect("runs");
    assert_eq!(runs.len(), 2);
    assert!(runs
        .iter()
        .any(|run| run.run.status == coppice_server::domain::run::RunStatus::Queued));
}

#[tokio::test]
async fn cancel_removes_a_clean_worktree() {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let agent_id =
        common::create_test_agent_from_preset(&app, "Cancel Agent", &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;
    let tree = linked_worktree(&[]);
    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    let agent_uuid = Uuid::parse_str(&agent_id).unwrap();
    insert_running_run(
        pool,
        ticket_uuid,
        agent_uuid,
        &tree.worktree,
        &head_sha(&tree.worktree),
    )
    .await;
    sweep_orphaned_runs(&state).await;

    let cancelled = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/restart/cancel"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(cancelled.status(), StatusCode::OK);
    assert!(!tree.worktree.exists());
    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "blocked");
}

#[tokio::test]
async fn untracked_new_file_is_counted_and_discard_removes_it() {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let agent_id =
        common::create_test_agent_from_preset(&app, "Untracked Agent", &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;
    let tree = linked_worktree(&["notes.txt"]);
    let start = head_sha(&tree.worktree);
    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    let agent_uuid = Uuid::parse_str(&agent_id).unwrap();
    insert_running_run(pool, ticket_uuid, agent_uuid, &tree.worktree, &start).await;
    sweep_orphaned_runs(&state).await;

    let restart = common::json_body(
        app.clone()
            .oneshot(common::json_request(
                "GET",
                &format!("/api/tickets/{ticket_id}/restart"),
                "",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(restart["changedFiles"], 1);

    let discarded = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/restart/discard"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(discarded.status(), StatusCode::OK);
    assert!(!tree.worktree.join("notes.txt").exists());
    assert!(tree.worktree.join(".agent").join("context.md").exists());
    assert_eq!(head_sha(&tree.worktree), start);
}

#[tokio::test]
async fn committed_change_is_counted_and_discard_returns_to_start() {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let agent_id =
        common::create_test_agent_from_preset(&app, "Committed Agent", &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;
    let tree = linked_worktree(&[]);
    let start = head_sha(&tree.worktree);
    std::fs::write(tree.worktree.join("README.md"), "during the run\n").unwrap();
    git(&tree.worktree, &["add", "README.md"]);
    git(&tree.worktree, &["commit", "-m", "before the crash"]);
    assert_ne!(head_sha(&tree.worktree), start);

    let ticket_uuid = Uuid::parse_str(&ticket_id).unwrap();
    let agent_uuid = Uuid::parse_str(&agent_id).unwrap();
    insert_running_run(pool, ticket_uuid, agent_uuid, &tree.worktree, &start).await;
    sweep_orphaned_runs(&state).await;

    let restart = common::json_body(
        app.clone()
            .oneshot(common::json_request(
                "GET",
                &format!("/api/tickets/{ticket_id}/restart"),
                "",
                &cookie,
                &csrf,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(restart["changedFiles"], 1);

    let discarded = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/restart/discard"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(discarded.status(), StatusCode::OK);
    assert_eq!(head_sha(&tree.worktree), start);
    assert_eq!(
        std::fs::read_to_string(tree.worktree.join("README.md")).unwrap(),
        "hello\n"
    );
    assert!(tree.worktree.join(".agent").join("context.md").exists());
}

#[tokio::test]
async fn killing_the_server_mid_run_blocks_on_the_next_launch() {
    let session = coppice_server::db::session_database_url()
        .await
        .expect("session database");
    let name = format!("restart_kill_{}", Uuid::new_v4().simple());
    let admin = sqlx::PgPool::connect(&session).await.expect("admin");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&admin)
        .await
        .expect("create database");
    admin.close().await;
    let url = database_url_for(&session, &name);
    let log_dir = tempfile::tempdir().expect("logs");

    let mut first = spawn_server(&url, log_dir.path());
    wait_health(&mut first);

    let tree = linked_worktree(&["notes.txt"]);
    let pool = sqlx::PgPool::connect(&url).await.expect("case db");
    let board_id = Uuid::new_v4();
    sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
        .bind(board_id)
        .bind("restart")
        .bind(format!("restart-{board_id}"))
        .execute(&pool)
        .await
        .expect("board");
    let agent_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agents (id, name, role, responsibilities, system_prompt, connector)
        VALUES ($1, $2, 'worker', '{}', 'prompt', 'mock')
        "#,
    )
    .bind(agent_id)
    .bind(format!("agent {agent_id}"))
    .execute(&pool)
    .await
    .expect("agent");
    let ticket_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO tickets (id, board_id, title, status, created_by, assignee_agent_id)
        VALUES ($1, $2, 'mid-run', 'in_progress', 'test', $3)
        "#,
    )
    .bind(ticket_id)
    .bind(board_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("ticket");
    let run_id = insert_running_run(
        &pool,
        ticket_id,
        agent_id,
        &tree.worktree,
        &head_sha(&tree.worktree),
    )
    .await;
    pool.close().await;

    first.child.kill().expect("kill server");
    let status = first.child.wait().expect("wait");
    assert!(
        !status.success(),
        "SIGKILL should not look like a clean quit"
    );

    let mut second = spawn_server(&url, log_dir.path());
    wait_health(&mut second);

    let pool = sqlx::PgPool::connect(&url).await.expect("case db");
    let status: String = sqlx::query_scalar("SELECT status FROM tickets WHERE id = $1")
        .bind(ticket_id)
        .fetch_one(&pool)
        .await
        .expect("ticket status");
    assert_eq!(status, "blocked");
    let bodies: Vec<String> =
        sqlx::query_scalar("SELECT body FROM ticket_comments WHERE ticket_id = $1")
            .bind(ticket_id)
            .fetch_all(&pool)
            .await
            .expect("comments");
    assert_eq!(bodies, vec![comment_for(1)]);
    assert!(tree.worktree.join("notes.txt").exists());
    let run_count: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_runs WHERE ticket_id = $1")
        .bind(ticket_id)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(run_count, 1);
    let error: Option<String> =
        sqlx::query_scalar("SELECT error_message FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .expect("run");
    assert_eq!(
        error.as_deref(),
        Some("interrupted: server restarted during run")
    );
    pool.close().await;
    drop(second);
    let admin = sqlx::PgPool::connect(&session).await.expect("admin");
    let _ = sqlx::query(&format!("DROP DATABASE \"{name}\""))
        .execute(&admin)
        .await;
    admin.close().await;
}
