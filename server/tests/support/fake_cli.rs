//! Stand-in for a streaming agent CLI in integration tests, driven by env:
//! `FAKE_CLI_PRINT_PID` (print `{"pid":N}` first), `FAKE_CLI_ECHO_STDIN`
//! (print `{"stdin":"..."}` with all of stdin), `FAKE_CLI_GRANDCHILD_SLEEP_MS`
//! (spawn a `sleep` grandchild inheriting stdio, print `{"grandchild_pid":N}`),
//! `FAKE_CLI_LINES`
//! (newline-separated stdout lines), `FAKE_CLI_STDERR` (newline-separated
//! stderr lines), `FAKE_CLI_SLEEP_MS` (sleep after printing), `FAKE_CLI_EXIT`.
//! Any of these may also come from a JSON object in `./.fake-cli-env.json`,
//! for callers (connector adapters) that spawn the bin without custom env.

use std::io::{Read, Write};

fn main() {
    if let Ok(raw) = std::fs::read_to_string(".fake-cli-env.json") {
        let vars: std::collections::HashMap<String, String> =
            serde_json::from_str(&raw).expect("parse .fake-cli-env.json");
        for (key, value) in vars {
            std::env::set_var(key, value);
        }
    }
    let mut stdout = std::io::stdout();
    if std::env::var_os("FAKE_CLI_PRINT_PID").is_some() {
        writeln!(stdout, "{{\"pid\":{}}}", std::process::id()).expect("write pid");
    }
    if let Some(ms) = std::env::var("FAKE_CLI_GRANDCHILD_SLEEP_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        let grandchild = std::process::Command::new("sleep")
            .arg(format!("{}", ms as f64 / 1000.0))
            .spawn()
            .expect("spawn grandchild");
        writeln!(stdout, "{{\"grandchild_pid\":{}}}", grandchild.id()).expect("write pid");
    }
    if std::env::var_os("FAKE_CLI_ECHO_STDIN").is_some() {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .expect("read stdin");
        writeln!(stdout, "{}", serde_json::json!({ "stdin": input })).expect("write stdin");
    }
    if let Ok(lines) = std::env::var("FAKE_CLI_LINES") {
        for line in lines.lines() {
            writeln!(stdout, "{line}").expect("write stdout");
        }
    }
    stdout.flush().expect("flush stdout");

    let mut stderr = std::io::stderr();
    if let Ok(lines) = std::env::var("FAKE_CLI_STDERR") {
        for line in lines.lines() {
            writeln!(stderr, "{line}").expect("write stderr");
        }
    }
    stderr.flush().expect("flush stderr");

    if let Some(ms) = std::env::var("FAKE_CLI_SLEEP_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }

    let code = std::env::var("FAKE_CLI_EXIT")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(0);
    std::process::exit(code);
}
