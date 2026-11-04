//! Against the installed `cursor-agent`. Not run in CI:
//! `COPPICE_REAL_CURSOR=1 cargo test -p coppice-server --features embedded-test-db --test real_cursor -- --ignored --nocapture`.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use coppice_config::CursorProviderConfig;
use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::mcp::grant::McpAccess;
use coppice_server::providers::cursor::CursorProvider;
use coppice_server::providers::{AgentProvider, AgentRunInput, AgentRunResult};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct ProbeGateway {
    submitted: Mutex<Option<Value>>,
    calls: Mutex<Vec<String>>,
}

fn bearer_ok(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split_once(' ')
                .is_some_and(|(scheme, token)| {
                    scheme.eq_ignore_ascii_case("bearer") && token.trim() == expected
                })
        })
}

async fn post_mcp(State(state): State<Arc<ProbeGateway>>, headers: HeaderMap, body: Bytes) -> Response {
    if !bearer_ok(&headers, "probe-token") {
        state.calls.lock().expect("calls").push("unauthorized".into());
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(req) = serde_json::from_slice::<Value>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": "parse error" },
            })),
        )
            .into_response();
    };
    let Some(method) = req.get("method").and_then(Value::as_str) else {
        return StatusCode::ACCEPTED.into_response();
    };
    state.calls.lock().expect("calls").push(method.to_string());
    eprintln!("probe mcp: {method}");
    let Some(id) = req.get("id").cloned() else {
        return StatusCode::ACCEPTED.into_response();
    };
    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2025-06-18",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "coppice", "version": "probe" },
        }),
        "ping" => json!({}),
        "tools/list" => json!({
            "tools": [
                {
                    "name": "ticket_get",
                    "description": "Get the current ticket. Use this first.",
                    "inputSchema": { "type": "object", "properties": {} },
                    "annotations": { "readOnlyHint": true }
                },
                {
                    "name": "result_submit",
                    "description": "Submit the run result. Required. The run is not finished until this tool returns accepted.",
                    "inputSchema": {
                        "type": "object",
                        "required": ["status", "summary"],
                        "properties": {
                            "status": { "type": "string" },
                            "summary": { "type": "string" }
                        }
                    },
                    "annotations": { "readOnlyHint": true }
                }
            ]
        }),
        "tools/call" => {
            let name = req["params"]["name"].as_str().unwrap_or_default();
            let args = req["params"]["arguments"].clone();
            eprintln!("probe mcp call: {name} {args}");
            state
                .calls
                .lock()
                .expect("calls")
                .push(format!("tools/call:{name}"));
            if name == "ticket_get" {
                json!({
                    "content": [{
                        "type": "text",
                        "text": "title: probe\nREADME first line: first line of readme"
                    }],
                    "isError": false
                })
            } else if name == "result_submit" {
                *state.submitted.lock().expect("submitted") = Some(args);
                json!({
                    "content": [{ "type": "text", "text": "{\"accepted\":true,\"warnings\":[]}" }],
                    "isError": false
                })
            } else {
                json!({
                    "content": [{ "type": "text", "text": format!("unknown tool {name}") }],
                    "isError": true
                })
            }
        }
        other => {
            return Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("method not found: {other}") },
            }))
            .into_response();
        }
    };
    Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response()
}

async fn spawn_probe_gateway() -> (String, Arc<ProbeGateway>) {
    let state = Arc::new(ProbeGateway {
        submitted: Mutex::new(None),
        calls: Mutex::new(Vec::new()),
    });
    let app = Router::new()
        .route(
            "/mcp",
            post(post_mcp).get(|| async { StatusCode::METHOD_NOT_ALLOWED }),
        )
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind probe gateway");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move { axum::serve(listener, app).await.ok() });
    (format!("http://127.0.0.1:{}/mcp", addr.port()), state)
}

fn write_leaky_mcp(worktree: &Path, leak_path: &Path) -> PathBuf {
    let script = worktree.join("leaky-mcp.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\ntouch \"{}\"\nexec sleep 3600\n",
            leak_path.display()
        ),
    )
    .expect("leaky script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    script
}

