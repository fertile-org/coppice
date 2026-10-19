//! The ACP client against `fake-opencode acp`, one scripted scenario per test.

use coppice_server::mcp::grant::McpAccess;
use coppice_server::providers::acp::{
    run_turn, AcpLaunch, AcpLimits, AcpObserver, AcpOutcome, AcpTurn, ToolPolicy,
};
use coppice_server::providers::ProviderError;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

const FAKE_OPENCODE: &str = env!("CARGO_BIN_EXE_fake-opencode");

#[derive(Default)]
struct Seen {
    session: Option<(String, Option<String>)>,
    updates: Vec<Value>,
    ended: bool,
}

impl AcpObserver for Seen {
    fn on_session(&mut self, session_id: &str, model: Option<&str>) {
        self.session = Some((session_id.to_string(), model.map(str::to_string)));
    }
    fn on_update(&mut self, update: &Value) {
        self.updates.push(update.clone());
    }
    fn on_end(&mut self) {
        self.ended = true;
    }
}

struct Run {
    dir: tempfile::TempDir,
    scenario: &'static str,
    policy: ToolPolicy,
    mcp: Option<McpAccess>,
    model: Option<&'static str>,
    resume: Option<&'static str>,
    limits: AcpLimits,
}

impl Run {
    fn new(scenario: &'static str) -> Self {
        Self {
            dir: tempfile::tempdir().expect("tempdir"),
            scenario,
            policy: ToolPolicy::AllowAll,
            mcp: Some(McpAccess {
                url: "http://127.0.0.1:5000/mcp".into(),
                token: "run-token-123".into(),
            }),
            model: None,
            resume: None,
            limits: AcpLimits {
                handshake: Duration::from_secs(10),
                run: Duration::from_secs(30),
                idle: Duration::from_secs(30),
            },
        }
    }

    fn record_path(&self) -> std::path::PathBuf {
        self.dir.path().join("record.json")
    }

    fn record(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.record_path()).unwrap_or_default())
            .unwrap_or(Value::Null)
    }

    async fn go(
        &self,
        seen: &mut Seen,
        cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<AcpOutcome, ProviderError> {
        let launch = AcpLaunch {
            display_name: "OpenCode".into(),
            program: FAKE_OPENCODE.into(),
            args: vec!["acp".into()],
            env: vec![
                ("FAKE_ACP_SCENARIO".into(), self.scenario.into()),
                (
                    "FAKE_ACP_RECORD".into(),
                    self.record_path().display().to_string(),
                ),
                ("OPENCODE_CONFIG".into(), "/tmp/run/opencode.json".into()),
            ],
            cwd: self.dir.path().to_path_buf(),
            run_id: None,
            ticket_id: None,
            task_log_dir: None,
        };
        let cwd: &Path = self.dir.path();
        let turn = AcpTurn {
            cwd,
            mcp: self.mcp.as_ref(),
            model: self.model,
            resume_session_id: self.resume,
            prompt: "do the thing",
            policy: self.policy,
        };
        run_turn(launch, turn, self.limits, seen, cancel_rx, None).await
    }
}

#[tokio::test]
async fn a_turn_passes_the_gateway_and_returns_the_agent_text() {
    let mut run = Run::new("done");
    run.model = Some("opencode/big-pickle");
    let mut seen = Seen::default();
    let outcome = run.go(&mut seen, None).await.expect("turn");

    assert_eq!(outcome.stop_reason, "end_turn");
    assert_eq!(outcome.session_id, "ses_fake");
    assert_eq!(
        outcome.agent_text,
        r#"{"status":"done","summary":"Read it"}"#
    );
    assert!(outcome.refused.is_empty());
    assert!(seen.ended);
    assert_eq!(
        seen.session,
        Some(("ses_fake".into(), Some("opencode/big-pickle".into())))
    );
    assert_eq!(seen.updates.len(), 4, "{:?}", seen.updates);

    let record = run.record();
    assert_eq!(record["model"], "opencode/big-pickle");
    assert_eq!(record["opencodeConfig"], "/tmp/run/opencode.json");
    let server = &record["mcpServers"][0];
    assert_eq!(server["type"], "http");
    assert_eq!(server["url"], "http://127.0.0.1:5000/mcp");
    assert_eq!(server["headers"][0]["value"], "Bearer run-token-123");
    assert_eq!(record["cwd"], run.dir.path().display().to_string());
}

#[tokio::test]
async fn the_current_model_is_reported_when_none_is_chosen() {
    let run = Run::new("done");
    let mut seen = Seen::default();
    run.go(&mut seen, None).await.expect("turn");
    assert_eq!(
        seen.session.unwrap().1.as_deref(),
        Some("opencode/default-free")
    );
    assert!(run.record().get("model").is_none());
}

