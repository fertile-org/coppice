use crate::sessions::run_registry::RunStreamHandle;
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
    for (seq, line) in lines.iter().enumerate() {
        if *cancel_rx.borrow() {
            break;
        }
        handle.publish_frame(seq as u64, line.as_bytes().to_vec());
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }
}
