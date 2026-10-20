//! The ACP CLI connectors against installed CLIs. Not run in CI. Each test runs
//! a read-only chat turn that asks for a shell command, a file write, and a
//! read, then checks nothing was written and the reply came back.
//!
//! - `COPPICE_REAL_COPILOT=1`: the signed-in `copilot`.
//! - `COPPICE_REAL_QWEN=<command>` / `COPPICE_REAL_KIMI=<command>`: a command
//!   for `qwen` / `kimi`, such as a wrapper that points it at a scripted model.
//! - `COPPICE_REAL_GROK=1`: the signed-in `grok`.
//!
//! The work folder is under `target/tmp`, not `/tmp`: Grok's read-only sandbox
//! lets it write to temp folders, so Coppice refuses a read-only Grok run there.
//!
//! `cargo test --features embedded-test-db --test real_acp_clis -- --ignored`

use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::providers::acp_cli::{self, AcpCliProvider, AcpCliSpec};
use coppice_server::providers::{AgentProvider, AgentRunInput, AgentRunResult};

fn input(dir: &std::path::Path) -> AgentRunInput {
    AgentRunInput {
        agent_id: "a".into(),
        agent_key: "pm".into(),
        agent_role: "PM".into(),
        job_type: "chat_turn".into(),
        ticket_id: None,
        ticket_status: None,
        context_profile: ContextProfile::Conversation,
        context_path: dir.join(".agent").join("context.md").display().to_string(),
        run_id: None,
        chat_session_id: None,
        artifacts_dir: None,
        stream: None,
        cancel_rx: None,
        model_provider: None,
        model: None,
        session_created_tx: None,
        resume_context: None,
        resume_session_id: None,
        read_only_tools: true,
        mcp: None,
    }
}

async fn read_only_turn(spec: AcpCliSpec, command: &str) {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    std::fs::write(dir.path().join("README.md"), "first line of readme\n").unwrap();
    std::fs::create_dir_all(dir.path().join(".agent")).unwrap();
    std::fs::write(
        dir.path().join(".agent").join("context.md"),
        "# Task\n\nDo these in order: run the shell command `touch bash-ran`; create a file \
         `wrote.txt` containing hi; read README.md. Then reply with only this JSON and nothing else: \
         {\"status\":\"done\",\"summary\":\"<README's first line>\"}\n",
    )
    .unwrap();
    let provider = AcpCliProvider::new(spec, command.to_string(), 600);
    let result = provider
        .run(input(dir.path()))
        .await
        .expect("turn");
    assert!(!dir.path().join("bash-ran").exists(), "shell ran");
    assert!(!dir.path().join("wrote.txt").exists(), "write ran");
    match result {
        AgentRunResult::Done { summary, .. } => {
            assert!(summary.contains("first line of readme"), "{summary}")
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
#[ignore = "needs a signed-in GitHub Copilot CLI"]
async fn copilot_read_only_chat_turn() {
    if std::env::var("COPPICE_REAL_COPILOT").as_deref() != Ok("1") {
        return;
    }
    read_only_turn(acp_cli::COPILOT, "copilot").await;
}

#[tokio::test]
#[ignore = "needs Qwen Code and a model"]
async fn qwen_read_only_chat_turn() {
    let Ok(command) = std::env::var("COPPICE_REAL_QWEN") else {
        return;
    };
    read_only_turn(acp_cli::QWEN_CODE, &command).await;
}

#[tokio::test]
#[ignore = "needs Kimi Code and a model"]
async fn kimi_read_only_chat_turn() {
    let Ok(command) = std::env::var("COPPICE_REAL_KIMI") else {
        return;
    };
    read_only_turn(acp_cli::KIMI, &command).await;
}

#[tokio::test]
#[ignore = "needs a signed-in Grok"]
async fn grok_read_only_chat_turn() {
    if std::env::var("COPPICE_REAL_GROK").as_deref() != Ok("1") {
        return;
    }
    read_only_turn(acp_cli::GROK, "grok").await;
}

/// Needs no CLI: the run is refused before anything starts.
#[tokio::test]
async fn grok_read_only_run_in_a_temp_folder_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join(".agent")).unwrap();
    std::fs::write(dir.path().join(".agent").join("context.md"), "# Task\n").unwrap();
    let provider = AcpCliProvider::new(acp_cli::GROK, "grok-not-installed".into(), 60);
    let err = provider.run(input(dir.path())).await.expect_err("refused");
    let text = err.to_string();
    assert!(text.contains("can't be kept read-only in a temp folder"), "{text}");
    assert!(!text.contains("grok-not-installed"), "it must not try to start: {text}");
}
