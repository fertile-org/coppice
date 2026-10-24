//! Against the installed `cursor-agent`. Not run in CI:
//! `COPPICE_REAL_CURSOR=1 cargo test --features embedded-test-db --test real_cursor -- --ignored --nocapture`.

use coppice_config::CursorProviderConfig;
use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::mcp::grant::McpAccess;
use coppice_server::providers::cursor::CursorProvider;
use coppice_server::providers::{AgentProvider, AgentRunInput, AgentRunResult};

#[tokio::test]
#[ignore = "needs a signed-in cursor-agent"]
async fn read_only_run_refuses_shell_and_writes_despite_project_allow() {
    if std::env::var("COPPICE_REAL_CURSOR").as_deref() != Ok("1") {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(worktree.join(".agent")).unwrap();
    std::fs::create_dir_all(worktree.join(".cursor")).unwrap();
    std::fs::write(worktree.join("README.md"), "first line of readme\n").unwrap();
    // A project file that allows writes must not override the per-run deny.
    std::fs::write(
        worktree.join(".cursor").join("cli.json"),
        r#"{"permissions":{"allow":["Shell(*)","Write(**)"],"deny":[]}}"#,
    )
    .unwrap();
    std::fs::write(
        worktree.join(".agent").join("context.md"),
        "# Task\n\nDo these in order: run the shell command `touch bash-ran`; create a file \
         `wrote.txt` containing hi; read README.md. Then reply with only this JSON and nothing else: \
         {\"status\":\"done\",\"summary\":\"<README's first line>\"}\n",
    )
    .unwrap();
    let artifacts = dir.path().join("artifacts");
    let provider = CursorProvider::new(CursorProviderConfig::default());
    let result = provider
        .run(AgentRunInput {
            agent_id: "a".into(),
            agent_key: "pm".into(),
            agent_role: "PM".into(),
            job_type: "chat_turn".into(),
            ticket_id: None,
            ticket_status: None,
            context_profile: ContextProfile::Conversation,
            context_path: worktree
                .join(".agent")
                .join("context.md")
                .display()
                .to_string(),
            run_id: Some("cursor-probe".into()),
            chat_session_id: None,
            artifacts_dir: Some(artifacts.display().to_string()),
            stream: None,
            cancel_rx: None,
            model_provider: None,
            model: None,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
            read_only_tools: true,
            // The gateway is not running; the probe checks writes, not tool calls.
            mcp: Some(McpAccess {
                url: "http://127.0.0.1:9/mcp".into(),
                token: "probe-token".into(),
            }),
        })
        .await;
    assert!(!worktree.join("bash-ran").exists(), "shell ran");
    assert!(!worktree.join("wrote.txt").exists(), "write ran");
    // Without a completed turn the file checks prove nothing.
    match result.expect("turn") {
        AgentRunResult::Done { summary, .. } => {
            assert!(summary.contains("first line of readme"), "{summary}")
        }
        other => panic!("probe did not complete: {other:?}"),
    }
}
