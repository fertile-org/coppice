use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use anyhow::{bail, Context};
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Connection};
use tokio::process::Command;

use super::layout::{DataLayout, ResourceLayout};

const SUPERUSER: &str = "coppice";
const START_TIMEOUT_SECS: &str = "60";

/// The bundled Postgres cluster in `D/pg/data`, run from `R/postgres`.
#[derive(Debug, Clone)]
pub struct DesktopPostgres {
    pg_bin: PathBuf,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pg_lib: PathBuf,
    pg_data: PathBuf,
    pg_log: PathBuf,
}

impl DesktopPostgres {
    pub fn new(resources: &ResourceLayout, layout: &DataLayout) -> Self {
        Self {
            pg_bin: resources.pg_bin.clone(),
            pg_lib: resources.pg_lib.clone(),
            pg_data: layout.pg_data.clone(),
            pg_log: layout.pg_log.clone(),
        }
    }

    pub async fn bundled_major(&self) -> anyhow::Result<u32> {
        let output = self
            .run("postgres", |cmd| {
                cmd.arg("--version");
            })
            .await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_major(&stdout).with_context(|| {
            format!(
                "unrecognised `postgres --version` output: {}",
                stdout.trim()
            )
        })
    }

    /// Major version the cluster was initialised with; `None` before `initdb`.
    pub fn data_major(&self) -> anyhow::Result<Option<u32>> {
        let path = self.pg_data.join("PG_VERSION");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err).with_context(|| format!("read {}", path.display())),
        };
        contents
            .trim()
            .parse()
            .map(Some)
            .with_context(|| format!("{} is corrupt: {:?}", path.display(), contents.trim()))
    }

    /// Runs `initdb` when the cluster is uninitialised; returns true when it did.
    pub async fn init_if_needed(&self, password: &str) -> anyhow::Result<bool> {
        if self.data_major()?.is_some() {
            return Ok(false);
        }
        std::fs::create_dir_all(&self.pg_data)
            .with_context(|| format!("create {}", self.pg_data.display()))?;
        set_private_dir(&self.pg_data)
            .with_context(|| format!("chmod 0700 {}", self.pg_data.display()))?;

        let mut pwfile = tempfile::NamedTempFile::new().context("create initdb password file")?;
        pwfile
            .write_all(password.as_bytes())
            .and_then(|()| pwfile.flush())
            .context("write initdb password file")?;

        self.run("initdb", |cmd| {
            cmd.arg("-D")
                .arg(&self.pg_data)
                .args(["-U", SUPERUSER])
                .arg(format!("--pwfile={}", pwfile.path().display()))
                .args(["-A", "scram-sha-256", "-E", "UTF8", "--locale=C"]);
        })
        .await?;

        let conf_path = self.pg_data.join("postgresql.conf");
        let mut conf = std::fs::OpenOptions::new()
            .append(true)
            .open(&conf_path)
            .with_context(|| format!("open {}", conf_path.display()))?;
        conf.write_all(
            b"\n# Coppice desktop: TCP on loopback only, no Unix socket.\n\
              listen_addresses = '127.0.0.1'\n\
              unix_socket_directories = ''\n",
        )
        .with_context(|| format!("append {}", conf_path.display()))?;
        Ok(true)
    }

    /// Removes `postmaster.pid` left by a crashed server; returns true when removed.
    pub fn clear_stale_pid(&self) -> anyhow::Result<bool> {
        let path = self.pg_data.join("postmaster.pid");
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(err) => return Err(err).with_context(|| format!("read {}", path.display())),
        };
        let pid = contents
            .lines()
            .next()
            .and_then(|line| line.trim().parse::<u32>().ok());
        if let Some(pid) = pid {
            if is_live_postgres(pid)? {
                return Ok(false);
            }
        }
        std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        Ok(true)
    }

    pub async fn start(&self, port: u16) -> anyhow::Result<()> {
        self.run("pg_ctl", |cmd| {
            cmd.arg("-D")
                .arg(&self.pg_data)
                .arg("-l")
                .arg(&self.pg_log)
                .arg("-o")
                .arg(format!("-p {port}"))
                .args(["-w", "-t", START_TIMEOUT_SECS, "start"]);
        })
        .await
        .with_context(|| format!("postgres failed to start; see {}", self.pg_log.display()))?;
        Ok(())
    }

    /// Creates database `name` if missing and returns its connection URL.
    pub async fn ensure_database(
        &self,
        port: u16,
        password: &str,
        name: &str,
    ) -> anyhow::Result<String> {
        if !is_valid_db_name(name) {
            bail!("invalid database name {name:?}");
        }
        let mut conn = PgConnectOptions::new()
            .host("127.0.0.1")
            .port(port)
            .username(SUPERUSER)
            .password(password)
            .database("postgres")
            .connect()
            .await
            .context("connect to bundled postgres")?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(name)
                .fetch_one(&mut conn)
                .await
                .context("check database exists")?;
        if !exists {
            sqlx::query(&format!("CREATE DATABASE \"{name}\""))
                .execute(&mut conn)
                .await
                .with_context(|| format!("create database {name}"))?;
        }
        conn.close().await.ok();
        Ok(format!(
            "postgres://{SUPERUSER}:{}@127.0.0.1:{port}/{name}",
            percent_encode(password)
        ))
    }

    /// Fast shutdown; Ok when the server is not running.
    pub async fn stop(&self) -> anyhow::Result<()> {
        if !self.pg_data.join("postmaster.pid").exists() {
            return Ok(());
        }
        self.run("pg_ctl", |cmd| {
            cmd.arg("-D")
                .arg(&self.pg_data)
                .args(["-m", "fast", "-w", "stop"]);
        })
        .await?;
        Ok(())
    }

    /// Runs a bundled binary to completion; a non-zero exit is an error carrying its output.
    async fn run(&self, bin: &str, configure: impl FnOnce(&mut Command)) -> anyhow::Result<Output> {
        let mut cmd = Command::new(self.pg_bin.join(bin));
        configure(&mut cmd);
        cmd.stdin(Stdio::null());
        #[cfg(target_os = "linux")]
        cmd.env("LD_LIBRARY_PATH", &self.pg_lib);
        let output = cmd
            .output()
            .await
            .with_context(|| format!("run {}", self.pg_bin.join(bin).display()))?;
        if !output.status.success() {
            bail!(
                "{bin} exited with {}: {}{}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim(),
                String::from_utf8_lossy(&output.stdout).trim()
            );
        }
        Ok(output)
    }
}

