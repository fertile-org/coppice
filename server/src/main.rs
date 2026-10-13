use std::net::SocketAddr;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

use coppice_server::desktop;
use coppice_server::serve::{serve, ServeOptions};

/// rmcp logs plugin-controlled content (notifications, peer info) below warn, so it
/// stays at warn unless the operator's `RUST_LOG` names it.
fn log_directives(rust_log: Option<&str>) -> String {
    let base = rust_log.filter(|s| !s.trim().is_empty()).unwrap_or("info");
    if base.contains("rmcp") {
        base.to_string()
    } else {
        format!("{base},rmcp=warn")
    }
}

fn init_tracing() {
    let rust_log = std::env::var(EnvFilter::DEFAULT_ENV).ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(log_directives(rust_log.as_deref()))
                .unwrap_or_else(|_| EnvFilter::new(log_directives(None))),
        )
        .init();
}

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

/// The environment is set before the tokio runtime exists, so no other thread
/// can read it concurrently.
fn version_text() -> String {
    format!(
        "coppice-server {}\ncommit {}\n",
        env!("CARGO_PKG_VERSION"),
        env!("COPPICE_BUILD_COMMIT")
    )
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--version") {
        print!("{}", version_text());
        return Ok(());
    }
    match desktop::parse_desktop_args(&args) {
        Ok(Some(args)) => run_desktop(args),
        Ok(None) => runtime()?.block_on(run()),
        Err(err) => {
            eprintln!("coppice-server: {err}");
            std::process::exit(2);
        }
    }
}

/// Electron shows stderr when startup fails, so the full error chain goes there.
fn run_desktop(args: desktop::DesktopArgs) -> anyhow::Result<()> {
    desktop::set_process_env(&args);
    let result = runtime().map_err(anyhow::Error::from).and_then(|runtime| {
        let result = runtime.block_on(async {
            init_tracing();
            desktop::run(args).await
        });
        runtime.shutdown_timeout(Duration::from_secs(1));
        result
    });
    if let Err(err) = result {
        eprintln!("coppice-server desktop: {err:#}");
        std::process::exit(1);
    }
    Ok(())
}

async fn run() -> anyhow::Result<()> {
    init_tracing();

    let config = coppice_server::AppConfig::load()
        .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;

    let addr: SocketAddr = format!("0.0.0.0:{}", config.server.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    serve(
        config,
        listener,
        ServeOptions {
            config_path: Some(coppice_server::AppConfig::writable_config_path()),
            ..ServeOptions::default()
        },
        server_shutdown_signal(),
    )
    .await
}

/// Docker stop and a terminal both need to reach the agent-tree cleanup in
/// `serve`. SIGINT alone (ctrl-c) misses `SIGTERM`.
async fn server_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(stream) => stream,
        Err(err) => {
            tracing::warn!(error = %err, "cannot listen for SIGTERM");
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    let mut interrupt = match signal(SignalKind::interrupt()) {
        Ok(stream) => stream,
        Err(err) => {
            tracing::warn!(error = %err, "cannot listen for SIGINT");
            let _ = terminate.recv().await;
            return;
        }
    };
    tokio::select! {
        _ = terminate.recv() => tracing::info!("SIGTERM received; shutting down"),
        _ = interrupt.recv() => tracing::info!("SIGINT received; shutting down"),
    }
}

#[cfg(test)]
mod tests {
    use super::log_directives;

    #[test]
    fn version_text_names_the_package_and_commit() {
        let text = super::version_text();
        assert!(text.starts_with("coppice-server "));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        let commit = text
            .lines()
            .find_map(|line| line.strip_prefix("commit "))
            .expect("commit line");
        assert!(!commit.is_empty());
        assert!(!commit.contains(' '));
    }

    #[test]
    fn rmcp_defaults_to_warn_unless_named() {
        assert_eq!(log_directives(None), "info,rmcp=warn");
        assert_eq!(log_directives(Some(" ")), "info,rmcp=warn");
        assert_eq!(log_directives(Some("debug")), "debug,rmcp=warn");
        assert_eq!(log_directives(Some("info,rmcp=debug")), "info,rmcp=debug");
    }
}
