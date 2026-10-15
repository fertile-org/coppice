//! Read daily JSONL logs and `postgres.log` for the Logs page.
//! No index. Newest matching lines come first.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Deserialize;
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime};

#[derive(Debug)]
pub enum QueryError {
    Io(io::Error),
    BadCursor,
    BadFilter,
}

impl From<io::Error> for QueryError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Server,
    Http,
    Cli,
    Postgres,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Http => "http",
            Self::Cli => "cli",
            Self::Postgres => "postgres",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "server" => Some(Self::Server),
            "http" => Some(Self::Http),
            "cli" => Some(Self::Cli),
            "postgres" => Some(Self::Postgres),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogFilter {
    pub source: Option<Source>,
    /// Minimum tracing rank. 0 trace … 4 error. Default info is 2.
    pub min_level: u8,
    pub from: Option<OffsetDateTime>,
    pub to: Option<OffsetDateTime>,
    pub run_id: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FilterParams {
    pub source: Option<String>,
    pub level: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub run: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LogQuery {
    pub filter: LogFilter,
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub timestamp: Option<String>,
    pub level: String,
    pub source: String,
    pub target: Option<String>,
    pub message: String,
    pub run_id: Option<String>,
    pub ticket_id: Option<String>,
    pub stream: Option<String>,
    pub fields: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogPage {
    pub lines: Vec<LogLine>,
    pub next_cursor: Option<String>,
}

pub fn filter_from_params(params: &FilterParams) -> Result<LogFilter, QueryError> {
    let source = match blank(params.source.as_deref()) {
        None => None,
        Some(value) => Some(Source::parse(value).ok_or(QueryError::BadFilter)?),
    };
    let min_level = match blank(params.level.as_deref()) {
        None => level_rank("info").expect("info"),
        Some(value) => level_rank(value).ok_or(QueryError::BadFilter)?,
    };
    let from = parse_bound(params.from.as_deref())?;
    let to = parse_bound(params.to.as_deref())?;
    Ok(LogFilter {
        source,
        min_level,
        from,
        to,
        run_id: blank(params.run.as_deref()).map(str::to_string),
        text: blank(params.q.as_deref()).map(str::to_string),
    })
}

pub fn query(dir: &Path, query: &LogQuery) -> Result<LogPage, QueryError> {
    if !dir.exists() {
        return Ok(LogPage {
            lines: Vec::new(),
            next_cursor: None,
        });
    }
    let mut dated = Vec::new();
    let mut undated = Vec::new();
    if query.filter.source != Some(Source::Postgres) {
        dated.extend(read_jsonl_days(dir, &query.filter)?);
    }
    if query.filter.source.is_none() || query.filter.source == Some(Source::Postgres) {
        let (postgres_dated, postgres_undated) = read_postgres(dir, &query.filter)?;
        dated.extend(postgres_dated);
        undated.extend(postgres_undated);
    }
    let ordered = merge(dated, undated);

    let start = match &query.cursor {
        None => 0,
        Some(cursor) => {
            let (file_key, line_index) = decode_cursor(cursor).ok_or(QueryError::BadCursor)?;
            let position = ordered
                .iter()
                .position(|item| item.file_key == file_key && item.line_index == line_index)
                .ok_or(QueryError::BadCursor)?;
            position + 1
        }
    };
    let end = start.saturating_add(query.limit).min(ordered.len());
    let page = if start >= ordered.len() {
        &[]
    } else {
        &ordered[start..end]
    };
    let next_cursor = if end < ordered.len() {
        page.last()
            .map(|item| encode_cursor(&item.file_key, item.line_index))
    } else {
        None
    };
    Ok(LogPage {
        lines: page.iter().map(|item| item.line.clone()).collect(),
        next_cursor,
    })
}

/// One JSONL line, when it matches. Used by the live tail.
pub fn matching_jsonl_line(filter: &LogFilter, raw: &str) -> Option<LogLine> {
    if filter.source == Some(Source::Postgres) {
        return None;
    }
    let (line, timestamp) = parse_jsonl(raw)?;
    if !accepts(filter, &line, timestamp, raw) {
        return None;
    }
    Some(line)
}

struct Located {
    timestamp: Option<OffsetDateTime>,
    postgres: bool,
    file_key: String,
    line_index: u64,
    line: LogLine,
}

fn merge(mut dated: Vec<Located>, mut undated: Vec<Located>) -> Vec<Located> {
    dated.sort_by(|left, right| {
        right
            .timestamp
            .cmp(&left.timestamp)
            .then_with(|| right.line_index.cmp(&left.line_index))
            .then_with(|| right.file_key.cmp(&left.file_key))
    });
    if undated.is_empty() {
        return dated;
    }
    // Lines with no timestamp stay together after the dated lines from postgres.log.
    undated.sort_by_key(|left| left.line_index);
    let insert_at = dated
        .iter()
        .rposition(|item| item.postgres)
        .map(|index| index + 1)
        .unwrap_or(dated.len());
    dated.splice(insert_at..insert_at, undated);
    dated
}

fn read_jsonl_days(dir: &Path, filter: &LogFilter) -> Result<Vec<Located>, QueryError> {
    let mut days = day_files(dir)?;
    days.sort_by_key(|left| std::cmp::Reverse(left.date));
    let mut located = Vec::new();
    for day in days {
        if !day_overlaps(day.date, filter) {
            continue;
        }
        let text = read_lossy(&day.path, day.compressed)?;
        let file_key = day.date.to_string();
        for (index, raw) in text.lines().enumerate() {
            if raw.trim().is_empty() {
                continue;
            }
            let Some((line, timestamp)) = parse_jsonl(raw) else {
                continue;
            };
            if !accepts(filter, &line, timestamp, raw) {
                continue;
            }
            located.push(Located {
                timestamp,
                postgres: false,
                file_key: file_key.clone(),
                line_index: u64::try_from(index).unwrap_or(u64::MAX),
                line,
            });
        }
    }
    Ok(located)
}

struct DayFile {
    date: Date,
    path: PathBuf,
    compressed: bool,
}

fn day_files(dir: &Path) -> Result<Vec<DayFile>, QueryError> {
    let mut by_date: Vec<DayFile> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some((date, compressed)) = day_name(&name) else {
            continue;
        };
        if let Some(existing) = by_date.iter_mut().find(|day| day.date == date) {
            if existing.compressed && !compressed {
                existing.path = entry.path();
                existing.compressed = false;
            }
            continue;
        }
        by_date.push(DayFile {
            date,
            path: entry.path(),
            compressed,
        });
    }
    Ok(by_date)
}

fn day_name(name: &str) -> Option<(Date, bool)> {
    let rest = name.strip_prefix("coppice.")?;
    if let Some(date) = rest.strip_suffix(".jsonl.zst") {
        return Some((parse_date(date)?, true));
    }
    if let Some(date) = rest.strip_suffix(".jsonl") {
        return Some((parse_date(date)?, false));
    }
    None
}

fn parse_date(value: &str) -> Option<Date> {
    let mut parts = value.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()
}

fn day_overlaps(date: Date, filter: &LogFilter) -> bool {
    if let Some(from) = filter.from {
        if date < utc_date(from) {
            return false;
        }
    }
    if let Some(to) = filter.to {
        if date > utc_date(to) {
            return false;
        }
    }
    true
}

fn utc_date(timestamp: OffsetDateTime) -> Date {
    timestamp.to_offset(time::UtcOffset::UTC).date()
}

fn read_postgres(
    dir: &Path,
    filter: &LogFilter,
) -> Result<(Vec<Located>, Vec<Located>), QueryError> {
    let path = dir.join("postgres.log");
    if !path.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    let text = read_lossy(&path, false)?;
    let mut dated = Vec::new();
    let mut undated = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let timestamp = leading_timestamp(raw);
        let line = postgres_line(raw, timestamp);
        if !accepts(filter, &line, timestamp, raw) {
            continue;
        }
        let located = Located {
            timestamp,
            postgres: true,
            file_key: "postgres".to_string(),
            line_index: u64::try_from(index).unwrap_or(u64::MAX),
            line,
        };
        if timestamp.is_some() {
            dated.push(located);
        } else {
            undated.push(located);
        }
    }
    Ok((dated, undated))
}

fn postgres_line(raw: &str, timestamp: Option<OffsetDateTime>) -> LogLine {
    LogLine {
        timestamp: timestamp
            .map(|value| value.format(&Rfc3339).unwrap_or_else(|_| raw.to_string())),
        level: postgres_level(raw).to_string(),
        source: Source::Postgres.as_str().to_string(),
        target: None,
        message: raw.to_string(),
        run_id: None,
        ticket_id: None,
        stream: None,
        fields: serde_json::Map::new(),
    }
}

fn postgres_level(line: &str) -> &'static str {
    if line.contains("PANIC:") || line.contains("FATAL:") || line.contains("ERROR:") {
        "ERROR"
    } else if line.contains("WARNING:") {
        "WARN"
    } else if line.contains("DEBUG:") {
        "DEBUG"
    } else {
        "INFO"
    }
}

