//! Stand-in MCP server over stdio (newline-delimited JSON-RPC) for proxy tests.
//! Tools: `echo`, `write_note`, `env` (`wrap: true` answers `token=<value>; done`,
//! `error: true` marks the result `isError`), `sleep`, `crash`. Env switches:
//! `FAKE_MCP_START_FAIL=1` (exit 1 before reading), `FAKE_MCP_TOOLS_CHANGED=1`
//! (after the first `tools/call`, add tool `extra` and send
//! `notifications/tools/list_changed`), `FAKE_MCP_PID_FILE` (write own pid,
//! relative to cwd), `FAKE_MCP_INIT_SLEEP_MS` (sleep before answering `initialize`),
//! `FAKE_MCP_STDERR_GARBAGE=1` (invalid UTF-8 on stderr at startup and on every `tools/call`).

use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::time::Duration;

fn tool(name: &str, description: &str, read_only: bool) -> Value {
    let mut tool = json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object" },
    });
    if read_only {
        tool["annotations"] = json!({ "readOnlyHint": true });
    }
    tool
}

fn tools(extra: bool) -> Vec<Value> {
    let mut tools = vec![
        tool("echo", "Echo args.text", true),
        tool("write_note", "Pretend to write a note", false),
        tool("env", "Read env var args.name", true),
        tool("sleep", "Sleep args.ms milliseconds", false),
        tool("crash", "Exit with status 1", false),
    ];
    if extra {
        tools.push(tool("extra", "Added after the first call", true));
    }
    tools
}

fn text_result(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
}

fn send(out: &mut impl Write, msg: Value) {
    writeln!(out, "{msg}").expect("write stdout");
    out.flush().expect("flush stdout");
}

fn stderr_garbage() {
    let mut err = std::io::stderr();
    err.write_all(b"fake-mcp: \xff\xfe\xfd not utf-8\n")
        .expect("write stderr");
    err.flush().expect("flush stderr");
}

fn main() {
    if std::env::var("FAKE_MCP_START_FAIL").as_deref() == Ok("1") {
        eprintln!("fake-mcp: start failure requested");
        std::process::exit(1);
    }
    if let Ok(path) = std::env::var("FAKE_MCP_PID_FILE") {
        std::fs::write(path, std::process::id().to_string()).expect("write pid file");
    }
    let tools_changed = std::env::var("FAKE_MCP_TOOLS_CHANGED").as_deref() == Ok("1");
    let init_sleep_ms = std::env::var("FAKE_MCP_INIT_SLEEP_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());
    let garbage = std::env::var("FAKE_MCP_STDERR_GARBAGE").as_deref() == Ok("1");
    if garbage {
        stderr_garbage();
    }
    let mut extra = false;
    let mut out = std::io::stdout().lock();
    eprintln!("fake-mcp: ready");

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = serde_json::from_str(&line).expect("parse json-rpc");
        let Some(id) = msg.get("id").cloned() else {
            continue;
        };
        let params = &msg["params"];
        let result = match msg["method"].as_str().unwrap_or_default() {
            "initialize" => {
                if let Some(ms) = init_sleep_ms {
                    std::thread::sleep(Duration::from_millis(ms));
                }
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": { "tools": { "listChanged": true } },
                    "serverInfo": { "name": "fake-mcp", "version": "0.0.0" },
                })
            }
            "tools/list" => json!({ "tools": tools(extra) }),
            "tools/call" => {
                if garbage {
                    stderr_garbage();
                }
                let args = &params["arguments"];
                let result = match params["name"].as_str().unwrap_or_default() {
                    "echo" | "extra" => text_result(args["text"].as_str().unwrap_or_default()),
                    "write_note" => text_result("noted"),
                    "env" => {
                        let name = args["name"].as_str().unwrap_or_default();
                        let value = std::env::var(name).unwrap_or_else(|_| "<unset>".into());
                        let text = if args["wrap"] == true {
                            format!("token={value}; done")
                        } else {
                            value
                        };
                        let mut result = text_result(&text);
                        result["isError"] = (args["error"] == true).into();
                        result
                    }
                    "sleep" => {
                        let ms = args["ms"].as_u64().unwrap_or(0);
                        std::thread::sleep(Duration::from_millis(ms));
                        text_result("slept")
                    }
                    "crash" => std::process::exit(1),
                    other => json!({
                        "content": [{ "type": "text", "text": format!("unknown tool {other}") }],
                        "isError": true,
                    }),
                };
                send(
                    &mut out,
                    json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                );
                if tools_changed && !extra {
                    extra = true;
                    send(
                        &mut out,
                        json!({ "jsonrpc": "2.0", "method": "notifications/tools/list_changed" }),
                    );
                }
                continue;
            }
            other => {
                send(
                    &mut out,
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": format!("method not found: {other}") },
                    }),
                );
                continue;
            }
        };
        send(
            &mut out,
            json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        );
    }
}
