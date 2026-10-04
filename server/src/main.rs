use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

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

/// Desktop launchers often omit common bin dirs; probes and runs share this PATH.
fn augment_process_path() {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let current = std::env::var_os("PATH").unwrap_or_default();
    let path = coppice_connectors::probe::augment_path(std::path::Path::new(&home), &current);
    std::env::set_var("PATH", path);
}

/// PATH is set before the tokio runtime exists, so no other thread can read
/// the environment concurrently.
fn main() -> anyhow::Result<()> {
    augment_process_path();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> anyhow::Result<()> {
    let rust_log = std::env::var(EnvFilter::DEFAULT_ENV).ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(log_directives(rust_log.as_deref()))
                .unwrap_or_else(|_| EnvFilter::new(log_directives(None))),
        )
        .init();

    let config = coppice_server::AppConfig::load()
        .map_err(|e| anyhow::anyhow!("failed to load config: {e}"))?;

    let addr: SocketAddr = format!("0.0.0.0:{}", config.server.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    serve(config, listener, ServeOptions::default(), async {
        tokio::signal::ctrl_c().await.ok();
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::log_directives;

    #[test]
    fn rmcp_defaults_to_warn_unless_named() {
        assert_eq!(log_directives(None), "info,rmcp=warn");
        assert_eq!(log_directives(Some(" ")), "info,rmcp=warn");
        assert_eq!(log_directives(Some("debug")), "debug,rmcp=warn");
        assert_eq!(log_directives(Some("info,rmcp=debug")), "info,rmcp=debug");
    }
}