fn leading_timestamp(line: &str) -> Option<OffsetDateTime> {
    let bytes = line.as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if bytes[10] != b'T' && bytes[10] != b' ' {
        return None;
    }
    let mut end = 19;
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    let date = &line[0..10];
    let clock = &line[11..19];
    let fraction = &line[19..end];
    let rest = line[end..].trim_start();
    let zone = offset_suffix(rest);
    let formatted = format!("{date}T{clock}{fraction}{zone}");
    OffsetDateTime::parse(&formatted, &Rfc3339).ok()
}

fn offset_suffix(rest: &str) -> String {
    let token = rest.split_whitespace().next().unwrap_or("");
    if token == "UTC" || token == "GMT" || token == "Z" || token.is_empty() {
        return "Z".to_string();
    }
    if token.starts_with('+') || token.starts_with('-') {
        return normalize_numeric_offset(token);
    }
    "Z".to_string()
}

fn normalize_numeric_offset(token: &str) -> String {
    if token.len() == 6 && token.as_bytes().get(3) == Some(&b':') {
        return token.to_string();
    }
    if token.len() == 5 {
        return format!("{}:{}", &token[..3], &token[3..5]);
    }
    if token.len() == 3 {
        return format!("{token}:00");
    }
    "Z".to_string()
}

