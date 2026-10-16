use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use coppice_server::desktop;
use coppice_server::logs;
use coppice_server::serve::{serve, ServeOptions};

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
        Ok(Some(args)) => {
            let logs_dir = args
                .logs_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("./data/logs"));
            logs::init(&logs_dir)?;
            run_desktop(args)
        }
        Ok(None) => {
            let config = coppice_server::AppConfig::load()
                .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;
            logs::init(std::path::Path::new(&config.logs.dir))?;
            runtime()?.block_on(run(config))
        }
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
        let result = runtime.block_on(async { desktop::run(args).await });
        runtime.shutdown_timeout(Duration::from_secs(1));
        result
    });
    if let Err(err) = result {
        eprintln!("coppice-server desktop: {err:#}");
        std::process::exit(1);
    }
    Ok(())
}

async fn run(config: coppice_server::AppConfig) -> anyhow::Result<()> {
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
    fn pipe_tracing_format_keeps_status_404_contiguous() {
        use std::sync::{Arc, Mutex};

        struct Mem(Arc<Mutex<Vec<u8>>>);

        impl std::io::Write for Mem {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer = Arc::clone(&buf);
        // Same settings as `init_tracing` when stdout is not a terminal.
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::INFO)
            .without_time()
            .with_writer(move || Mem(Arc::clone(&writer)))
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(status = 404_u16, "http request");
        });
        let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(text.contains("status=404"), "{text}");
        assert!(!text.contains('\u{1b}'), "{text}");
    }
}
