//! Compression and deletion of daily JSONL logs.
//!
//! On process start, and every hour after that. The sweep reads `retention_days`
//! from the on-disk config so a Save applies on the next pass.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
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
    let now = OffsetDateTime::now_utc();
    let today = now.date();
    let dir = Path::new(&state.config.logs.dir);
    if let Err(error) = sweep_logs(dir, days, today, now, legacy_logs_dir().as_deref()) {
        tracing::warn!(error = %error, "log retention sweep failed");
    }
}

fn legacy_logs_dir() -> Option<PathBuf> {
    dirs::home_dir().and_then(|home| legacy_logs_path(&home, std::env::consts::OS))
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

/// 256 MiB. Not a config key and not a Settings control.
pub const LOG_DIR_MAX_BYTES: u64 = 268_435_456;

/// Shown in the log when the active segment, a fresh `server.log`, or the
/// database log Postgres is writing is still over [`LOG_DIR_MAX_BYTES`] after
/// older files are gone.
pub const OVER_CAP_WARNING: &str = "Logs are over 256 MiB even after cleanup. Coppice keeps the log file and the database log it is writing.";

/// Compress past days, delete by age and by the 256 MiB cap, and remove old
/// `server.log` files. `now` and `legacy_logs` let tests pin the clock and the
/// old config-folder logs directory. Pass `None` to skip that directory.
pub(crate) fn legacy_logs_path(home: &Path, os: &str) -> Option<PathBuf> {
    match os {
        "linux" => Some(home.join(".config/Coppice/logs")),
        "macos" => Some(home.join("Library/Application Support/Coppice/logs")),
        _ => None,
    }
}

pub fn sweep_logs(
    dir: &Path,
    retention_days: u32,
    today: Date,
    now: OffsetDateTime,
    legacy_logs: Option<&Path>,
) -> io::Result<()> {
    let retention_days = coerce(retention_days);
    if !is_symlink(dir) {
        compress_past_days(dir, today)?;
        delete_expired(dir, retention_days, today)?;
        delete_expired_postgres(dir, retention_days, today, now)?;
        delete_old_server_logs(dir, retention_days, now)?;
    }
    if let Some(legacy) = legacy_logs {
        if !same_directory(legacy, dir) {
            sweep_legacy(legacy, retention_days, now)?;
        }
    }
    if !is_symlink(dir) {
        enforce_size_cap(dir, today, now)?;
    }
    Ok(())
}

/// Delete `coppice.YYYY-MM-DD.jsonl` and `.jsonl.zst` whose calendar age is
/// at least `retention_days`. Today's file stays. Symlinks, including
/// `current.jsonl`, are left in place. Postgres logs are handled by
/// [`delete_expired_postgres`], not here.
pub fn delete_expired(dir: &Path, retention_days: u32, today: Date) -> io::Result<()> {
    if is_symlink(dir) || !dir.is_dir() {
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

/// Deletes rotated Postgres logs past `retention_days`, and `postgres.log`
/// once its mtime is that old. The file Postgres is writing stays.
fn delete_expired_postgres(
    dir: &Path,
    retention_days: u32,
    today: Date,
    now: OffsetDateTime,
) -> io::Result<()> {
    let files = crate::logs::postgres_files::list(dir)?;
    let open = crate::logs::postgres_files::open_names(&files, now);
    let retention_days_i = i64::from(coerce(retention_days));
    for file in files {
        if open.contains(&file.name) {
            continue;
        }
        let expired = if let Some(opened) = file.opened {
            let age = i64::from(today.to_julian_day()) - i64::from(opened.date().to_julian_day());
            age >= retention_days_i
        } else {
            file_is_old_at(file.modified, now, retention_days)
        };
        if expired {
            remove_log(&file.path);
        }
    }
    Ok(())
}

fn day_from_name(name: &str) -> Option<Date> {
    crate::logs::segments::parse_log_name(name).map(|log| log.date)
}

fn plain_day(name: &str) -> Option<Date> {
    crate::logs::segments::parse_log_name(name)
        .filter(|log| !log.compressed)
        .map(|log| log.date)
}

fn entry_name(entry: &fs::DirEntry) -> Option<String> {
    entry.file_name().into_string().ok()
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

fn same_directory(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn is_server_log_name(name: &str) -> bool {
    matches!(
        name,
        "server.log" | "server.log.1" | "server.log.2" | "server.log.3"
    )
}

fn file_is_old(meta: &fs::Metadata, now: OffsetDateTime, days: u32) -> bool {
    let Ok(modified) = meta.modified() else {
        return false;
    };
    file_is_old_at(OffsetDateTime::from(modified), now, days)
}

fn file_is_old_at(modified: OffsetDateTime, now: OffsetDateTime, days: u32) -> bool {
    if modified > now {
        return false;
    }
    now - modified >= time::Duration::days(i64::from(days))
}

fn compress_past_days(dir: &Path, today: Date) -> io::Result<()> {
    let Some(entries) = read_entries(dir)? else {
        return Ok(());
    };
    // Before the first line after midnight, the open segment is still dated
    // yesterday. It stays plain until the appender moves on.
    let active = crate::logs::segments::current_target_name(dir);
    let mut plains = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let Some(name) = entry_name(&entry) else {
            continue;
        };
        let Some(date) = plain_day(&name) else {
            continue;
        };
        if date < today && active.as_deref() != Some(name.as_str()) {
            plains.push(path);
        }
    }
    for plain in plains {
        if let Err(error) = compress_one(&plain) {
            tracing::warn!(
                error = %error,
                path = %plain.display(),
                "failed to compress a past log day"
            );
        }
    }
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn compress_one(plain: &Path) -> io::Result<()> {
    let zst = with_suffix(plain, ".zst");
    if let Ok(meta) = fs::symlink_metadata(&zst) {
        if meta.file_type().is_symlink() {
            return Ok(());
        }
        if meta.is_file() && meta.len() > 0 {
            fs::remove_file(plain)?;
            return Ok(());
        }
    }
    let tmp = with_suffix(&zst, ".tmp");
    if let Ok(meta) = fs::symlink_metadata(&tmp) {
        if meta.file_type().is_symlink() {
            return Ok(());
        }
        fs::remove_file(&tmp)?;
    }
    let mut input = File::open(plain)?;
    let output = File::create(&tmp)?;
    let mut encoder = zstd::stream::write::Encoder::new(output, zstd::DEFAULT_COMPRESSION_LEVEL)?;
    io::copy(&mut input, &mut encoder)?;
    let output = encoder.finish()?;
    output.sync_all()?;
    drop(output);
    drop(input);
    fs::rename(&tmp, &zst)?;
    fs::remove_file(plain)?;
    Ok(())
}

fn delete_old_server_logs(dir: &Path, retention_days: u32, now: OffsetDateTime) -> io::Result<()> {
    let Some(entries) = read_entries(dir)? else {
        return Ok(());
    };
    let mut old = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let Some(name) = entry_name(&entry) else {
            continue;
        };
        if is_server_log_name(&name) && file_is_old(&meta, now, retention_days) {
            old.push(path);
        }
    }
    for path in old {
        remove_log(&path);
    }
    Ok(())
}

fn enforce_size_cap(dir: &Path, today: Date, now: OffsetDateTime) -> io::Result<()> {
    enforce_size_cap_at(dir, today, now, LOG_DIR_MAX_BYTES)
}

fn enforce_size_cap_at(dir: &Path, today: Date, now: OffsetDateTime, cap: u64) -> io::Result<()> {
    let Some(entries) = read_entries(dir)? else {
        return Ok(());
    };
    struct Piece {
        key: OffsetDateTime,
        path: PathBuf,
        len: u64,
    }
    let mut days = Vec::new();
    let mut extra = 0u64;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let Some(name) = entry_name(&entry) else {
            continue;
        };
        if let Some(log) = crate::logs::segments::parse_log_name(&name) {
            days.push((log, name, path, meta.len()));
        } else if is_server_log_name(&name) {
            extra = extra.saturating_add(meta.len());
        }
    }
    // The segment being written stays. Everything else, including today's
    // closed segments, counts toward the cap and can go, oldest first.
    let active = crate::logs::segments::current_target_name(dir).or_else(|| {
        days.iter()
            .filter(|(log, ..)| log.date == today && !log.compressed)
            .max_by_key(|(log, ..)| log.seg)
            .map(|(_, name, ..)| name.clone())
    });
    let pg_files = crate::logs::postgres_files::list(dir)?;
    let open = crate::logs::postgres_files::open_names(&pg_files, now);
    let mut deletable = Vec::new();
    let mut total = days
        .iter()
        .fold(extra, |sum, (.., len)| sum.saturating_add(*len));
    for (log, name, path, len) in days {
        if active.as_deref() == Some(name.as_str()) {
            continue;
        }
        deletable.push(Piece {
            key: log.date.midnight().assume_utc()
                + time::Duration::milliseconds(i64::from(log.seg)),
            path,
            len,
        });
    }
    for file in pg_files {
        total = total.saturating_add(file.len);
        if open.contains(&file.name) {
            continue;
        }
        deletable.push(Piece {
            key: file.opened.unwrap_or(file.modified),
            path: file.path,
            len: file.len,
        });
    }
    deletable.sort_by_key(|piece| piece.key);
    while total > cap {
        if deletable.is_empty() {
            tracing::warn!("{OVER_CAP_WARNING}");
            break;
        }
        let piece = deletable.remove(0);
        if remove_log(&piece.path) {
            total = total.saturating_sub(piece.len);
        } else {
            break;
        }
    }
    Ok(())
}

fn sweep_legacy(dir: &Path, retention_days: u32, now: OffsetDateTime) -> io::Result<()> {
    let meta = match fs::symlink_metadata(dir) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Ok(());
    }
    let mut old = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let Some(name) = entry_name(&entry) else {
            continue;
        };
        if name == "postgres.log" {
            continue;
        }
        if file_is_old(&meta, now, retention_days) {
            old.push(path);
        }
    }
    for path in old {
        remove_log(&path);
    }
    let remaining = match fs::read_dir(dir)?.next() {
        Some(entry) => {
            entry?;
            true
        }
        None => false,
    };
    if !remaining {
        if let Err(error) = fs::remove_dir(dir) {
            tracing::warn!(
                error = %error,
                path = %dir.display(),
                "failed to remove an empty logs directory"
            );
        }
    }
    Ok(())
}

fn read_entries(dir: &Path) -> io::Result<Option<fs::ReadDir>> {
    if is_symlink(dir) {
        return Ok(None);
    }
    match fs::read_dir(dir) {
        Ok(entries) => Ok(Some(entries)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn remove_log(path: &Path) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        Err(error) => {
            tracing::warn!(
                error = %error,
                path = %path.display(),
                "failed to delete an expired log"
            );
            false
        }
    }
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

    fn fixed_now() -> OffsetDateTime {
        today().with_hms(12, 0, 0).expect("time").assume_utc()
    }

    fn set_mtime(path: &Path, when: OffsetDateTime) {
        let file = fs::OpenOptions::new().write(true).open(path).expect("open");
        file.set_modified(std::time::SystemTime::from(when))
            .expect("mtime");
    }

    fn set_len(path: &Path, len: u64) {
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .expect("open");
        file.set_len(len).expect("len");
    }

    fn decode_zst(path: &Path) -> String {
        let file = fs::File::open(path).expect("open zst");
        let mut decoder = zstd::stream::read::Decoder::new(file).expect("decoder");
        let mut text = String::new();
        std::io::Read::read_to_string(&mut decoder, &mut text).expect("decode");
        text
    }

    fn info_line(message: &str) -> String {
        format!(
            r#"{{"timestamp":"2026-10-14T03:00:00Z","level":"INFO","target":"coppice_server::logs","fields":{{"message":"{message}","source":"server"}}}}"#
        )
    }

    #[test]
    fn the_size_cap_is_256_mib_and_not_a_setting() {
        assert_eq!(LOG_DIR_MAX_BYTES, 268_435_456);
        assert_eq!(
            OVER_CAP_WARNING,
            "Logs are over 256 MiB even after cleanup. Coppice keeps the log file and the database log it is writing."
        );
    }

    #[test]
    fn past_days_are_compressed_and_the_page_and_export_still_read_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        let line = info_line("still-readable");
        fs::write(
            dir.path().join("coppice.2026-10-14.jsonl"),
            format!("{line}\n"),
        )
        .expect("write");
        let today_body = format!("{}\n", info_line("today-stays"));
        fs::write(dir.path().join("coppice.2026-10-15.jsonl"), &today_body).expect("write");

        sweep_logs(dir.path(), 14, today(), fixed_now(), None).expect("sweep");

        assert!(!dir.path().join("coppice.2026-10-14.jsonl").exists());
        let zst = dir.path().join("coppice.2026-10-14.jsonl.zst");
        assert_eq!(decode_zst(&zst), format!("{line}\n"));
        assert!(!dir.path().join("coppice.2026-10-14.jsonl.zst.tmp").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("coppice.2026-10-15.jsonl")).expect("today"),
            today_body
        );

        let filter =
            crate::logs::read::filter_from_params(&crate::logs::read::FilterParams::default())
                .expect("filter");
        let page = crate::logs::read::query(
            dir.path(),
            &crate::logs::read::LogQuery {
                filter: filter.clone(),
                cursor: None,
                limit: 20,
            },
        )
        .expect("query");
        let messages: Vec<_> = page
            .lines
            .iter()
            .map(|line| line.message.as_str())
            .collect();
        assert!(messages.contains(&"still-readable"), "{messages:?}");
        let exported = crate::logs::read::export_lines(dir.path(), &filter).expect("export");
        assert!(exported.iter().any(|line| line.contains("still-readable")));
    }

    #[test]
    fn a_non_empty_compressed_day_is_kept_and_the_plain_file_is_removed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zst = dir.path().join("coppice.2026-10-14.jsonl.zst");
        let file = fs::File::create(&zst).expect("create");
        let mut encoder = zstd::stream::write::Encoder::new(file, zstd::DEFAULT_COMPRESSION_LEVEL)
            .expect("encoder");
        std::io::Write::write_all(&mut encoder, b"already-compressed\n").expect("write");
        encoder.finish().expect("finish");
        let kept = fs::read(&zst).expect("bytes");
        fs::write(dir.path().join("coppice.2026-10-14.jsonl"), b"plain\n").expect("plain");

        sweep_logs(dir.path(), 14, today(), fixed_now(), None).expect("sweep");

        assert!(!dir.path().join("coppice.2026-10-14.jsonl").exists());
        assert_eq!(fs::read(&zst).expect("zst"), kept);
    }

    #[test]
    fn an_empty_compressed_day_is_replaced_from_the_plain_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zst = dir.path().join("coppice.2026-10-14.jsonl.zst");
        fs::write(&zst, b"").expect("empty");
        fs::write(dir.path().join("coppice.2026-10-14.jsonl"), b"from-plain\n").expect("plain");

        sweep_logs(dir.path(), 14, today(), fixed_now(), None).expect("sweep");

        assert!(!dir.path().join("coppice.2026-10-14.jsonl").exists());
        assert_eq!(decode_zst(&zst), "from-plain\n");
    }

    #[test]
    fn a_symlink_day_is_not_compressed_or_followed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("target.txt");
        fs::write(&target, b"do-not-touch").expect("target");
        let link = dir.path().join("coppice.2026-10-01.jsonl");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        sweep_logs(dir.path(), 14, today(), fixed_now(), None).expect("sweep");

        assert!(link
            .symlink_metadata()
            .expect("link")
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(&target).expect("target"), b"do-not-touch");
        assert!(!dir.path().join("coppice.2026-10-01.jsonl.zst").exists());
    }

    #[test]
    fn the_size_cap_deletes_the_oldest_day_and_never_today_or_postgres() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mib = 1024 * 1024;
        set_len(&dir.path().join("coppice.2026-10-15.jsonl"), 200 * mib);
        set_len(&dir.path().join("coppice.2026-10-14.jsonl.zst"), 100 * mib);
        set_len(&dir.path().join("coppice.2026-10-13.jsonl.zst"), 100 * mib);
        let pg = dir.path().join("postgres.log");
        set_len(&pg, 400 * mib);
        set_mtime(&pg, fixed_now());
        let blob = dir.path().join("blob");
        set_len(&blob, 400 * mib);
        std::os::unix::fs::symlink(&blob, dir.path().join("coppice.2020-01-01.jsonl"))
            .expect("symlink");

        sweep_logs(dir.path(), 14, today(), fixed_now(), None).expect("sweep");

        assert!(dir.path().join("coppice.2026-10-15.jsonl").is_file());
        assert!(!dir.path().join("coppice.2026-10-14.jsonl.zst").exists());
        assert!(!dir.path().join("coppice.2026-10-13.jsonl.zst").exists());
        assert_eq!(
            fs::metadata(dir.path().join("postgres.log"))
                .expect("postgres")
                .len(),
            400 * mib
        );
        assert!(dir
            .path()
            .join("coppice.2020-01-01.jsonl")
            .symlink_metadata()
            .expect("old symlink")
            .file_type()
            .is_symlink());
        assert_eq!(fs::metadata(&blob).expect("blob").len(), 400 * mib);
    }

    #[test]
    fn a_fresh_server_log_counts_toward_the_cap_and_today_stays_when_still_over() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mib = 1024 * 1024;
        set_len(&dir.path().join("coppice.2026-10-15.jsonl"), 200 * mib);
        set_len(&dir.path().join("server.log"), 100 * mib);
        set_mtime(&dir.path().join("server.log"), fixed_now());
        set_len(&dir.path().join("coppice.2026-10-14.jsonl.zst"), 100 * mib);

        let warnings = warnings_during(|| {
            sweep_logs(dir.path(), 14, today(), fixed_now(), None).expect("sweep");
        });

        assert!(dir.path().join("coppice.2026-10-15.jsonl").is_file());
        assert!(dir.path().join("server.log").is_file());
        assert!(!dir.path().join("coppice.2026-10-14.jsonl.zst").exists());
        assert!(
            warnings.iter().any(|line| line.contains(OVER_CAP_WARNING)),
            "{warnings:?}"
        );
    }

    #[test]
    fn old_server_logs_are_deleted_and_not_copied_into_jsonl() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = b"not-imported-server-log-bytes";
        let old = fixed_now() - time::Duration::days(14);
        let fresh = fixed_now() - time::Duration::days(14) + time::Duration::seconds(1);
        for name in ["server.log", "server.log.2", "server.log.3"] {
            let path = dir.path().join(name);
            fs::write(&path, marker).expect("write");
            set_mtime(&path, old);
        }
        let kept = dir.path().join("server.log.1");
        fs::write(&kept, b"still-fresh").expect("write");
        set_mtime(&kept, fresh);
        let extra = dir.path().join("server.log.4");
        fs::write(&extra, marker).expect("write");
        set_mtime(&extra, old);
        fs::write(dir.path().join("coppice.2026-10-14.jsonl"), b"daily\n").expect("daily");
        let pg = dir.path().join("postgres.log");
        fs::write(&pg, marker).expect("postgres");
        set_mtime(&pg, old);
        std::os::unix::fs::symlink("postgres.log", dir.path().join("server.log.linked"))
            .expect("symlink");

        sweep_logs(dir.path(), 14, today(), fixed_now(), None).expect("sweep");

        assert!(!dir.path().join("server.log").exists());
        assert!(!dir.path().join("server.log.2").exists());
        assert!(!dir.path().join("server.log.3").exists());
        assert_eq!(fs::read(&kept).expect("fresh"), b"still-fresh");
        assert_eq!(fs::read(&extra).expect("extra"), marker);
        assert!(
            !dir.path().join("postgres.log").exists(),
            "an old postgres.log is past retention when nothing is writing it"
        );
        assert!(dir
            .path()
            .join("server.log.linked")
            .symlink_metadata()
            .expect("link")
            .file_type()
            .is_symlink());
        let stored = fs::read(dir.path().join("coppice.2026-10-14.jsonl.zst")).unwrap_or_default();
        assert!(!stored.windows(marker.len()).any(|window| window == marker));
    }

    #[test]
    fn old_config_folder_logs_are_deleted_and_postgres_is_kept() {
        let root = tempfile::tempdir().expect("tempdir");
        let live = root.path().join("live");
        fs::create_dir(&live).expect("live");
        let parent = root.path().join("Coppice");
        let logs = parent.join("logs");
        fs::create_dir_all(&logs).expect("logs");
        let old = logs.join("server.log");
        fs::write(&old, b"old-config-log").expect("old");
        set_mtime(&old, fixed_now() - time::Duration::days(14));
        let notes = logs.join("notes.txt");
        fs::write(&notes, b"keep").expect("notes");
        set_mtime(&notes, fixed_now());
        let pg = logs.join("postgres.log");
        fs::write(&pg, b"pg").expect("pg");
        set_mtime(&pg, fixed_now() - time::Duration::days(40));
        let outside = root.path().join("outside.txt");
        fs::write(&outside, b"outside").expect("outside");
        std::os::unix::fs::symlink(&outside, logs.join("link.txt")).expect("symlink");

        sweep_logs(&live, 14, today(), fixed_now(), Some(&logs)).expect("sweep");

        assert!(!old.exists());
        assert_eq!(fs::read(&notes).expect("notes"), b"keep");
        assert_eq!(fs::read(&pg).expect("pg"), b"pg");
        assert!(logs
            .join("link.txt")
            .symlink_metadata()
            .expect("link")
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(&outside).expect("outside"), b"outside");
        assert!(logs.is_dir());
        assert!(parent.is_dir());
    }

    #[test]
    fn an_empty_old_config_logs_directory_is_removed_and_the_parent_stays() {
        let root = tempfile::tempdir().expect("tempdir");
        let live = root.path().join("live");
        fs::create_dir(&live).expect("live");
        let parent = root.path().join("Coppice");
        let logs = parent.join("logs");
        fs::create_dir_all(&logs).expect("logs");
        let old = logs.join("server.log.1");
        fs::write(&old, b"x").expect("old");
        set_mtime(&old, fixed_now() - time::Duration::days(30));

        sweep_logs(&live, 14, today(), fixed_now(), Some(&logs)).expect("sweep");

        assert!(!logs.exists());
        assert!(parent.is_dir());
    }

    #[test]
    fn the_old_config_pass_does_not_apply_to_the_live_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let notes = dir.path().join("notes.txt");
        fs::write(&notes, b"stay").expect("notes");
        set_mtime(&notes, fixed_now() - time::Duration::days(40));
        let pg = dir.path().join("postgres.log");
        fs::write(&pg, b"pg").expect("pg");
        set_mtime(&pg, fixed_now() - time::Duration::days(40));

        sweep_logs(dir.path(), 14, today(), fixed_now(), Some(dir.path())).expect("sweep");

        assert_eq!(fs::read(&notes).expect("notes"), b"stay");
        assert!(
            !pg.exists(),
            "an old database log in the live directory is past retention"
        );
    }

    #[test]
    fn rotated_postgres_logs_expire_and_the_open_file_stays() {
        let dir = tempfile::tempdir().expect("tempdir");
        let expired = dir.path().join("postgresql-2026-10-01_000000.log");
        fs::write(&expired, b"old-rotated").expect("write");
        set_mtime(&expired, fixed_now() - time::Duration::days(20));
        let kept = dir.path().join("postgresql-2026-10-02_000000.log");
        fs::write(&kept, b"kept-rotated").expect("write");
        set_mtime(&kept, fixed_now() - time::Duration::days(13));
        // Filename date is past retention, but Postgres is still writing it.
        let open = dir.path().join("postgresql-2026-10-01_000001.log");
        fs::write(&open, b"still-writing").expect("write");
        set_mtime(&open, fixed_now());
        let legacy = dir.path().join("postgres.log");
        fs::write(&legacy, b"startup").expect("write");
        set_mtime(&legacy, fixed_now() - time::Duration::days(20));
        std::os::unix::fs::symlink(&kept, dir.path().join("postgresql-2020-01-01_000000.log"))
            .expect("symlink");

        sweep_logs(dir.path(), 14, today(), fixed_now(), None).expect("sweep");

        assert!(!expired.exists());
        assert!(!legacy.exists());
        assert_eq!(fs::read(&kept).expect("kept"), b"kept-rotated");
        assert_eq!(fs::read(&open).expect("open"), b"still-writing");
        assert!(dir
            .path()
            .join("postgresql-2020-01-01_000000.log")
            .symlink_metadata()
            .expect("link")
            .file_type()
            .is_symlink());
    }

    #[test]
    fn postgres_logs_count_toward_the_cap_and_the_open_file_is_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mib = 1024 * 1024;
        set_len(&dir.path().join("coppice.2026-10-15.jsonl"), 200 * mib);
        set_len(&dir.path().join("coppice.2026-10-13.jsonl.zst"), 50 * mib);
        let legacy = dir.path().join("postgres.log");
        set_len(&legacy, 100 * mib);
        set_mtime(&legacy, fixed_now() - time::Duration::hours(1));
        let open = dir.path().join("postgresql-2026-10-15_120000.log");
        set_len(&open, 100 * mib);
        set_mtime(&open, fixed_now());
        let old = dir.path().join("postgresql-2026-10-10_000000.log");
        set_len(&old, 50 * mib);
        set_mtime(&old, fixed_now() - time::Duration::days(5));

        let warnings = warnings_during(|| {
            sweep_logs(dir.path(), 14, today(), fixed_now(), None).expect("sweep");
        });

        assert!(dir.path().join("coppice.2026-10-15.jsonl").is_file());
        assert!(!dir.path().join("coppice.2026-10-13.jsonl.zst").exists());
        assert!(!legacy.exists(), "a closed database log counts and can go");
        assert!(!old.exists());
        assert_eq!(fs::metadata(&open).expect("open").len(), 100 * mib);
        assert!(
            warnings.iter().any(|line| line.contains(OVER_CAP_WARNING)),
            "{warnings:?}"
        );
    }

    #[test]
    fn a_symlinked_old_config_logs_directory_is_not_followed() {
        let root = tempfile::tempdir().expect("tempdir");
        let live = root.path().join("live");
        fs::create_dir(&live).expect("live");
        let real = root.path().join("real");
        fs::create_dir(&real).expect("real");
        let victim = real.join("server.log");
        fs::write(&victim, b"nope").expect("victim");
        set_mtime(&victim, fixed_now() - time::Duration::days(40));
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");

        sweep_logs(&live, 14, today(), fixed_now(), Some(&link)).expect("sweep");

        assert_eq!(fs::read(&victim).expect("victim"), b"nope");
        assert!(link
            .symlink_metadata()
            .expect("link")
            .file_type()
            .is_symlink());
    }

    #[test]
    fn legacy_logs_path_uses_the_old_config_folder() {
        let home = Path::new("/home/ada");
        assert_eq!(
            legacy_logs_path(home, "linux").expect("linux"),
            std::path::PathBuf::from("/home/ada/.config/Coppice/logs")
        );
        assert_eq!(
            legacy_logs_path(home, "macos").expect("macos"),
            std::path::PathBuf::from("/home/ada/Library/Application Support/Coppice/logs")
        );
        assert!(legacy_logs_path(home, "windows").is_none());
    }

    fn warnings_during(f: impl FnOnce()) -> Vec<String> {
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::layer::{Layer, SubscriberExt};

        struct Capture(Arc<Mutex<Vec<String>>>);
        struct Visit(String);

        impl tracing::field::Visit for Visit {
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "message" {
                    self.0 = value.to_string();
                }
            }

            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" && self.0.is_empty() {
                    self.0 = format!("{value:?}");
                }
            }
        }

        impl<S: tracing::Subscriber> Layer<S> for Capture {
            fn on_event(
                &self,
                event: &tracing::Event<'_>,
                _ctx: tracing_subscriber::layer::Context<'_, S>,
            ) {
                if *event.metadata().level() > tracing::Level::WARN {
                    return;
                }
                let mut visit = Visit(String::new());
                event.record(&mut visit);
                if !visit.0.is_empty() {
                    self.0.lock().expect("lock").push(visit.0);
                }
            }
        }

        let lines = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::registry().with(Capture(Arc::clone(&lines)));
        tracing::subscriber::with_default(subscriber, f);
        let recorded = lines.lock().expect("lock").clone();
        recorded
    }

    #[test]
    fn todays_closed_segments_count_toward_the_cap() {
        let dir = tempfile::tempdir().expect("tempdir");
        let appender =
            crate::logs::segments::SegmentedAppender::with_max_bytes(dir.path(), 64 * 1024)
                .expect("appender");
        let line = format!("{}\n", info_line("x".repeat(40).as_str()));
        for _ in 0..300_000 {
            appender.write_record(line.as_bytes()).expect("write");
        }
        let cap = 1024 * 1024;
        let now = OffsetDateTime::now_utc();
        enforce_size_cap_at(dir.path(), now.date(), now, cap).expect("cap");
        let total: u64 = fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
            .map(|entry| entry.metadata().expect("meta").len())
            .sum();
        assert!(total <= cap, "logs are {total} bytes after the sweep");
        let active = crate::logs::segments::current_target_name(dir.path()).expect("current");
        assert!(dir.path().join(&active).is_file(), "the active segment stays");
        appender.write_record(line.as_bytes()).expect("still writable");
    }
}
