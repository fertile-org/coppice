use std::io::Read;

use tokio::signal::unix::{signal, SignalKind};

/// Resolves on SIGTERM, SIGINT, or stdin EOF (the parent Electron process died).
///
/// stdin is always a pipe from Electron, so `coppice-server desktop </dev/null`
/// shuts down as soon as it is ready. Signal handlers are installed on first poll.
pub async fn desktop_shutdown_signal() {
    tokio::select! {
        () = unix_signal(SignalKind::terminate()) => tracing::info!("SIGTERM received; shutting down"),
        () = unix_signal(SignalKind::interrupt()) => tracing::info!("SIGINT received; shutting down"),
        () = stdin_eof() => tracing::info!("stdin closed; shutting down"),
    }
}

async fn unix_signal(kind: SignalKind) {
    match signal(kind) {
        Ok(mut stream) => {
            stream.recv().await;
        }
        Err(err) => {
            tracing::warn!(error = %err, ?kind, "cannot listen for signal");
            std::future::pending::<()>().await;
        }
    }
}

/// A detached thread, not `tokio::io::stdin`, so a blocked read never holds up
/// runtime shutdown.
async fn stdin_eof() {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 1024];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        let _ = tx.send(());
    });
    let _ = rx.await;
}
