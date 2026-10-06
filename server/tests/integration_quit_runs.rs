//! Quitting Coppice mid-run must not leave the ticket In Progress.
#![cfg(feature = "embedded-test-db")]

mod common;

use std::path::Path;
use std::time::Duration;

use coppice_server::domain::substatus::{Substatus, TicketStatus};
use coppice_server::domain::ticket::status_to_str;
use coppice_server::process_tree::{self, process_alive, SpawnMeta};
use coppice_server::serve::{
    shutdown_agent_sessions, sweep_orphaned_runs, GRACEFUL_QUIT_REASON, RUN_STOPPED_BECAUSE_QUIT,
};
use coppice_server::services::comment_service::CommentService;
use coppice_server::services::run_service::RunService;
use coppice_server::services::ticket_service::TicketService;
use tokio::process::Command;
use tokio::sync::Mutex;
use uuid::Uuid;

static QUIT_TEST_LOCK: Mutex<()> = Mutex::const_new(());

struct KillGroup(u32);

impl Drop for KillGroup {
    fn drop(&mut self) {
        if self.0 > 1 {
            // procps-ng 4 treats `kill -KILL -PGID` as a flag, so the group stays
            // alive. `--` makes the negative id an operand.
            let _ = std::process::Command::new("kill")
                .args(["-s", "KILL", "--", &format!("-{}", self.0)])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

async fn insert_running_ticket(pool: &sqlx::PgPool, status: &str) -> (Uuid, Uuid) {
    let board_id = Uuid::new_v4();
    sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
        .bind(board_id)
        .bind("quit board")
        .bind(format!("quit-{board_id}"))
        .execute(pool)
        .await
        .expect("insert board");

    let agent_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agents (
            id, name, role, skills, responsibilities, system_prompt, connector
        )
        VALUES ($1, $2, 'worker', '{}', '{}', 'prompt', 'mock')
        "#,
    )
    .bind(agent_id)
    .bind(format!("quit agent {agent_id}"))
    .execute(pool)
    .await
    .expect("insert agent");

    let ticket_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO tickets (
            id, board_id, title, status, created_by, assignee_agent_id
        )
        VALUES ($1, $2, 'mid-run', $3, 'test', $4)
        "#,
    )
    .bind(ticket_id)
    .bind(board_id)
    .bind(status)
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("insert ticket");

    let run_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, agent_id, job_type, status, sandbox_profile_id
        )
        VALUES ($1, $2, $3, 'work_on_ticket', 'running', 'permissive-default')
        "#,
    )
    .bind(run_id)
    .bind(ticket_id)
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("insert run");

    (ticket_id, run_id)
}

async fn spawn_tracked_sleeper(dir: &Path, run_id: Uuid) -> (u32, u32) {
    let pid_file = dir.join("child.pid");
    let script = format!(
        "sleep 1000 >/dev/null 2>&1 & echo $! > '{}'; wait",
        pid_file.display()
    );
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let tracked = process_tree::active()
        .spawn(
            &mut cmd,
            SpawnMeta {
                command: "sh".into(),
                label: "cli",
                run_id: Some(run_id.to_string()),
                task_log_dir: None,
            },
        )
        .expect("spawn agent tree");
    let leader = tracked.id();
    tracked.abandon();
    let child = read_child_pid(&pid_file).await;
    (leader, child)
}

