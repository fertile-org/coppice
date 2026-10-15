//! Age-based deletion of daily JSONL logs.
//!
//! On process start, and every hour after that. The sweep reads `retention_days`
//! from the on-disk config so a Save applies on the next pass.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use time::{Date, OffsetDateTime};

use coppice_config::settings_file;

const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60);

pub fn spawn_sweep(state: Arc<crate::AppState>) {
    tokio::spawn(async move {
        sweep(&state);
        loop {
            tokio::time::sleep(SWEEP_INTERVAL).await;
            sweep(&state);
        }
    });
}

fn sweep(state: &crate::AppState) {
    let days = configured_retention_days(
        state.config.logs.effective_retention_days(),
        state.connectors.config_path().as_deref(),
    );
    let today = OffsetDateTime::now_utc().date();
    if let Err(error) = delete_expired(Path::new(&state.config.logs.dir), days, today) {
        tracing::warn!(error = %error, "log retention sweep failed");
    }
}

/// Days from the config file when it validates, otherwise `fallback`.
/// A value other than 3, 7, 14, or 30 is 14.
pub fn configured_retention_days(fallback: u32, path: Option<&Path>) -> u32 {
    let from_file = path.and_then(|path| {
        if !path.is_file() {
            return None;
        }
        let text = fs::read_to_string(path).ok()?;
        let config = settings_file::validate_config_text(&text).ok()?;
        Some(config.logs.effective_retention_days())
    });
    coerce(from_file.unwrap_or(fallback))
}

fn coerce(days: u32) -> u32 {
    if coppice_config::LOG_RETENTION_DAYS.contains(&days) {
        days
    } else {
        14
    }
}

/// Delete `coppice.YYYY-MM-DD.jsonl` and `.jsonl.zst` whose calendar age is
/// at least `retention_days`. Today's file stays. Symlinks, including
/// `current.jsonl`, are left in place. `postgres.log` is not a candidate.
pub fn delete_expired(dir: &Path, retention_days: u32, today: Date) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let retention_days = i64::from(coerce(retention_days));
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(date) = day_from_name(&name) else {
            continue;
        };
        let age = i64::from(today.to_julian_day()) - i64::from(date.to_julian_day());
        if age >= retention_days {
            if let Err(error) = fs::remove_file(&path) {
                tracing::warn!(error = %error, path = %path.display(), "failed to delete an expired log");
            }
        }
    }
    Ok(())
}

fn day_from_name(name: &str) -> Option<Date> {
    let rest = name.strip_prefix("coppice.")?;
    let date = if let Some(date) = rest.strip_suffix(".jsonl.zst") {
        date
    } else {
        rest.strip_suffix(".jsonl")?
    };
    let mut parts = date.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> Date {
        Date::from_calendar_date(2026, time::Month::October, 15).expect("date")
    }

    fn write(dir: &Path, name: &str) {
        fs::write(dir.join(name), b"line\n").expect("write");
    }

    #[test]
    fn age_deletes_a_day_at_the_retention_boundary_and_keeps_a_newer_day() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "coppice.2026-10-15.jsonl");
        write(dir.path(), "coppice.2026-10-02.jsonl");
        write(dir.path(), "coppice.2026-10-01.jsonl");
        write(dir.path(), "postgres.log");
        write(dir.path(), "server.log");
        std::os::unix::fs::symlink("coppice.2026-10-15.jsonl", dir.path().join("current.jsonl"))
            .expect("symlink");

        delete_expired(dir.path(), 14, today()).expect("sweep");

        assert!(dir.path().join("coppice.2026-10-15.jsonl").is_file());
        assert!(dir.path().join("coppice.2026-10-02.jsonl").is_file());
        assert!(!dir.path().join("coppice.2026-10-01.jsonl").exists());
        assert!(dir.path().join("postgres.log").is_file());
        assert!(dir.path().join("server.log").is_file());
        assert!(dir.path().join("current.jsonl").symlink_metadata().is_ok());
    }

    #[test]
    fn a_compressed_day_expires_and_a_newer_one_stays() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "coppice.2026-10-15.jsonl");
        write(dir.path(), "coppice.2026-10-02.jsonl.zst");
        write(dir.path(), "coppice.2026-10-01.jsonl.zst");
        std::os::unix::fs::symlink(
            "coppice.2026-10-01.jsonl.zst",
            dir.path().join("coppice.2020-01-01.jsonl"),
        )
        .expect("symlink");

        delete_expired(dir.path(), 14, today()).expect("sweep");

        assert!(dir.path().join("coppice.2026-10-15.jsonl").is_file());
        assert!(dir.path().join("coppice.2026-10-02.jsonl.zst").is_file());
        assert!(!dir.path().join("coppice.2026-10-01.jsonl.zst").exists());
        assert!(dir
            .path()
            .join("coppice.2020-01-01.jsonl")
            .symlink_metadata()
            .expect("old symlink")
            .file_type()
            .is_symlink());
    }

    #[test]
    fn age_uses_calendar_days_across_a_month() {
        let dir = tempfile::tempdir().expect("tempdir");
        let march = Date::from_calendar_date(2026, time::Month::March, 1).expect("date");
        write(dir.path(), "coppice.2026-02-15.jsonl");
        write(dir.path(), "coppice.2026-02-16.jsonl");

        delete_expired(dir.path(), 14, march).expect("sweep");

        assert!(!dir.path().join("coppice.2026-02-15.jsonl").exists());
        assert!(dir.path().join("coppice.2026-02-16.jsonl").is_file());
    }

    #[test]
    fn retention_is_read_from_the_config_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        fs::write(&path, "[logs]\nretention_days = 7\n").expect("write");
        assert_eq!(configured_retention_days(14, Some(&path)), 7);
    }

    #[test]
    fn an_invalid_retention_in_the_file_uses_the_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        fs::write(&path, "[logs]\nretention_days = 10\n").expect("write");
        assert_eq!(configured_retention_days(30, Some(&path)), 30);
        assert_eq!(configured_retention_days(99, None), 14);
    }

    #[test]
    fn a_missing_log_directory_is_left_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope");
        delete_expired(&missing, 14, today()).expect("missing");
    }
}
