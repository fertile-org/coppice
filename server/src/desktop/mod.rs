//! `coppice-server desktop`: data dir layout, bundled resources, secrets and
//! generated config for the single-user desktop app.

pub mod args;
pub mod bootstrap;
pub mod layout;
pub mod postgres;
pub mod shutdown;

use std::io::Write;
use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{bail, Context};
use tokio::sync::watch;

pub use args::{parse_desktop_args, DesktopArgs};
pub use shutdown::desktop_shutdown_signal;

use crate::serve::{serve, ServeOptions};
use crate::services::backup_service::{PG_BIN_DIR_ENV, PG_LIB_DIR_ENV};
use bootstrap::DesktopSecrets;
use layout::{DataLayout, ResourceLayout};
use postgres::DesktopPostgres;

const DATABASE_NAME: &str = "coppice";
/// How long in-flight requests and plugin/OpenCode shutdown get after a signal.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// A loopback port that was free a moment ago; the caller binds it next.
pub fn free_loopback_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

/// The single stdout line Electron waits for before loading the UI.
pub fn ready_line(addr: SocketAddr) -> String {
    format!("COPPICE_READY url=http://127.0.0.1:{}", addr.port())
}

/// Points resource lookups and backup's `pg_dump`/`psql` at the bundle.
/// `PATH` is untouched so agents keep the user's own Postgres tools. Must run
/// before the tokio runtime starts any threads.
pub fn set_process_env(args: &DesktopArgs) {
    let resources = ResourceLayout::new(&args.resources);
    std::env::set_var("COPPICE_AGENT_TEMPLATES_DIR", &resources.agent_templates);
    std::env::set_var(PG_BIN_DIR_ENV, &resources.pg_bin);
    std::env::set_var(PG_LIB_DIR_ENV, &resources.pg_lib);
}

/// Not printed when shutdown was requested during startup: Electron is
/// already tearing down and must not load the UI.
fn ready_announcement(addr: SocketAddr, shutdown_requested: bool) -> Option<String> {
    (!shutdown_requested).then(|| ready_line(addr))
}

async fn shutdown_requested(mut requested: watch::Receiver<bool>) {
    // A dropped sender also ends the wait, which is the safe direction.
    let _ = requested.wait_for(|requested| *requested).await;
}

/// Runs the desktop server until [`desktop_shutdown_signal`] fires. Postgres is
/// stopped on every exit path once it has started.
///
/// Electron SIGKILLs the server 15 s after SIGTERM, so shutdown is bounded:
/// serving gets [`DRAIN_TIMEOUT`] (5 s) to drain, then `pg_ctl stop` takes at
/// most 5 s fast + 3 s immediate, and `main` gives the runtime 1 s to wind down
/// (14 s worst case, leaving 1 s of margin).
pub async fn run(args: DesktopArgs) -> anyhow::Result<()> {
    // Polled from the start so a signal during startup still shuts down cleanly.
    let (requested_tx, requested) = watch::channel(false);
    tokio::spawn(async move {
        desktop_shutdown_signal().await;
        let _ = requested_tx.send(true);
    });

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

    let served = tokio::select! {
        served = serve_with_postgres(&pg, pg_port, &layout, &resources, &secrets, requested.clone()) => served,
        () = async {
            shutdown_requested(requested.clone()).await;
            tokio::time::sleep(DRAIN_TIMEOUT).await;
        } => {
            tracing::warn!(timeout = ?DRAIN_TIMEOUT, "server did not drain in time; stopping postgres anyway");
            Ok(())
        }
    };
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

/// `pg_ctl start -w` can time out with the server still coming up. Before the
/// retry this also stops a Postgres left running on this data dir by a
/// previous SIGKILLed session (its live pid makes `start` fail), so keep it.
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
    requested: watch::Receiver<bool>,
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
        config_path: Some(layout.config_file.clone()),
        on_ready: Some(Box::new({
            let requested = requested.clone();
            move |addr| {
                let Some(line) = ready_announcement(addr, *requested.borrow()) else {
                    return;
                };
                let mut stdout = std::io::stdout().lock();
                let _ = writeln!(stdout, "{line}");
                let _ = stdout.flush();
            }
        })),
    };
    serve(config, listener, options, shutdown_requested(requested)).await
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
    fn shutdown_budget_fits_electron_grace() {
        let pg_stop: u64 = [
            postgres::FAST_STOP_TIMEOUT_SECS,
            postgres::IMMEDIATE_STOP_TIMEOUT_SECS,
        ]
        .iter()
        .map(|secs| secs.parse::<u64>().unwrap())
        .sum();
        let runtime_wind_down = 1;
        let electron_grace = 15;
        assert!(DRAIN_TIMEOUT.as_secs() + pg_stop + runtime_wind_down < electron_grace);
    }

    #[test]
    fn ready_announcement_is_skipped_once_shutdown_was_requested() {
        let addr: SocketAddr = "127.0.0.1:5123".parse().unwrap();
        assert_eq!(
            ready_announcement(addr, false).as_deref(),
            Some("COPPICE_READY url=http://127.0.0.1:5123")
        );
        assert_eq!(ready_announcement(addr, true), None);
    }

    #[test]
    fn ready_line_names_loopback_url() {
        let addr: SocketAddr = "127.0.0.1:5123".parse().unwrap();
        assert_eq!(ready_line(addr), "COPPICE_READY url=http://127.0.0.1:5123");
    }
}