/// Major version from `postgres --version`, e.g. `postgres (PostgreSQL) 16.4` → 16.
pub fn parse_major(version_output: &str) -> Option<u32> {
    let (_, rest) = version_output.split_once("(PostgreSQL)")?;
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn set_private_dir(path: &Path) -> io::Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

fn is_live_postgres(pid: u32) -> anyhow::Result<bool> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .stdin(Stdio::null())
        .output()
        .context("run ps")?;
    Ok(output.status.success() && String::from_utf8_lossy(&output.stdout).contains("postgres"))
}

fn is_valid_db_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c == '_')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn postgres_in(dir: &tempfile::TempDir) -> DesktopPostgres {
        let layout = DataLayout::new(&dir.path().join("Application Support").join("Coppice"));
        layout.ensure_dirs().expect("ensure dirs");
        DesktopPostgres::new(&ResourceLayout::new(&dir.path().join("res")), &layout)
    }

    #[test]
    fn parse_major_reads_release_and_prerelease_versions() {
        assert_eq!(parse_major("postgres (PostgreSQL) 16.4"), Some(16));
        assert_eq!(parse_major("postgres (PostgreSQL) 17beta1"), Some(17));
        assert_eq!(parse_major("garbage"), None);
    }

    #[test]
    fn data_major_is_none_until_pg_version_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        assert_eq!(pg.data_major().expect("empty dir"), None);

        std::fs::write(pg.pg_data.join("PG_VERSION"), "16\n").unwrap();
        assert_eq!(pg.data_major().expect("initialised"), Some(16));
    }

    #[test]
    fn clear_stale_pid_removes_dead_pid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        let pid_file = pg.pg_data.join("postmaster.pid");
        std::fs::write(&pid_file, "999999\n/data\n").unwrap();

        assert!(pg.clear_stale_pid().expect("clear"));
        assert!(!pid_file.exists());
    }

    #[test]
    fn clear_stale_pid_removes_live_non_postgres_pid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        let pid_file = pg.pg_data.join("postmaster.pid");
        std::fs::write(&pid_file, format!("{}\n/data\n", std::process::id())).unwrap();

        assert!(pg.clear_stale_pid().expect("clear"));
        assert!(!pid_file.exists());
    }

    #[test]
    fn clear_stale_pid_without_file_is_noop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        assert!(!pg.clear_stale_pid().expect("clear"));
    }

    #[test]
    fn db_names_must_be_plain_lowercase_identifiers() {
        assert!(is_valid_db_name("coppice"));
        assert!(is_valid_db_name("_test_1"));
        for bad in ["", "1db", "Coppice", "a-b", "a\"b", "a b"] {
            assert!(!is_valid_db_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn percent_encode_escapes_reserved_bytes() {
        assert_eq!(percent_encode("abc123-._~"), "abc123-._~");
        assert_eq!(percent_encode("p@ss:w/d%"), "p%40ss%3Aw%2Fd%25");
    }

    #[test]
    fn set_private_dir_makes_data_dir_owner_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pg = postgres_in(&dir);
        std::fs::set_permissions(&pg.pg_data, std::fs::Permissions::from_mode(0o755)).unwrap();

        set_private_dir(&pg.pg_data).expect("chmod");
        let mode = std::fs::metadata(&pg.pg_data).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