#[tokio::test]
async fn a_model_the_agent_does_not_offer_is_refused_before_the_prompt() {
    let mut run = Run::new("done");
    run.model = Some("anthropic/not-there");
    let err = run
        .go(&mut Seen::default(), None)
        .await
        .expect_err("bad model");
    let text = err.to_string();
    assert!(
        text.contains("doesn't offer the model `anthropic/not-there`"),
        "{text}"
    );
}

#[tokio::test]
async fn no_http_mcp_is_mcp_unavailable_only_when_the_gateway_is_needed() {
    let run = Run::new("no_http");
    let err = run
        .go(&mut Seen::default(), None)
        .await
        .expect_err("no http");
    assert!(err.to_string().contains("mcp_unavailable"), "{err}");

    let mut run = Run::new("no_http");
    run.mcp = None;
    run.go(&mut Seen::default(), None)
        .await
        .expect("no gateway needed");
}

#[tokio::test]
async fn another_protocol_version_is_refused() {
    let err = Run::new("wrong_version")
        .go(&mut Seen::default(), None)
        .await
        .expect_err("v2");
    assert!(err.to_string().contains("ACP version 2"), "{err}");
}

#[tokio::test]
async fn read_only_refuses_a_shell_request_and_says_why_the_turn_stopped() {
    let mut run = Run::new("permission");
    run.policy = ToolPolicy::ReadOnly;
    let outcome = run.go(&mut Seen::default(), None).await.expect("turn");
    assert_eq!(outcome.stop_reason, "cancelled");
    assert_eq!(outcome.refused, ["touch x"]);
    let problem = outcome.stop_problem("OpenCode").expect("problem");
    assert!(problem.contains("refused: touch x"), "{problem}");
    assert_eq!(
        run.record()["permissionReply"]["result"]["outcome"]["optionId"],
        "reject"
    );
}

#[tokio::test]
async fn a_write_run_approves_once() {
    let run = Run::new("permission");
    let outcome = run.go(&mut Seen::default(), None).await.expect("turn");
    assert_eq!(outcome.stop_reason, "end_turn");
    assert_eq!(
        run.record()["permissionReply"]["result"]["outcome"]["optionId"],
        "once"
    );
}

#[tokio::test]
async fn an_unsupported_request_gets_an_error_reply_and_the_turn_goes_on() {
    let run = Run::new("unknown_request");
    let outcome = run.go(&mut Seen::default(), None).await.expect("turn");
    assert_eq!(outcome.stop_reason, "end_turn");
    let reply = &run.record()["permissionReply"];
    assert_eq!(reply["error"]["code"], -32601);
}

#[tokio::test]
async fn resume_uses_session_resume_when_offered() {
    let mut run = Run::new("done");
    run.resume = Some("ses_old");
    let outcome = run.go(&mut Seen::default(), None).await.expect("turn");
    assert_eq!(outcome.session_id, "ses_old");
    assert_eq!(run.record()["resumeMethod"], "session/resume");
}

#[tokio::test]
async fn load_replays_history_without_counting_it_as_this_turn() {
    let mut run = Run::new("load_only");
    run.resume = Some("ses_old");
    let mut seen = Seen::default();
    let outcome = run.go(&mut seen, None).await.expect("turn");
    assert_eq!(run.record()["resumeMethod"], "session/load");
    assert!(
        !outcome.agent_text.contains("OLD REPLY"),
        "{}",
        outcome.agent_text
    );
    assert!(!seen
        .updates
        .iter()
        .any(|u| u.to_string().contains("OLD REPLY")));
}

#[tokio::test]
async fn a_missing_session_is_resume_invalid_but_a_gateway_failure_is_not() {
    let mut run = Run::new("resume_missing");
    run.resume = Some("ses_gone");
    let err = run.go(&mut Seen::default(), None).await.expect_err("gone");
    assert!(
        matches!(err, ProviderError::ResumeSessionInvalid(_)),
        "{err}"
    );

    let mut run = Run::new("resume_mcp_down");
    run.resume = Some("ses_ok");
    let err = run
        .go(&mut Seen::default(), None)
        .await
        .expect_err("mcp down");
    assert!(
        !matches!(err, ProviderError::ResumeSessionInvalid(_)),
        "{err}"
    );
    assert!(
        err.to_string()
            .contains("MCP server coppice failed to connect"),
        "{err}"
    );
}

#[tokio::test]
async fn a_silent_turn_stops_at_the_idle_limit() {
    let mut run = Run::new("stall");
    run.limits.idle = Duration::from_secs(1);
    let err = run.go(&mut Seen::default(), None).await.expect_err("idle");
    assert!(err.to_string().contains("sent nothing for"), "{err}");
}

#[tokio::test]
async fn a_running_tool_is_not_idle() {
    let mut run = Run::new("tool_running");
    run.limits.idle = Duration::from_secs(1);
    let outcome = run
        .go(&mut Seen::default(), None)
        .await
        .expect("tool kept it alive");
    assert_eq!(outcome.stop_reason, "end_turn");
}