fn parse_jsonl(raw: &str) -> Option<(LogLine, Option<OffsetDateTime>)> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let timestamp_text = value
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_string);
    let timestamp = timestamp_text
        .as_deref()
        .and_then(|text| OffsetDateTime::parse(text, &Rfc3339).ok());
    let mut fields = value.get("fields")?.as_object()?.clone();
    let source = fields
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("server")
        .to_string();
    Source::parse(&source)?;
    let message = fields
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let run_id = string_field(&fields, "run_id");
    let ticket_id = string_field(&fields, "ticket_id");
    let stream = string_field(&fields, "stream");
    for key in ["message", "source", "run_id", "ticket_id", "stream"] {
        fields.remove(key);
    }
    let level = value
        .get("level")
        .and_then(Value::as_str)
        .unwrap_or("INFO")
        .to_string();
    let target = value
        .get("target")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some((
        LogLine {
            timestamp: timestamp_text,
            level,
            source,
            target,
            message,
            run_id,
            ticket_id,
            stream,
            fields,
        },
        timestamp,
    ))
}

fn string_field(fields: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    fields.get(key).and_then(Value::as_str).map(str::to_string)
}

fn accepts(
    filter: &LogFilter,
    line: &LogLine,
    timestamp: Option<OffsetDateTime>,
    raw: &str,
) -> bool {
    if let Some(source) = filter.source {
        if line.source != source.as_str() {
            return false;
        }
    }
    let rank = level_rank(&line.level).unwrap_or(2);
    if rank < filter.min_level {
        return false;
    }
    if filter.from.is_some() || filter.to.is_some() {
        let Some(timestamp) = timestamp else {
            return false;
        };
        if let Some(from) = filter.from {
            if timestamp < from {
                return false;
            }
        }
        if let Some(to) = filter.to {
            if timestamp > to {
                return false;
            }
        }
    }
    if let Some(run_id) = &filter.run_id {
        if line.run_id.as_deref() != Some(run_id.as_str()) {
            return false;
        }
    }
    if let Some(needle) = &filter.text {
        let message_hit = contains_ci(&line.message, needle);
        let raw_hit = contains_ci(raw, needle);
        if !message_hit && !raw_hit {
            return false;
        }
    }
    true
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn level_rank(level: &str) -> Option<u8> {
    match level.to_ascii_lowercase().as_str() {
        "trace" => Some(0),
        "debug" => Some(1),
        "info" => Some(2),
        "warn" | "warning" => Some(3),
        "error" => Some(4),
        _ => None,
    }
}

fn blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn parse_bound(value: Option<&str>) -> Result<Option<OffsetDateTime>, QueryError> {
    let Some(value) = blank(value) else {
        return Ok(None);
    };
    OffsetDateTime::parse(value, &Rfc3339)
        .map(Some)
        .map_err(|_| QueryError::BadFilter)
}

fn read_lossy(path: &Path, compressed: bool) -> io::Result<String> {
    let bytes = if compressed {
        let file = File::open(path)?;
        let mut decoder = zstd::stream::read::Decoder::new(file)?;
        let mut bytes = Vec::new();
        decoder.read_to_end(&mut bytes)?;
        bytes
    } else {
        fs::read(path)?
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn encode_cursor(file_key: &str, line_index: u64) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{file_key}:{line_index}"))
}

fn decode_cursor(cursor: &str) -> Option<(String, u64)> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let (file_key, index) = text.rsplit_once(':')?;
    if file_key.is_empty() {
        return None;
    }
    Some((file_key.to_string(), index.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(
        source: Option<&str>,
        level: Option<&str>,
        run: Option<&str>,
        q: Option<&str>,
    ) -> LogQuery {
        let filter = filter_from_params(&FilterParams {
            source: source.map(str::to_string),
            level: level.map(str::to_string),
            from: None,
            to: None,
            run: run.map(str::to_string),
            q: q.map(str::to_string),
        })
        .expect("filter");
        LogQuery {
            filter,
            cursor: None,
            limit: 200,
        }
    }

    fn jsonl(timestamp: &str, level: &str, source: &str, message: &str, extra: &str) -> String {
        format!(
            r#"{{"timestamp":"{timestamp}","level":"{level}","target":"coppice_server::logs","fields":{{"message":"{message}","source":"{source}"{extra}}}}}"#
        )
    }

    fn write_zst(path: &Path, text: &str) {
        let file = File::create(path).expect("create");
        let mut encoder = zstd::stream::write::Encoder::new(file, 3).expect("encoder");
        use std::io::Write;
        encoder.write_all(text.as_bytes()).expect("write");
        encoder.finish().expect("finish");
    }

    #[test]
    fn filter_returns_a_compressed_day_and_the_next_page_skips_the_boundary() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run = "run-77";
        let compressed = format!(
            "{}\n{}\n{}\n",
            jsonl(
                "2026-10-13T01:00:00Z",
                "DEBUG",
                "cli",
                "hidden",
                &format!(r#","run_id":"{run}","stream":"stdout""#)
            ),
            jsonl(
                "2026-10-13T02:00:00Z",
                "INFO",
                "cli",
                "needle-from-zst",
                &format!(r#","run_id":"{run}","stream":"stderr""#)
            ),
            jsonl("2026-10-13T03:00:00Z", "INFO", "server", "other", ""),
        );
        write_zst(
            &dir.path().join("coppice.2026-10-13.jsonl.zst"),
            &compressed,
        );
        fs::write(
            dir.path().join("coppice.2026-10-14.jsonl"),
            format!(
                "{}\n{}\n{}\n",
                jsonl("2026-10-14T01:00:00Z", "INFO", "server", "oldest", ""),
                jsonl("2026-10-14T02:00:00Z", "INFO", "server", "middle", ""),
                jsonl("2026-10-14T03:00:00Z", "INFO", "server", "newest", ""),
            ),
        )
        .expect("write");
        std::os::unix::fs::symlink("coppice.2026-10-14.jsonl", dir.path().join("current.jsonl"))
            .expect("symlink");

        let mut filtered = params(Some("cli"), None, Some(run), Some("NEEDLE"));
        let page = query(dir.path(), &filtered).expect("query");
        assert_eq!(page.lines.len(), 1);
        assert_eq!(page.lines[0].message, "needle-from-zst");
        assert_eq!(page.lines[0].source, "cli");
        assert_eq!(page.lines[0].run_id.as_deref(), Some(run));
        assert_eq!(page.lines[0].stream.as_deref(), Some("stderr"));
        assert!(page.next_cursor.is_none());

        filtered = params(None, None, None, None);
        filtered.limit = 2;
        let first = query(dir.path(), &filtered).expect("first");
        assert_eq!(
            first
                .lines
                .iter()
                .map(|line| line.message.as_str())
                .collect::<Vec<_>>(),
            vec!["newest", "middle"]
        );
        let cursor = first.next_cursor.expect("cursor");
        filtered.cursor = Some(cursor);
        let second = query(dir.path(), &filtered).expect("second");
        let messages: Vec<_> = second
            .lines
            .iter()
            .map(|line| line.message.as_str())
            .collect();
        assert!(!messages.contains(&"middle"), "{messages:?}");
        assert_eq!(messages.first().copied(), Some("oldest"));
    }

    #[test]
    fn postgres_lines_are_their_own_source_and_undated_lines_follow_dated_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("coppice.2026-10-14.jsonl"),
            format!(
                "{}\n{}\n",
                jsonl("2026-10-14T04:00:00Z", "INFO", "server", "newer", ""),
                jsonl(
                    "2026-10-14T01:00:00Z",
                    "INFO",
                    "cli",
                    "older",
                    r#","run_id":"run-1","stream":"stdout""#
                ),
            ),
        )
        .expect("write");
        fs::write(
            dir.path().join("postgres.log"),
            "2026-10-14 03:35:01.123 UTC [1] LOG:  database system is ready\nno stamp line\n2026-10-14 03:36:00 UTC [1] ERROR:  boom\n",
        )
        .expect("postgres");

        let only_db = query(dir.path(), &params(Some("postgres"), None, None, None)).expect("db");
        assert!(only_db.lines.iter().all(|line| line.source == "postgres"));
        assert!(only_db
            .lines
            .iter()
            .any(|line| line.message.contains("boom")));
        assert!(only_db.lines.iter().any(|line| line
            .timestamp
            .as_deref()
            .is_some_and(|ts| ts.starts_with("2026-10-14T03:35:01"))));

        let only_cli = query(dir.path(), &params(Some("cli"), None, None, None)).expect("cli");
        assert_eq!(only_cli.lines.len(), 1);
        assert_eq!(only_cli.lines[0].message, "older");

        let merged = query(dir.path(), &params(None, None, None, None)).expect("merged");
        let messages: Vec<_> = merged
            .lines
            .iter()
            .map(|line| line.message.as_str())
            .collect();
        let newer = messages
            .iter()
            .position(|message| *message == "newer")
            .unwrap();
        let dated = messages
            .iter()
            .position(|message| message.contains("database system is ready"))
            .unwrap();
        let undated = messages
            .iter()
            .position(|message| *message == "no stamp line")
            .unwrap();
        let older = messages
            .iter()
            .position(|message| *message == "older")
            .unwrap();
        assert!(newer < dated, "{messages:?}");
        assert!(dated < undated, "{messages:?}");
        assert!(undated < older, "{messages:?}");
    }

    #[test]
    fn text_matches_the_message_or_the_raw_line_and_debug_is_hidden_by_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("coppice.2026-10-14.jsonl"),
            format!(
                "{}\nnot json\n{}\n",
                jsonl("2026-10-14T01:00:00Z", "DEBUG", "server", "debug-only", ""),
                jsonl(
                    "2026-10-14T02:00:00Z",
                    "INFO",
                    "http",
                    "http request",
                    r#","method":"GET","path":"/api/health","status":200,"duration_ms":4"#
                ),
            ),
        )
        .expect("write");

        let page = query(dir.path(), &params(None, None, None, Some("HeAlTh"))).expect("query");
        assert_eq!(page.lines.len(), 1);
        assert_eq!(page.lines[0].source, "http");
        assert_eq!(
            page.lines[0].fields.get("path").and_then(Value::as_str),
            Some("/api/health")
        );
        assert!(page.lines[0].fields.get("message").is_none());

        let all = query(dir.path(), &params(None, None, None, None)).expect("all");
        assert!(all.lines.iter().all(|line| line.message != "debug-only"));
        assert!(all.lines.iter().all(|line| line.message != "not json"));
    }

    #[test]
    fn a_missing_source_is_read_as_server_and_bounds_are_inclusive() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("coppice.2026-10-14.jsonl"),
            "{\"timestamp\":\"2026-10-14T03:00:00Z\",\"level\":\"INFO\",\"target\":\"t\",\"fields\":{\"message\":\"untagged\"}}\n",
        )
        .expect("write");
        let page = query(dir.path(), &params(Some("server"), None, None, None)).expect("query");
        assert_eq!(page.lines[0].source, "server");
        assert_eq!(page.lines[0].message, "untagged");

        let mut bounded = params(None, None, None, None);
        bounded.filter.from =
            Some(OffsetDateTime::parse("2026-10-14T03:00:00Z", &Rfc3339).unwrap());
        bounded.filter.to = Some(OffsetDateTime::parse("2026-10-14T03:00:00Z", &Rfc3339).unwrap());
        let page = query(dir.path(), &bounded).expect("bounded");
        assert_eq!(page.lines.len(), 1);
    }
}
