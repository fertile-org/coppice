//! `coppice-server desktop`: data dir layout, bundled resources, secrets and
//! generated config for the single-user desktop app.

mod app_role;
pub mod args;
pub mod bootstrap;
pub mod layout;
pub mod legacy_config;
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
/// How long in-flight requests, agent-tree shutdown, and plugin/OpenCode
/// shutdown get after a signal. Agent groups get 3 s of that for SIGTERM
/// before SIGKILL (`process_tree::TERM_GRACE`); a dropped drain SIGKILLs them.
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
    let layout = match &args.logs_dir {
        Some(logs_dir) => DataLayout::with_logs_dir(&args.data_dir, logs_dir.clone()),
        None => DataLayout::new(&args.data_dir),
    };
    layout
        .ensure_dirs()
        .with_context(|| format!("create data dir {}", layout.root.display()))?;
    let secrets = bootstrap::load_or_create_secrets(&layout.secrets_dir)
        .with_context(|| format!("load secrets from {}", layout.secrets_dir.display()))?;
    bootstrap::ensure_config_file(&layout)
        .with_context(|| format!("write {}", layout.config_file.display()))?;
    let import_notice = match legacy_config::import_legacy_config(&layout.config_file) {
        Ok(notice) => notice,
        Err(err) => {
            tracing::warn!(error = %err, "legacy config import failed");
            None
        }
    };

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
    reject_leftover(pg.stop_leftover().await.context("stop leftover postgres")?)?;
    if *requested.borrow() {
        return Ok(());
    }
    let pg_port = match start_postgres(&pg, requested.clone()).await? {
        Startup::Shutdown => return Ok(()),
        Startup::Ready(port) => port,
    };

    let served = tokio::select! {
        served = serve_with_postgres(&pg, pg_port, &layout, &resources, &secrets, import_notice, requested.clone()) => served,
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

enum Startup {
    Ready(u16),
    Shutdown,
}

/// Retries once on a fresh port in case another process took the first one.
/// A shutdown during the wait stops Postgres and returns [`Startup::Shutdown`]
/// instead of waiting out the 60 s start timeout.
async fn start_postgres(
    pg: &DesktopPostgres,
    cancel: watch::Receiver<bool>,
) -> anyhow::Result<Startup> {
    if *cancel.borrow() {
        return Ok(Startup::Shutdown);
    }
    let port = free_loopback_port().context("pick postgres port")?;
    match pg.start(port, cancel.clone()).await {
        Ok(postgres::StartStatus::Started) => Ok(Startup::Ready(port)),
        Ok(postgres::StartStatus::Cancelled) => Ok(Startup::Shutdown),
        Err(first) => {
            if postgres::data_directory_in_use(&first) {
                stop_after_failed_start(pg).await;
                return Err(blocked_database_error(first));
            }
            tracing::warn!(error = %format!("{first:#}"), port, "postgres failed to start; retrying on another port");
            stop_after_failed_start(pg).await;
            if *cancel.borrow() {
                return Ok(Startup::Shutdown);
            }
            let port = free_loopback_port().context("pick postgres port")?;
            match pg.start(port, cancel).await {
                Ok(postgres::StartStatus::Started) => Ok(Startup::Ready(port)),
                Ok(postgres::StartStatus::Cancelled) => Ok(Startup::Shutdown),
                Err(err) => {
                    stop_after_failed_start(pg).await;
                    Err(blocked_database_error(err))
                }
            }
        }
    }
}

fn reject_leftover(status: postgres::LeftoverStop) -> anyhow::Result<()> {
    match status {
        postgres::LeftoverStop::Stopped => {
            tracing::info!("stopped a database left running from an earlier session");
            Ok(())
        }
        postgres::LeftoverStop::NotRunning => Ok(()),
        postgres::LeftoverStop::LeftRunning => bail!(postgres::DATABASE_STILL_RUNNING),
    }
}

fn blocked_database_error(err: anyhow::Error) -> anyhow::Error {
    if postgres::data_directory_in_use(&err) {
        tracing::warn!(error = %format!("{err:#}"), "postgres data directory is still in use");
        anyhow::anyhow!(postgres::DATABASE_STILL_RUNNING)
    } else {
        err
    }
}

/// `pg_ctl start -w` can time out with the server still coming up.
async fn stop_after_failed_start(pg: &DesktopPostgres) {
    if let Err(err) = pg.stop().await {
        tracing::warn!(error = %format!("{err:#}"), "stop after failed postgres start");
    }
}

/// Directory the Logs page reads. `--logs-dir` (anything other than
/// `<data-dir>/logs`) replaces `[logs] dir`, matching where the writer landed.
pub(crate) fn page_logs_dir(config_dir: &str, layout: &DataLayout) -> String {
    if layout.logs != layout.root.join("logs") {
        layout.logs.to_string_lossy().into_owned()
    } else {
        config_dir.to_string()
    }
}

async fn serve_with_postgres(
    pg: &DesktopPostgres,
    pg_port: u16,
    layout: &DataLayout,
    resources: &ResourceLayout,
    secrets: &DesktopSecrets,
    import_notice: Option<String>,
    requested: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let pg_password = crate::services::secret_rotation::finish_file_password(
        &layout.secrets_dir,
        "127.0.0.1",
        pg_port,
        postgres::SUPERUSER,
        "postgres",
    )
    .await
    .map_err(|err| anyhow::anyhow!("could not finish replacing keys: {err}"))?;
    let mut secrets = secrets.clone();
    secrets.pg_password = pg_password;
    // Provision before settling a pending app password: creating the role
    // must not overwrite a password that already landed in PostgreSQL.
    let mut pg_url = pg
        .ensure_database(
            pg_port,
            &secrets.pg_password,
            &secrets.pg_app_password,
            DATABASE_NAME,
            &layout.root.join(app_role::READY_FLAG_FILE),
        )
        .await?;
    let app_password = crate::services::secret_rotation::finish_app_password(
        &layout.secrets_dir,
        "127.0.0.1",
        pg_port,
        app_role::APP_ROLE,
        DATABASE_NAME,
    )
    .await
    .map_err(|err| anyhow::anyhow!("could not finish replacing keys: {err}"))?;
    if app_password != secrets.pg_app_password {
        pg_url = app_role::app_database_url(pg_port, &app_password, DATABASE_NAME)?;
        secrets.pg_app_password = app_password;
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("bind 127.0.0.1")?;
    let port = listener.local_addr()?.port();
    let mut config = bootstrap::desktop_config(layout, &secrets, &pg_url, port)?;
    config.logs.dir = page_logs_dir(&config.logs.dir, layout);
    let options = ServeOptions {
        static_web_dir: Some(resources.web.clone()),
        config_path: Some(layout.config_file.clone()),
        import_notice,
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
    fn a_leftover_left_running_is_the_startup_error() {
        let err = reject_leftover(postgres::LeftoverStop::LeftRunning).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Coppice couldn't start because an earlier Coppice database may still be running. Quit any other Coppice window, or restart your computer, then open Coppice again."
        );
        assert!(reject_leftover(postgres::LeftoverStop::NotRunning).is_ok());
        assert!(reject_leftover(postgres::LeftoverStop::Stopped).is_ok());
    }

    #[test]
    fn a_data_directory_still_in_use_is_the_same_startup_error() {
        let raw = anyhow::anyhow!(
            "pg_ctl exited with exit status: 1\nFATAL: lock file \"postmaster.pid\" already exists\nHINT: Is another postmaster (PID 9) running in data directory \"/d\"?"
        );
        assert_eq!(
            blocked_database_error(raw).to_string(),
            postgres::DATABASE_STILL_RUNNING
        );
        let shared = anyhow::anyhow!(
            "FATAL: pre-existing shared memory block is still in use\nHINT: Check if there are any old server processes still running"
        );
        assert_eq!(
            blocked_database_error(shared).to_string(),
            postgres::DATABASE_STILL_RUNNING
        );
        let trying = anyhow::anyhow!(
            "pg_ctl: another server might be running; trying to start server anyway"
        );
        assert_eq!(
            blocked_database_error(trying).to_string(),
            postgres::DATABASE_STILL_RUNNING
        );
        let other = anyhow::anyhow!(
            "pg_ctl exited with exit status: 1\ninitdb: error: could not create directory"
        );
        assert!(blocked_database_error(other).to_string().contains("initdb"));
    }

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
        let startup_cancel = postgres::START_INTERRUPT_WAIT.as_secs()
            + postgres::START_PID_POLL.as_secs()
            + pg_stop
            + runtime_wind_down;
        assert!(
            startup_cancel < electron_grace,
            "quit during pg_ctl start must stop postgres before Electron SIGKILL ({startup_cancel}s)"
        );
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

    #[test]
    fn logs_dir_flag_replaces_the_config_dir() {
        let root = std::path::Path::new("/data");
        let flagged =
            super::DataLayout::with_logs_dir(root, std::path::PathBuf::from("/var/log/Coppice"));
        assert_eq!(
            super::page_logs_dir("./data/logs", &flagged),
            "/var/log/Coppice"
        );
        let plain = super::DataLayout::new(root);
        assert_eq!(super::page_logs_dir("./data/logs", &plain), "./data/logs");
    }
}
