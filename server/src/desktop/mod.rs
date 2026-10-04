//! `coppice-server desktop`: data dir layout, bundled resources, secrets and
//! generated config for the single-user desktop app.

pub mod args;
pub mod bootstrap;
pub mod layout;
pub mod postgres;
pub mod shutdown;

use std::ffi::OsString;
use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;

use anyhow::{bail, Context};

pub use args::{parse_desktop_args, DesktopArgs};
pub use shutdown::desktop_shutdown_signal;

use crate::serve::{serve, ServeOptions};
use bootstrap::DesktopSecrets;
use layout::{DataLayout, ResourceLayout};
use postgres::DesktopPostgres;

const DATABASE_NAME: &str = "coppice";

/// A loopback port that was free a moment ago; the caller binds it next.
pub fn free_loopback_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

/// The single stdout line Electron waits for before loading the UI.
pub fn ready_line(addr: SocketAddr) -> String {
    format!("COPPICE_READY url=http://127.0.0.1:{}", addr.port())
}

/// Points resource lookups at the bundle and puts its `pg_dump`/`psql` first
/// on `PATH`. Must run before the tokio runtime starts any threads.
pub fn set_process_env(args: &DesktopArgs) -> anyhow::Result<()> {
    let resources = ResourceLayout::new(&args.resources);
    let path = prepend_path(
        &resources.pg_bin,
        &std::env::var_os("PATH").unwrap_or_default(),
    )?;
    std::env::set_var("COPPICE_AGENT_TEMPLATES_DIR", &resources.agent_templates);
    std::env::set_var("COPPICE_MOCK_FIXTURES_DIR", &resources.mock_fixtures);
    std::env::set_var("PATH", path);
    Ok(())
}

fn prepend_path(dir: &Path, current: &std::ffi::OsStr) -> anyhow::Result<OsString> {
    // An empty entry would put the working directory on PATH.
    let existing = std::env::split_paths(current).filter(|p| !p.as_os_str().is_empty());
    let paths = std::iter::once(dir.to_path_buf()).chain(existing);
    std::env::join_paths(paths).with_context(|| format!("cannot add {} to PATH", dir.display()))
}

/// Runs the desktop server until [`desktop_shutdown_signal`] fires. Postgres is
/// stopped on every exit path once it has started.
pub async fn run(args: DesktopArgs) -> anyhow::Result<()> {
    // Polled from the start so a signal during startup still shuts down cleanly.
    let shutdown = tokio::spawn(desktop_shutdown_signal());

    let resources = ResourceLayout::new(&args.resources);
    resources.validate().map_err(anyhow::Error::msg)?;
    let layout = DataLayout::new(&args.data_dir);
    layout
        .ensure_dirs()
        .with_context(|| format!("create data dir {}", layout.root.display()))?;
    let secrets = bootstrap::load_or_create_secrets(&layout.secrets_dir)
        .with_context(|| format!("load secrets from {}", layout.secrets_dir.display()))?;
    bootstrap::ensure_config_file(&layout)
        .with_context(|| format!("write {}", layout.config_file.display()))?;

    let pg = DesktopPostgres::new(&resources, &layout);
    let bundled = pg.bundled_major().await?;
    if let Some(data) = pg.data_major()? {
        if data != bundled {
            bail!(
                "database was created by PostgreSQL {data}; this app bundles PostgreSQL {bundled}"
            );
        }
    }
    pg.init_if_needed(&secrets.pg_password).await?;
    pg.clear_stale_pid()?;
    let pg_port = start_postgres(&pg).await?;

    let served = serve_with_postgres(&pg, pg_port, &layout, &resources, &secrets, async move {
        let _ = shutdown.await;
    })
    .await;
    let stopped = pg.stop().await.context("stop postgres");
    match (served, stopped) {
        (Err(err), Err(stop_err)) => {
            tracing::error!(error = %format!("{stop_err:#}"), "postgres did not stop cleanly");
            Err(err)
        }
        (served, stopped) => served.and(stopped),
    }
}

/// Retries once on a fresh port in case another process took the first one.
async fn start_postgres(pg: &DesktopPostgres) -> anyhow::Result<u16> {
    let port = free_loopback_port().context("pick postgres port")?;
    let Err(first) = pg.start(port).await else {
        return Ok(port);
    };
    tracing::warn!(error = %format!("{first:#}"), port, "postgres failed to start; retrying on another port");
    stop_after_failed_start(pg).await;
    let port = free_loopback_port().context("pick postgres port")?;
    if let Err(err) = pg.start(port).await {
        stop_after_failed_start(pg).await;
        return Err(err);
    }
    Ok(port)
}

/// `pg_ctl start -w` can time out with the server still coming up.
async fn stop_after_failed_start(pg: &DesktopPostgres) {
    if let Err(err) = pg.stop().await {
        tracing::warn!(error = %format!("{err:#}"), "stop after failed postgres start");
    }
}

async fn serve_with_postgres(
    pg: &DesktopPostgres,
    pg_port: u16,
    layout: &DataLayout,
    resources: &ResourceLayout,
    secrets: &DesktopSecrets,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let pg_url = pg
        .ensure_database(pg_port, &secrets.pg_password, DATABASE_NAME)
        .await?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("bind 127.0.0.1")?;
    let port = listener.local_addr()?.port();
    let config = bootstrap::desktop_config(layout, secrets, &pg_url, port)?;
    let options = ServeOptions {
        static_web_dir: Some(resources.web.clone()),
        on_ready: Some(Box::new(|addr| {
            let mut stdout = std::io::stdout().lock();
            let _ = writeln!(stdout, "{}", ready_line(addr));
            let _ = stdout.flush();
        })),
    };
    serve(config, listener, options, shutdown).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_loopback_port_is_bindable() {
        let port = free_loopback_port().expect("port");
        assert_ne!(port, 0);
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("bind free port");
    }

    #[test]
    fn prepend_path_puts_dir_first() {
        let path = prepend_path(Path::new("/r/postgres/bin"), "/usr/bin:/bin".as_ref()).unwrap();
        assert_eq!(path, "/r/postgres/bin:/usr/bin:/bin");
        let path = prepend_path(Path::new("/r/postgres/bin"), "".as_ref()).unwrap();
        assert_eq!(path, "/r/postgres/bin");
    }

    #[test]
    fn ready_line_names_loopback_url() {
        let addr: SocketAddr = "127.0.0.1:5123".parse().unwrap();
        assert_eq!(ready_line(addr), "COPPICE_READY url=http://127.0.0.1:5123");
    }
}