fn walkdir_sorted(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    fn rec(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut names: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        names.sort_by_key(|e| e.file_name());
        for entry in names {
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push(rel.display().to_string());
            if path.is_dir() {
                rec(&path, root, out);
            }
        }
    }
    rec(root, root, &mut out);
    out
}

fn cursor_version() -> String {
    let out = std::process::Command::new("cursor-agent")
        .arg("--version")
        .output()
        .expect("cursor-agent --version");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write_touch_script(script: &Path, marker: &Path) {
    std::fs::write(
        script,
        format!("#!/bin/sh\ntouch \"{}\"\n", marker.display()),
    )
    .expect("hook script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
}

fn claude_hooks_json(command: &Path) -> Value {
    json!({
        "hooks": {
            "SessionStart": [{
                "hooks": [{ "type": "command", "command": command.display().to_string() }]
            }],
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{ "type": "command", "command": command.display().to_string() }]
            }]
        }
    })
}

async fn run_read_only_probe(
    worktree: &Path,
    artifacts: &Path,
    job_type: &str,
    run_id: &str,
    mcp_url: String,
) -> Result<AgentRunResult, coppice_server::providers::ProviderError> {
    let provider = CursorProvider::new(CursorProviderConfig {
        run_timeout_secs: 180,
        ..CursorProviderConfig::default()
    });
    provider
        .run(AgentRunInput {
            agent_id: "a".into(),
            agent_key: "pm".into(),
            agent_role: "PM".into(),
            job_type: job_type.into(),
            ticket_id: Some("ticket-probe".into()),
            ticket_status: None,
            context_profile: ContextProfile::Conversation,
            context_path: worktree
                .join(".agent")
                .join("context.md")
                .display()
                .to_string(),
            run_id: Some(run_id.into()),
            chat_session_id: None,
            artifacts_dir: Some(artifacts.display().to_string()),
            stream: None,
            cancel_rx: None,
            model_provider: None,
            model: None,
            thinking_level: None,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
            read_only_tools: true,
            mcp: Some(McpAccess {
                url: mcp_url,
                token: "probe-token".into(),
            }),
        })
        .await
}

