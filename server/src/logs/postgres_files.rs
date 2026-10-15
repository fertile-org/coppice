//! Postgres log files that live in the Coppice logs directory.
//!
//! `postgres.log` is the `pg_ctl -l` capture (startup lines, and the whole
//! log when the collector is off). Rotated files are
//! `postgresql-YYYY-MM-DD_HHMMSS.log`, written by `logging_collector`.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use time::OffsetDateTime;

pub const LEGACY_NAME: &str = "postgres.log";

/// `log_rotation_age` on the desktop is one day (`desktop/postgres.rs`).
/// This window is longer so the file Postgres still has open is kept when
/// its timestamp and UTC differ by a timezone, and for the moment before
/// the collector rotates.
pub const OPEN_WINDOW: time::Duration = time::Duration::hours(48);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgFile {
    pub name: String,
    pub path: PathBuf,
    /// When the collector opened a rotated file, from its name. `None` for
    /// `postgres.log`.
    pub opened: Option<OffsetDateTime>,
    pub modified: OffsetDateTime,
    pub len: u64,
}

/// Regular Postgres log files in `dir`. Symlinks are skipped.
pub fn list(dir: &Path) -> io::Result<Vec<PgFile>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        let opened = if name == LEGACY_NAME {
            None
        } else if let Some(opened) = parse_rotated(&name) {
            Some(opened)
        } else {
            continue;
        };
        let modified = system_time(meta.modified()?);
        files.push(PgFile {
            name,
            path,
            opened,
            modified,
            len: meta.len(),
        });
    }
    Ok(files)
}

/// The file Postgres is writing, when one is still fresh.
///
/// That is the Postgres log with the newest mtime inside [`OPEN_WINDOW`]
/// (a future mtime counts as fresh). A rotated file wins a tie with
/// `postgres.log`. Nothing is open when every file is older than the window:
/// Postgres is not writing, and retention may delete them.
pub fn open_names(files: &[PgFile], now: OffsetDateTime) -> BTreeSet<String> {
    let mut best: Option<&PgFile> = None;
    for file in files {
        if !is_fresh(file.modified, now) {
            continue;
        }
        best = Some(match best {
            None => file,
            Some(current) => prefer(current, file),
        });
    }
    best.map(|file| {
        let mut names = BTreeSet::new();
        names.insert(file.name.clone());
        names
    })
    .unwrap_or_default()
}

fn prefer<'a>(current: &'a PgFile, next: &'a PgFile) -> &'a PgFile {
    if next.modified > current.modified {
        return next;
    }
    if next.modified < current.modified {
        return current;
    }
    if next.opened.is_some() && current.opened.is_none() {
        return next;
    }
    current
}

fn is_fresh(modified: OffsetDateTime, now: OffsetDateTime) -> bool {
    if modified > now {
        return true;
    }
    now - modified <= OPEN_WINDOW
}

fn system_time(time: SystemTime) -> OffsetDateTime {
    OffsetDateTime::from(time)
}

/// `postgresql-YYYY-MM-DD_HHMMSS.log` → that UTC timestamp.
pub fn parse_rotated(name: &str) -> Option<OffsetDateTime> {
    let rest = name.strip_prefix("postgresql-")?.strip_suffix(".log")?;
    let (date, clock) = rest.split_once('_')?;
    if clock.len() != 6 || !clock.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let date = parse_date(date)?;
    let hour: u8 = clock[0..2].parse().ok()?;
    let minute: u8 = clock[2..4].parse().ok()?;
    let second: u8 = clock[4..6].parse().ok()?;
    Some(date.with_hms(hour, minute, second).ok()?.assume_utc())
}

fn parse_date(value: &str) -> Option<time::Date> {
    let mut parts = value.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> OffsetDateTime {
        time::Date::from_calendar_date(2026, time::Month::October, 15)
            .unwrap()
            .with_hms(12, 0, 0)
            .unwrap()
            .assume_utc()
    }

    fn file(name: &str, hours_ago: i64) -> PgFile {
        PgFile {
            name: name.to_string(),
            path: PathBuf::from(name),
            opened: parse_rotated(name),
            modified: now() - time::Duration::hours(hours_ago),
            len: 1,
        }
    }

    #[test]
    fn a_rotated_name_parses_and_a_near_name_does_not() {
        let opened = parse_rotated("postgresql-2026-10-15_153045.log").unwrap();
        assert_eq!(
            opened,
            time::Date::from_calendar_date(2026, time::Month::October, 15)
                .unwrap()
                .with_hms(15, 30, 45)
                .unwrap()
                .assume_utc()
        );
        assert!(parse_rotated("postgres.log").is_none());
        assert!(parse_rotated("postgresql-2026-10-15.log").is_none());
        assert!(parse_rotated("postgresql-2026-13-15_000000.log").is_none());
        assert!(parse_rotated("postgresql-2026-10-15_000000.log.bak").is_none());
    }

    #[test]
    fn the_fresh_file_with_the_newest_mtime_is_the_open_one() {
        let legacy = file(LEGACY_NAME, 1);
        let older = file("postgresql-2026-10-14_100000.log", 20);
        let current = file("postgresql-2026-10-15_110000.log", 0);
        let open = open_names(&[legacy, older.clone(), current.clone()], now());
        assert_eq!(open.iter().collect::<Vec<_>>(), vec![&current.name]);

        let stale = file("postgresql-2026-09-01_000000.log", 24 * 40);
        assert!(open_names(&[stale], now()).is_empty());
    }

    #[test]
    fn a_rotated_file_wins_a_tie_with_postgres_log() {
        let mut legacy = file(LEGACY_NAME, 0);
        let mut current = file("postgresql-2026-10-15_120000.log", 0);
        legacy.modified = now();
        current.modified = now();
        let open = open_names(&[legacy, current.clone()], now());
        assert_eq!(open.iter().cloned().collect::<Vec<_>>(), vec![current.name]);
    }

    #[test]
    fn list_skips_symlinks_and_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(LEGACY_NAME), b"legacy").unwrap();
        fs::write(dir.path().join("postgresql-2026-10-15_120000.log"), b"rot").unwrap();
        fs::write(dir.path().join("notes.txt"), b"no").unwrap();
        let target = dir.path().join("target.txt");
        fs::write(&target, b"secret").unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join("postgresql-2026-10-01_000000.log"))
            .unwrap();

        let mut names: Vec<_> = list(dir.path())
            .unwrap()
            .into_iter()
            .map(|file| file.name)
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "postgres.log".to_string(),
                "postgresql-2026-10-15_120000.log".to_string(),
            ]
        );
    }
}
