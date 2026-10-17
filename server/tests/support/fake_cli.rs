//! Stand-in for a streaming agent CLI in integration tests, driven by env:
//! `FAKE_CLI_PRINT_PID` (print `{"pid":N}` first), `FAKE_CLI_ECHO_STDIN`
//! (print `{"stdin":"..."}` with all of stdin), `FAKE_CLI_GRANDCHILD_SLEEP_MS`
//! (spawn a `sleep` grandchild inheriting stdio, print `{"grandchild_pid":N}`),
//! `FAKE_CLI_BACKGROUND_SLEEP_SECS` (spawn `sleep` with stdio closed so it
//! outlives this process, print `{"background_pid":N}`),
//! `FAKE_CLI_LINES`
//! (newline-separated stdout lines), `FAKE_CLI_STDERR` (newline-separated
//! stderr lines), `FAKE_CLI_SLEEP_MS` (sleep after printing), `FAKE_CLI_EXIT`.
//! `FAKE_CLI_REJECT_SESSION` exits 1 with `Session not found` on stderr when
//! argv contains `--session` or `-s`. It first prints a JSON `error` event
//! carrying that session id, the way a missing Kilo session can, then exits
//! before any other stdout.
//! Any of these may also come from a JSON object in `./.fake-cli-env.json`,
//! for callers (connector adapters) that spawn the bin without custom env.

use std::io::{Read, Write};

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("fake-cli 0.0.0");
        return;
    }
    if let Ok(raw) = std::fs::read_to_string(".fake-cli-env.json") {
        let vars: std::collections::HashMap<String, String> =
            serde_json::from_str(&raw).expect("parse .fake-cli-env.json");
        for (key, value) in vars {
            std::env::set_var(key, value);
        }
    }
    if std::env::var_os("FAKE_CLI_REJECT_SESSION").is_some()
        && std::env::args().any(|arg| arg == "--session" || arg == "-s")
    {
        let rejected = std::env::args()
            .skip_while(|arg| arg != "--session" && arg != "-s")
            .nth(1);
        let mut stdout = std::io::stdout();
        if let Some(sid) = rejected {
            writeln!(
                stdout,
                "{}",
                serde_json::json!({
                    "type": "error",
                    "sessionID": sid,
                    "error": { "message": "Session not found" }
                })
            )
            .expect("write stdout");
            stdout.flush().expect("flush stdout");
        }
        let mut stderr = std::io::stderr();
        writeln!(stderr, "Session not found").expect("write stderr");
        stderr.flush().expect("flush stderr");
        std::process::exit(1);
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
        // Dropping the child would leave a zombie. Forget it so the sleeper
        // stays in this process group until the test reaps the group.
        std::mem::forget(grandchild);
    }
    if let Some(secs) = std::env::var("FAKE_CLI_BACKGROUND_SLEEP_SECS")
        .ok()
        .filter(|value| !value.is_empty())
    {
        let background = std::process::Command::new("sleep")
            .arg(secs)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn background sleep");
        writeln!(stdout, "{{\"background_pid\":{}}}", background.id()).expect("write pid");
        // `process::exit` skips destructors; forget so a normal return cannot
        // reap or kill the sleeper either. It stays in this process group.
        std::mem::forget(background);
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