async fn read_child_pid(path: &Path) -> u32 {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(pid) = text.trim().parse::<u32>() {
                return pid;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("child pid file was not written: {}", path.display());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_dead(pid: u32) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if !process_alive(pid) {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

async fn assert_blocked_with_quit_comment(
    pool: &sqlx::PgPool,
    ticket_id: Uuid,
    run_id: Uuid,
    reason: &str,
) {
    let ticket = TicketService::new(pool)
        .get(ticket_id)
        .await
        .expect("ticket");
    assert!(
        !matches!(
            ticket.ticket.status,
            TicketStatus::InProgress | TicketStatus::InReview | TicketStatus::InQa
        ),
        "ticket stayed {}",
        status_to_str(ticket.ticket.status)
    );
    assert_eq!(ticket.ticket.status, TicketStatus::Blocked);
    assert_eq!(ticket.ticket.substatus, Some(Substatus::BlockedByError));

    let comments = CommentService::new(pool)
        .list_by_ticket(ticket_id)
        .await
        .expect("comments");
    assert!(
        comments
            .iter()
            .any(|comment| comment.body == RUN_STOPPED_BECAUSE_QUIT),
        "{comments:?}"
    );

    let run = RunService::new(pool).get(run_id).await.expect("run");
    assert_eq!(
        run.error_message.as_deref(),
        Some(format!("interrupted: {reason}").as_str())
    );
}

#[tokio::test]
async fn graceful_shutdown_and_crash_reap_block_in_progress_tickets() {
    let _guard = QUIT_TEST_LOCK.lock().await;
    let state = common::bootstrap_and_login_with_state().await.0;
    let pool = state.db.as_ref().expect("db");

    let (grace_ticket, grace_run) = insert_running_ticket(pool, "in_progress").await;
    let grace_dir = tempfile::tempdir().expect("grace dir");
    process_tree::install(grace_dir.path().join("agent-processes.json"));
    let (grace_leader, grace_child) = spawn_tracked_sleeper(grace_dir.path(), grace_run).await;
    let _grace_cleanup = KillGroup(grace_leader);
    assert!(process_alive(grace_child));

    shutdown_agent_sessions(&state).await;

    assert!(
        wait_dead(grace_leader).await,
        "leader {grace_leader} survived shutdown"
    );
    assert!(
        wait_dead(grace_child).await,
        "background {grace_child} survived shutdown"
    );
    assert_blocked_with_quit_comment(pool, grace_ticket, grace_run, GRACEFUL_QUIT_REASON).await;

    let (crash_ticket, crash_run) = insert_running_ticket(pool, "in_progress").await;
    let crash_dir = tempfile::tempdir().expect("crash dir");
    let crash_path = crash_dir.path().join("agent-processes.json");
    process_tree::install(&crash_path);
    let (crash_leader, crash_child) = spawn_tracked_sleeper(crash_dir.path(), crash_run).await;
    let _crash_cleanup = KillGroup(crash_leader);
    assert!(process_alive(crash_leader));
    assert!(process_alive(crash_child));

    // The process that spawned the agent is gone. The next launch installs the
    // same record file, reaps the orphan, then sweeps runs still marked active.
    process_tree::install(&crash_path);
    process_tree::reap_orphaned_agents().await;
    sweep_orphaned_runs(&state).await;

    assert!(
        wait_dead(crash_leader).await,
        "leader {crash_leader} survived restart reap"
    );
    assert!(
        wait_dead(crash_child).await,
        "background {crash_child} survived restart reap"
    );
    assert_blocked_with_quit_comment(pool, crash_ticket, crash_run, "server restarted during run")
        .await;
}

#[tokio::test]
async fn graceful_shutdown_blocks_in_review_and_in_qa_tickets() {
    let _guard = QUIT_TEST_LOCK.lock().await;
    let state = common::bootstrap_and_login_with_state().await.0;
    let pool = state.db.as_ref().expect("db");

    for status in ["in_review", "in_qa"] {
        let (ticket_id, run_id) = insert_running_ticket(pool, status).await;
        let dir = tempfile::tempdir().expect("dir");
        process_tree::install(dir.path().join("agent-processes.json"));
        let (leader, child) = spawn_tracked_sleeper(dir.path(), run_id).await;
        let _cleanup = KillGroup(leader);
        assert!(process_alive(child), "{status} child was not running");

        shutdown_agent_sessions(&state).await;

        assert!(
            wait_dead(child).await,
            "{status} background {child} survived quit"
        );
        assert_blocked_with_quit_comment(pool, ticket_id, run_id, GRACEFUL_QUIT_REASON).await;
    }
}