#[tokio::test]
async fn cancel_sends_session_cancel_and_ends_as_cancelled() {
    for scenario in ["stall", "slow_cancel"] {
        let run = Run::new(scenario);
        let (tx, rx) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = tx.send(true);
            // Keep the sender alive past the grace period.
            tokio::time::sleep(Duration::from_secs(10)).await;
        });
        let err = run
            .go(&mut Seen::default(), Some(rx))
            .await
            .expect_err("cancelled");
        assert!(matches!(err, ProviderError::Cancelled), "{scenario}: {err}");
        assert_eq!(run.record()["cancelled"], true, "{scenario}");
    }
}

#[tokio::test]
async fn an_oversized_line_fails_the_turn_clearly() {
    let err = Run::new("big_line")
        .go(&mut Seen::default(), None)
        .await
        .expect_err("too big");
    assert!(err.to_string().contains("longer than 32 MiB"), "{err}");
}

#[tokio::test]
async fn an_early_exit_reports_stderr() {
    let err = Run::new("exit_early")
        .go(&mut Seen::default(), None)
        .await
        .expect_err("exit");
    let text = err.to_string();
    // Depending on timing the exit shows on the write or on the read.
    assert!(text.contains("exited before"), "{text}");
    assert!(text.contains("provider not configured"), "{text}");

    let err = Run::new("exit_mid_turn")
        .go(&mut Seen::default(), None)
        .await
        .expect_err("crash");
    let text = err.to_string();
    assert!(
        text.contains("exited before answering session/prompt"),
        "{text}"
    );
    assert!(text.contains("crashed"), "{text}");
}

#[tokio::test]
async fn a_stopped_turn_names_its_reason() {
    let outcome = Run::new("max_tokens")
        .go(&mut Seen::default(), None)
        .await
        .expect("turn");
    assert_eq!(outcome.stop_reason, "max_tokens");
    assert!(outcome
        .stop_problem("OpenCode")
        .unwrap()
        .contains("token limit"));
}

/// Against the installed `opencode` (2.x). Not run in CI:
/// `COPPICE_REAL_OPENCODE=1 cargo test --test integration_acp real_opencode -- --ignored`.
#[tokio::test]
#[ignore = "needs a real OpenCode 2.x install"]
async fn real_opencode_read_only_turn() {
    if std::env::var("COPPICE_REAL_OPENCODE").as_deref() != Ok("1") {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("README.md"), "first line of readme\n").unwrap();
    let config = dir.path().join("opencode.json");
    std::fs::write(
        &config,
        r#"{"$schema":"https://opencode.ai/config.json","permission":{"edit":"deny","bash":"deny","write":"deny"}}"#,
    )
    .unwrap();
    let launch = AcpLaunch {
        display_name: "OpenCode".into(),
        program: "opencode".into(),
        args: vec!["acp".into()],
        env: vec![
            ("OPENCODE_CONFIG".into(), config.display().to_string()),
            ("OPENCODE_DISABLE_PROJECT_CONFIG".into(), "1".into()),
        ],
        cwd: dir.path().to_path_buf(),
        run_id: None,
        ticket_id: None,
        task_log_dir: None,
    };
    let model = std::env::var("COPPICE_REAL_OPENCODE_MODEL")
        .unwrap_or_else(|_| "opencode/big-pickle".into());
    let turn = AcpTurn {
        cwd: dir.path(),
        mcp: None,
        model: Some(&model),
        resume_session_id: None,
        prompt:
            "Run the shell command `touch bash-ran`, then create a file wrote.txt containing hi, \
                 then read README.md. Finally reply with only this JSON and nothing else: \
                 {\"status\":\"done\",\"summary\":\"<the README's first line>\"}",
        policy: ToolPolicy::ReadOnly,
    };
    let mut seen = Seen::default();
    let outcome = run_turn(
        launch,
        turn,
        AcpLimits {
            handshake: Duration::from_secs(60),
            run: Duration::from_secs(300),
            idle: Duration::from_secs(180),
        },
        &mut seen,
        None,
        None,
    )
    .await
    .expect("turn");
    eprintln!(
        "stop={} text={:?} refused={:?}",
        outcome.stop_reason, outcome.agent_text, outcome.refused
    );
    assert!(
        !dir.path().join("bash-ran").exists(),
        "shell ran in a read-only turn"
    );
    assert!(
        !dir.path().join("wrote.txt").exists(),
        "write ran in a read-only turn"
    );
    assert_eq!(outcome.stop_reason, "end_turn");
    assert!(
        outcome.agent_text.contains("first line of readme"),
        "{}",
        outcome.agent_text
    );
    assert_eq!(seen.session.unwrap().1.as_deref(), Some(model.as_str()));
}
