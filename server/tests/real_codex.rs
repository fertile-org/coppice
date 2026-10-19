//! Against the installed `codex`. Not run in CI:
//! `COPPICE_REAL_CODEX=1 cargo test --features embedded-test-db --test real_codex -- --ignored`.

use coppice_config::CodexProviderConfig;
use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::providers::codex::CodexProvider;
use coppice_server::providers::{AgentProvider, AgentRunInput, AgentRunResult};

#[tokio::test]
#[ignore = "needs a signed-in Codex"]
async fn read_only_chat_turn_refuses_shell_and_writes() {
    if std::env::var("COPPICE_REAL_CODEX").as_deref() != Ok("1") {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("README.md"), "first line of readme\n").unwrap();
    std::fs::create_dir_all(dir.path().join(".agent")).unwrap();
    std::fs::write(
        dir.path().join(".agent").join("context.md"),
        "# Task\n\nDo these in order: run the shell command `touch bash-ran`; create a file \
         `wrote.txt` containing hi; read README.md. Then reply with only this JSON and nothing else: \
         {\"status\":\"done\",\"summary\":\"<README's first line>\"}\n",
    )
    .unwrap();
    let provider = CodexProvider::new(CodexProviderConfig::default());
    let result = provider
        .run(AgentRunInput {
            agent_id: "a".into(),
            agent_key: "pm".into(),
            agent_role: "PM".into(),
            job_type: "chat_turn".into(),
            ticket_id: None,
            ticket_status: None,
            context_profile: ContextProfile::Conversation,
            context_path: dir
                .path()
                .join(".agent")
                .join("context.md")
                .display()
                .to_string(),
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
        })
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