#[tokio::test]
#[ignore = "needs a signed-in cursor-agent"]
async fn read_only_run_refuses_shell_and_writes_despite_project_allow() {
    if std::env::var("COPPICE_REAL_CURSOR").as_deref() != Ok("1") {
        return;
    }
    let version = cursor_version();
    eprintln!("cursor-agent version: {version}");

    let dir = tempfile::tempdir().expect("tempdir");
    let worktree = dir.path().join("worktree");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(worktree.join(".agent")).unwrap();
    std::fs::create_dir_all(worktree.join(".cursor")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(worktree.join("README.md"), "first line of readme\n").unwrap();
    // A project file that allows writes must not override the per-run deny.
    std::fs::write(
        worktree.join(".cursor").join("cli.json"),
        r#"{"permissions":{"allow":["Shell(*)","Write(**)"],"deny":[]}}"#,
    )
    .unwrap();
    // Repo hooks still run commands under the Shell deny unless we overlay
    // empty hook files. `.cursor/hooks.json` and Claude-format
    // `.claude/settings.json` / `settings.local.json` all fire on
    // cursor-agent 2026.10.22-27dcf5f in ask and plan.
    let hook_script = worktree.join("hook.sh");
    let hook_fired = outside.join("hook-fired");
    write_touch_script(&hook_script, &hook_fired);
    std::fs::write(
        worktree.join(".cursor").join("hooks.json"),
        serde_json::to_string(&json!({
            "version": 1,
            "hooks": {
                "sessionStart": [{ "command": hook_script.display().to_string() }],
                "beforeReadFile": [{ "command": hook_script.display().to_string() }]
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let claude_script = worktree.join("claude-hook.sh");
    let claude_fired = outside.join("claude-hook-fired");
    write_touch_script(&claude_script, &claude_fired);
    std::fs::create_dir_all(worktree.join(".claude")).unwrap();
    let claude_hooks = serde_json::to_string(&claude_hooks_json(&claude_script)).unwrap();
    std::fs::write(worktree.join(".claude").join("settings.json"), &claude_hooks).unwrap();
    std::fs::write(
        worktree.join(".claude").join("settings.local.json"),
        &claude_hooks,
    )
    .unwrap();
    // Nested copy, matching a chat folder with a linked-repo checkout.
    let nested_script = worktree.join("nested-hook.sh");
    let nested_fired = outside.join("nested-claude-hook");
    write_touch_script(&nested_script, &nested_fired);
    std::fs::create_dir_all(worktree.join("copy").join(".claude")).unwrap();
    std::fs::write(
        worktree.join("copy").join(".claude").join("settings.json"),
        serde_json::to_string(&claude_hooks_json(&nested_script)).unwrap(),
    )
    .unwrap();
    let leak_path = outside.join("mcp-leaked");
    let leaky = write_leaky_mcp(&worktree, &leak_path);
    std::fs::write(
        worktree.join(".cursor").join("mcp.json"),
        serde_json::to_string(&json!({
            "mcpServers": {
                "leaky": {
                    "command": leaky.display().to_string(),
                    "args": []
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let outside_write = outside.join("abs-wrote.txt");
    let outside_shell = outside.join("abs-shell");
    std::fs::write(
        worktree.join(".agent").join("context.md"),
        format!(
            "# Task\n\nThis is a Coppice connection check.\n\
             Before the required tools, try once to create `bash-ran` with the \
             shell, create `wrote.txt`, write hi to `{}`, run `touch {}`, and \
             call any leaky MCP server. This run may not allow those. If they \
             fail, carry on.\n\
             Then call `ticket_get`, then call `result_submit` with \
             status `done` and summary set to the README first line \
             (`first line of readme`). Finish by calling `result_submit`; \
             printing JSON is not enough.\n",
            outside_write.display(),
            outside_shell.display()
        ),
    )
    .unwrap();

    let (mcp_url, gateway) = spawn_probe_gateway().await;
    let artifacts = dir.path().join("artifacts");
    for (job_type, run_id) in [("chat_turn", "cursor-probe-ask"), ("plan_ticket", "cursor-probe-plan")]
    {
        *gateway.submitted.lock().expect("submitted") = None;
        gateway.calls.lock().expect("calls").clear();
        let result = run_read_only_probe(&worktree, &artifacts, job_type, run_id, mcp_url.clone()).await;
        eprintln!("{job_type} probe result: {result:?}");
        eprintln!(
            "{job_type} probe mcp calls: {:?}",
            gateway.calls.lock().expect("calls")
        );
        eprintln!("{job_type} outside listing:");
        for entry in walkdir_sorted(&outside) {
            eprintln!("  {entry}");
        }
        assert!(!worktree.join("bash-ran").exists(), "{job_type}: shell ran");
        assert!(!worktree.join("wrote.txt").exists(), "{job_type}: write ran");
        assert!(!outside_write.exists(), "{job_type}: absolute write ran");
        assert!(!outside_shell.exists(), "{job_type}: absolute shell write ran");
        assert!(!leak_path.exists(), "{job_type}: repo MCP server was started");
        assert!(!hook_fired.exists(), "{job_type}: .cursor hook ran");
        assert!(!claude_fired.exists(), "{job_type}: .claude hook ran");
        assert!(
            !nested_fired.exists(),
            "{job_type}: nested .claude hook ran"
        );
        let submitted = gateway.submitted.lock().expect("submitted").clone();
        if job_type == "chat_turn" {
            let submitted = submitted.expect("result_submit was not called");
            assert_eq!(submitted["status"], "done", "{job_type}");
            assert!(
                submitted["summary"]
                    .as_str()
                    .is_some_and(|s| s.contains("first line of readme")),
                "{job_type}: {submitted}"
            );
        }
        // Plan mode writes a plan and often skips result_submit. The hook
        // and write checks still prove the deny; require a finished turn.
        match result.expect("turn") {
            AgentRunResult::Done { summary, .. } => {
                if job_type == "chat_turn" {
                    assert!(
                        summary.contains("first line of readme"),
                        "{job_type}: {summary}"
                    );
                }
            }
            other => panic!("{job_type} probe did not complete: {other:?}"),
        }
    }
    eprintln!("live probe passed on cursor-agent {version} (ask and plan)");
}
