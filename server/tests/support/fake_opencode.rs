//! Stand-in for `opencode serve` in integration tests: records how it was
//! launched and answers `GET /doc` until killed. `fake-opencode acp` is a
//! scripted ACP agent; `FAKE_ACP_SCENARIO` picks the script.

use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        println!("opencode 1.19.0");
        return;
    }
    if args.first().map(String::as_str) == Some("acp") {
        acp::serve();
        return;
    }
    if args.first().map(String::as_str) != Some("serve") {
        eprintln!("fake-opencode: expected `serve`, got {args:?}");
        std::process::exit(2);
    }
    let hostname = flag(&args, "--hostname").unwrap_or_else(|| "127.0.0.1".into());
    let port = flag(&args, "--port").expect("--port is required");

    if let Ok(line) = std::env::var("FAKE_OPENCODE_STDOUT") {
        println!("{line}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
    if let Ok(line) = std::env::var("FAKE_OPENCODE_STDERR") {
        eprintln!("{line}");
    }

    if let Ok(record) = std::env::var("FAKE_OPENCODE_RECORD") {
        let body = serde_json::json!({
            "opencodeConfig": std::env::var("OPENCODE_CONFIG").ok(),
            "hasToken": std::env::var("COPPICE_MCP_TOKEN").is_ok(),
            "pid": std::process::id(),
        });
        std::fs::write(record, body.to_string()).expect("write record");
    }

    let app = Router::new().route("/doc", get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind(format!("{hostname}:{port}"))
        .await
        .expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

mod acp {
    use serde_json::{json, Value};
    use std::io::{BufRead, Write};

    fn send(value: Value) {
        let mut out = std::io::stdout().lock();
        writeln!(out, "{value}").unwrap();
        out.flush().unwrap();
    }

    fn update(session: &str, update: Value) {
        send(json!({ "jsonrpc": "2.0", "method": "session/update",
            "params": { "sessionId": session, "update": update } }));
    }

    fn text(session: &str, text: &str) {
        update(
            session,
            json!({ "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": text } }),
        );
    }

    fn record(key: &str, value: Value) {
        if let Ok(path) = std::env::var("FAKE_ACP_RECORD") {
            let mut all: Value = std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(json!({}));
            all[key] = value;
            std::fs::write(path, all.to_string()).unwrap();
        }
    }

    pub fn serve() {
        let scenario = std::env::var("FAKE_ACP_SCENARIO").unwrap_or_else(|_| "done".into());
        record(
            "opencodeConfig",
            json!(std::env::var("OPENCODE_CONFIG").ok()),
        );
        record(
            "disableProjectConfig",
            json!(std::env::var("OPENCODE_DISABLE_PROJECT_CONFIG").ok()),
        );
        record("claudecode", json!(std::env::var("CLAUDECODE").ok()));
        if scenario == "exit_early" {
            eprintln!("fake-opencode: provider not configured");
            std::process::exit(3);
        }
        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();
        let mut session = "ses_fake".to_string();
        let mut pending_permission: Option<(u64, u64)> = None;
        let mut prompt_pending: Option<u64> = None;
        while let Some(Ok(line)) = lines.next() {
            let msg: Value = serde_json::from_str(&line).expect("json from client");
            let method = msg
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let id = msg.get("id").cloned();
            let params = msg.get("params").cloned().unwrap_or(json!({}));
            match method.as_str() {
                "initialize" => {
                    let http = scenario != "no_http";
                    let mut caps =
                        json!({ "loadSession": true, "mcpCapabilities": { "http": http } });
                    if scenario != "load_only" {
                        caps["sessionCapabilities"] = json!({ "resume": {} });
                    }
                    send(json!({ "jsonrpc": "2.0", "id": id, "result": {
                        "protocolVersion": if scenario == "wrong_version" { 2 } else { 1 },
                        "agentCapabilities": caps } }));
                }
                "session/new" => {
                    record("mcpServers", params["mcpServers"].clone());
                    record("cwd", params["cwd"].clone());
                    send(json!({ "jsonrpc": "2.0", "id": id, "result": {
                        "sessionId": session,
                        "configOptions": [{ "id": "model", "category": "model", "type": "select",
                            "currentValue": "opencode/default-free",
                            "options": [
                                { "value": "opencode/default-free", "name": "Default" },
                                { "group": "more", "name": "More", "options": [{ "value": "opencode/big-pickle", "name": "Big Pickle" }] }
                            ] }] } }));
                }
                "session/resume" | "session/load" => {
                    record("resumeMethod", json!(method));
                    session = params["sessionId"].as_str().unwrap_or("").to_string();
                    match scenario.as_str() {
                        "resume_missing" => send(json!({ "jsonrpc": "2.0", "id": id,
                            "error": { "code": -32002, "message": format!("Session {session} not found") } })),
                        "resume_mcp_down" => send(json!({ "jsonrpc": "2.0", "id": id,
                            "error": { "code": -32603, "message": "MCP server coppice failed to connect" } })),
                        _ => {
                            if method == "session/load" {
                                // Replayed history is not part of this turn.
                                text(&session, "OLD REPLY FROM HISTORY");
                            }
                            send(json!({ "jsonrpc": "2.0", "id": id, "result": {} }));
                        }
                    }
                }
                "session/set_config_option" => {
                    record("model", params["value"].clone());
                    send(json!({ "jsonrpc": "2.0", "id": id, "result": { "configOptions": [] } }));
                }
                "session/cancel" => {
                    record("cancelled", json!(true));
                    pending_permission = None;
                    if scenario == "slow_cancel" {
                        continue;
                    }
                    if let Some(prompt_id) = prompt_pending.take() {
                        send(
                            json!({ "jsonrpc": "2.0", "id": prompt_id, "result": { "stopReason": "cancelled" } }),
                        );
                    }
                }
                "session/prompt" => {
                    let prompt_id = id.clone().unwrap().as_u64().unwrap();
                    prompt_pending = Some(prompt_id);
                    match scenario.as_str() {
                        "stall" | "slow_cancel" => {
                            update(
                                &session,
                                json!({ "sessionUpdate": "agent_thought_chunk",
                                "content": { "type": "text", "text": "thinking" } }),
                            );
                        }
                        "tool_running" => {
                            update(
                                &session,
                                json!({ "sessionUpdate": "tool_call", "toolCallId": "long",
                                "kind": "execute", "status": "in_progress", "title": "cargo build" }),
                            );
                            std::thread::sleep(std::time::Duration::from_millis(1500));
                            update(
                                &session,
                                json!({ "sessionUpdate": "tool_call_update", "toolCallId": "long", "status": "completed" }),
                            );
                            text(&session, r#"{"status":"done","summary":"built"}"#);
                            send(
                                json!({ "jsonrpc": "2.0", "id": prompt_id, "result": { "stopReason": "end_turn" } }),
                            );
                        }
                        "permission" => {
                            update(
                                &session,
                                json!({ "sessionUpdate": "tool_call", "toolCallId": "t1",
                                "kind": "execute", "status": "pending", "title": "touch x" }),
                            );
                            send(
                                json!({ "jsonrpc": "2.0", "id": 900, "method": "session/request_permission", "params": {
                                "sessionId": session,
                                "toolCall": { "toolCallId": "t1", "kind": "execute", "title": "touch x" },
                                "options": [
                                    { "optionId": "once", "kind": "allow_once", "name": "Allow" },
                                    { "optionId": "always", "kind": "allow_always", "name": "Always" },
                                    { "optionId": "reject", "kind": "reject_once", "name": "Reject" }
                                ] } }),
                            );
                            pending_permission = Some((prompt_id, 900));
                        }
                        "unknown_request" => {
                            send(
                                json!({ "jsonrpc": "2.0", "id": 901, "method": "fs/read_text_file",
                                "params": { "sessionId": session, "path": "/etc/hostname" } }),
                            );
                            pending_permission = Some((prompt_id, 901));
                        }
                        "big_line" => {
                            let big = "x".repeat(40 * 1024 * 1024);
                            text(&session, &big);
                        }
                        "max_tokens" => {
                            text(&session, "partial");
                            send(
                                json!({ "jsonrpc": "2.0", "id": prompt_id, "result": { "stopReason": "max_tokens" } }),
                            );
                        }
                        "exit_mid_turn" => {
                            eprintln!("fake-opencode: crashed");
                            std::process::exit(4);
                        }
                        _ => {
                            println!("a log line that is not json");
                            update(
                                &session,
                                json!({ "sessionUpdate": "tool_call", "toolCallId": "r1",
                                "kind": "read", "status": "pending", "title": "read README.md" }),
                            );
                            update(
                                &session,
                                json!({ "sessionUpdate": "tool_call_update", "toolCallId": "r1", "status": "completed",
                                "content": [{ "type": "content", "content": { "type": "text", "text": "first line" } }] }),
                            );
                            text(&session, r#"{"status":"done","#);
                            text(&session, r#""summary":"Read it"}"#);
                            send(
                                json!({ "jsonrpc": "2.0", "id": prompt_id, "result": { "stopReason": "end_turn" } }),
                            );
                        }
                    }
                }
                "" => {
                    // A response to one of our requests.
                    if let Some((prompt_id, request_id)) = pending_permission {
                        if id.as_ref().and_then(Value::as_u64) == Some(request_id) {
                            pending_permission = None;
                            record("permissionReply", msg.clone());
                            let selected = msg
                                .pointer("/result/outcome/optionId")
                                .and_then(Value::as_str);
                            if request_id == 900 && selected != Some("reject") {
                                text(&session, r#"{"status":"done","summary":"ran it"}"#);
                                send(
                                    json!({ "jsonrpc": "2.0", "id": prompt_id, "result": { "stopReason": "end_turn" } }),
                                );
                            } else if request_id == 900 {
                                update(
                                    &session,
                                    json!({ "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "failed" }),
                                );
                                send(
                                    json!({ "jsonrpc": "2.0", "id": prompt_id, "result": { "stopReason": "cancelled" } }),
                                );
                            } else {
                                text(&session, r#"{"status":"done","summary":"no file access"}"#);
                                send(
                                    json!({ "jsonrpc": "2.0", "id": prompt_id, "result": { "stopReason": "end_turn" } }),
                                );
                            }
                        }
                    }
                }
                other => {
                    send(
                        json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("unknown {other}") } }),
                    );
                }
            }
        }
    }
}
