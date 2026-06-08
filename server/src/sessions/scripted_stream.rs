use crate::sessions::{run_registry::RunStreamHandle, TerminalFrame};
use time::OffsetDateTime;
use tokio::sync::watch;

pub const MOCK_SCRIPT: &[&str] = &[
    "Mock agent starting...\n",
    "Reading .agent/context.md\n",
    "Running tests...\n",
    "Done.\n",
];

pub async fn emit_script(
    handle: &RunStreamHandle,
    cancel_rx: &mut watch::Receiver<bool>,
    lines: &[&str],
) {
    let mut seq = 0u64;
    for line in lines {
        if *cancel_rx.borrow() {
            break;
        }
        handle.publish(TerminalFrame {
            seq,
            data: line.as_bytes().to_vec(),
            ts: OffsetDateTime::now_utc(),
        });
        seq += 1;
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }
}
