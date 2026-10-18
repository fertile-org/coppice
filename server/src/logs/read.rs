//! Read daily JSONL logs and Postgres logs for the Logs page.
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
    /// The configured log directory is empty, missing, or not a directory.
    StoreMissing,
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
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub run: Option<String>,
    #[serde(default)]
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

/// One page, newest first. JSONL files are read backwards from the cursor,
/// so a page never loads a whole day.
pub fn query(dir: &Path, query: &LogQuery) -> Result<LogPage, QueryError> {
    if log_store_missing(dir) {
        return Err(QueryError::StoreMissing);
    }
    let cursor = match &query.cursor {
        None => Cursor::default(),
        Some(raw) => decode_cursor(raw).ok_or(QueryError::BadCursor)?,
    };
    let keep = query.limit.saturating_add(1);
    let mut merged = Merged::open(dir, &query.filter, &cursor, keep)?;
    let mut lines = Vec::new();
    while lines.len() < query.limit {
        let Some(item) = merged.next()? else {
            break;
        };
        lines.push(item.line);
    }
    let next_cursor = if merged.has_more()? {
        Some(encode_cursor(&merged.position()))
    } else {
        None
    };
    Ok(LogPage { lines, next_cursor })
}

/// Every matching line, newest first, as stored. Not one page.
pub fn export_lines(dir: &Path, filter: &LogFilter) -> Result<Vec<String>, QueryError> {
    if log_store_missing(dir) {
        return Err(QueryError::StoreMissing);
    }
    let mut merged = Merged::open(dir, filter, &Cursor::default(), usize::MAX)?;
    let mut out = Vec::new();
    while let Some(item) = merged.next()? {
        out.push(item.raw);
    }
    Ok(out)
}

/// Where the previous page stopped: the last JSONL line it returned (file
/// key and byte offset) and the last Postgres line (file and line index).
#[derive(Debug, Default, Clone, PartialEq)]
struct Cursor {
    jsonl: Option<(String, u64)>,
    postgres: Option<(String, u64)>,
}

/// JSONL lines (read lazily) merged with Postgres lines by time, newest first.
struct Merged {
    jsonl: Option<JsonlStream>,
    postgres: Vec<Located>,
    postgres_next: usize,
    position: Cursor,
}

impl Merged {
    fn open(dir: &Path, filter: &LogFilter, cursor: &Cursor, keep: usize) -> Result<Self, QueryError> {
        let jsonl = if filter.source == Some(Source::Postgres) {
            None
        } else {
            Some(JsonlStream::open(dir, filter, cursor.jsonl.clone(), keep)?)
        };
        let postgres = if filter.source.is_none() || filter.source == Some(Source::Postgres) {
            let (dated, undated) = read_postgres(dir, filter)?;
            merge(dated, undated)
        } else {
            Vec::new()
        };
        let postgres_next = match &cursor.postgres {
            None => 0,
            Some((file_key, line_index)) => {
                postgres
                    .iter()
                    .position(|item| &item.file_key == file_key && item.line_index == *line_index)
                    .ok_or(QueryError::BadCursor)?
                    + 1
            }
        };
        Ok(Self {
            jsonl,
            postgres,
            postgres_next,
            position: cursor.clone(),
        })
    }

    fn next(&mut self) -> Result<Option<Located>, QueryError> {
        let from_jsonl = match (
            self.jsonl.as_mut().map(JsonlStream::peek).transpose()?.flatten(),
            self.postgres.get(self.postgres_next),
        ) {
            (None, None) => return Ok(None),
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(left), Some(right)) => match (left.timestamp, right.timestamp) {
                // An undated Postgres line already sits after its dated neighbour.
                (_, None) => false,
                (None, _) => true,
                (Some(left), Some(right)) => left >= right,
            },
        };
        if from_jsonl {
            let item = self
                .jsonl
                .as_mut()
                .and_then(|stream| stream.take())
                .expect("peeked line");
            self.position.jsonl = Some((item.file_key.clone(), item.line_index));
            Ok(Some(item))
        } else {
            let item = self.postgres[self.postgres_next].clone_located();
            self.postgres_next += 1;
            self.position.postgres = Some((item.file_key.clone(), item.line_index));
            Ok(Some(item))
        }
    }

    fn has_more(&mut self) -> Result<bool, QueryError> {
        let jsonl_more = match self.jsonl.as_mut() {
            Some(stream) => stream.peek()?.is_some(),
            None => false,
        };
        Ok(jsonl_more || self.postgres_next < self.postgres.len())
    }

    fn position(&self) -> Cursor {
        self.position.clone()
    }
}

/// One JSONL segment on disk.
struct SegmentFile {
    log: crate::logs::segments::LogName,
    path: PathBuf,
}

impl SegmentFile {
    fn key(&self) -> String {
        format!("{}.{}", self.log.date, self.log.seg)
    }
}

fn parse_file_key(key: &str) -> Option<(Date, u32)> {
    let (date, seg) = key.rsplit_once('.')?;
    Some((crate::logs::segments::parse_date(date)?, seg.parse().ok()?))
}

/// Matching JSONL lines, newest first, across segments.
struct JsonlStream {
    filter: LogFilter,
    keep: usize,
    /// Segments still to read, newest first.
    files: std::collections::VecDeque<(SegmentFile, u64)>,
    current: Option<(String, LineSource)>,
    peeked: Option<Located>,
}

enum LineSource {
    Plain(crate::logs::segments::ReverseLines),
    Loaded(std::vec::IntoIter<(u64, String)>),
}

impl JsonlStream {
    fn open(
        dir: &Path,
        filter: &LogFilter,
        after: Option<(String, u64)>,
        keep: usize,
    ) -> Result<Self, QueryError> {
        let mut segments = match fs::read_dir(dir) {
            Ok(entries) => segment_files_from(entries)?,
            Err(error) if store_disappeared(&error) => return Err(QueryError::StoreMissing),
            Err(error) => return Err(error.into()),
        };
        segments.sort_by_key(|file| std::cmp::Reverse((file.log.date, file.log.seg)));
        let bound = match &after {
            None => None,
            Some((key, offset)) => Some((parse_file_key(key).ok_or(QueryError::BadCursor)?, *offset)),
        };
        let mut files = std::collections::VecDeque::new();
        for file in segments {
            if !day_overlaps(file.log.date, filter) {
                continue;
            }
            let end = match bound {
                None => u64::MAX,
                Some((at, offset)) => {
                    let here = (file.log.date, file.log.seg);
                    if here > at {
                        continue;
                    }
                    if here == at {
                        offset
                    } else {
                        u64::MAX
                    }
                }
            };
            files.push_back((file, end));
        }
        Ok(Self {
            filter: filter.clone(),
            keep,
            files,
            current: None,
            peeked: None,
        })
    }

    fn peek(&mut self) -> Result<Option<&Located>, QueryError> {
        if self.peeked.is_none() {
            self.peeked = self.advance()?;
        }
        Ok(self.peeked.as_ref())
    }

    fn take(&mut self) -> Option<Located> {
        self.peeked.take()
    }

    fn advance(&mut self) -> Result<Option<Located>, QueryError> {
        loop {
            if self.current.is_none() {
                let Some((file, end)) = self.files.pop_front() else {
                    return Ok(None);
                };
                let source = if file.log.compressed {
                    let filter = self.filter.clone();
                    let lines = crate::logs::segments::compressed_lines_before(
                        &file.path,
                        end,
                        self.keep,
                        |raw| matching_raw(&filter, raw).is_some(),
                    )?;
                    LineSource::Loaded(lines.into_iter())
                } else {
                    match crate::logs::segments::ReverseLines::before(&file.path, end) {
                        Ok(lines) => LineSource::Plain(lines),
                        Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                        Err(error) => return Err(error.into()),
                    }
                };
                self.current = Some((file.key(), source));
            }
            let (key, source) = self.current.as_mut().expect("current file");
            let next = match source {
                LineSource::Plain(lines) => lines.next().transpose()?,
                LineSource::Loaded(lines) => lines.next(),
            };
            let Some((offset, raw)) = next else {
                self.current = None;
                continue;
            };
            if raw.trim().is_empty() {
                continue;
            }
            if let Some((line, timestamp)) = matching_raw(&self.filter, &raw) {
                return Ok(Some(Located {
                    timestamp,
                    postgres: false,
                    file_key: key.clone(),
                    line_index: offset,
                    line,
                    raw,
                }));
            }
        }
    }
}

fn matching_raw(filter: &LogFilter, raw: &str) -> Option<(LogLine, Option<OffsetDateTime>)> {
    let (line, timestamp) = parse_jsonl(raw)?;
    accepts(filter, &line, timestamp, raw).then_some((line, timestamp))
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

#[derive(Clone)]
struct Located {
    timestamp: Option<OffsetDateTime>,
    postgres: bool,
    file_key: String,
    line_index: u64,
    line: LogLine,
    raw: String,
}

impl Located {
    fn clone_located(&self) -> Self {
        self.clone()
    }
}

fn merge(mut dated: Vec<Located>, undated: Vec<Located>) -> Vec<Located> {
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
    // Lines with no timestamp stay together after the dated lines from that file.
    let mut groups: std::collections::BTreeMap<String, Vec<Located>> =
        std::collections::BTreeMap::new();
    for item in undated {
        groups.entry(item.file_key.clone()).or_default().push(item);
    }
    let mut inserts = Vec::new();
    for (file_key, mut lines) in groups {
        lines.sort_by_key(|item| item.line_index);
        let at = dated
            .iter()
            .rposition(|item| item.postgres && item.file_key == file_key)
            .map(|index| index + 1)
            .or_else(|| {
                dated
                    .iter()
                    .rposition(|item| item.postgres)
                    .map(|index| index + 1)
            })
            .unwrap_or(dated.len());
        inserts.push((at, lines));
    }
    inserts.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(left.1[0].file_key.cmp(&right.1[0].file_key))
    });
    for (at, lines) in inserts {
        dated.splice(at..at, lines);
    }
    dated
}

/// Empty path, a path that is not there, or a path that is not a directory.
fn log_store_missing(dir: &Path) -> bool {
    dir.as_os_str().is_empty() || !dir.is_dir()
}

fn store_disappeared(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}

/// One file per segment. A segment that exists both plain and compressed
/// (compression in progress) is read plain.
fn segment_files_from(entries: fs::ReadDir) -> Result<Vec<SegmentFile>, QueryError> {
    let mut found: Vec<SegmentFile> = Vec::new();
    for entry in entries {
        let entry = entry?;
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(log) = crate::logs::segments::parse_log_name(&name) else {
            continue;
        };
        if let Some(existing) = found
            .iter_mut()
            .find(|file| file.log.date == log.date && file.log.seg == log.seg)
        {
            if existing.log.compressed && !log.compressed {
                existing.path = entry.path();
                existing.log = log;
            }
            continue;
        }
        found.push(SegmentFile {
            log,
            path: entry.path(),
        });
    }
    Ok(found)
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
    let files = match crate::logs::postgres_files::list(dir) {
        Ok(files) => files,
        Err(error) if store_disappeared(&error) => return Err(QueryError::StoreMissing),
        Err(error) => return Err(error.into()),
    };
    let mut dated = Vec::new();
    let mut undated = Vec::new();
    for file in files {
        let text = read_lossy(&file.path, false)?;
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
                file_key: file.name.clone(),
                line_index: u64::try_from(index).unwrap_or(u64::MAX),
                line,
                raw: raw.to_string(),
            };
            if timestamp.is_some() {
                dated.push(located);
            } else {
                undated.push(located);
            }
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

fn encode_cursor(cursor: &Cursor) -> String {
    let part = |position: &Option<(String, u64)>| match position {
        Some((key, index)) => format!("{key}:{index}"),
        None => "-".to_string(),
    };
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!(
        "j={}|p={}",
        part(&cursor.jsonl),
        part(&cursor.postgres)
    ))
}

fn decode_cursor(cursor: &str) -> Option<Cursor> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let (jsonl, postgres) = text.split_once('|')?;
    let part = |value: &str, prefix: &str| -> Option<Option<(String, u64)>> {
        let value = value.strip_prefix(prefix)?;
        if value == "-" {
            return Some(None);
        }
        let (key, index) = value.rsplit_once(':')?;
        if key.is_empty() {
            return None;
        }
        Some(Some((key.to_string(), index.parse().ok()?)))
    };
    Some(Cursor {
        jsonl: part(jsonl, "j=")?,
        postgres: part(postgres, "p=")?,
    })
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
    fn pages_walk_back_across_segments_and_a_compressed_day() {
        let dir = tempfile::tempdir().expect("tempdir");
        let day = |n: usize| format!("2026-10-1{n}");
        let mut expected = Vec::new();
        // Yesterday, compressed: lines 0..30. Today: segments 0, 1, 2.
        let mut yesterday = String::new();
        for i in 0..30 {
            let message = format!("m{i:03}");
            yesterday.push_str(&jsonl(&format!("{}T01:00:{:02}Z", day(3), i), "INFO", "server", &message, ""));
            yesterday.push('\n');
            expected.push(message);
        }
        write_zst(&dir.path().join(format!("coppice.{}.jsonl.zst", day(3))), &yesterday);
        let mut i = 30;
        for seg in 0..3 {
            let mut text = String::new();
            for _ in 0..25 {
                let message = format!("m{i:03}");
                text.push_str(&jsonl(&format!("{}T02:{:02}:{:02}Z", day(4), i / 60, i % 60), "INFO", "server", &message, ""));
                text.push('\n');
                expected.push(message);
                i += 1;
            }
            fs::write(dir.path().join(format!("coppice.{}.{seg}.jsonl", day(4))), text).expect("write");
        }
        expected.reverse();

        let mut seen = Vec::new();
        let mut cursor = None;
        loop {
            let mut request = params(Some("server"), None, None, None);
            request.cursor = cursor.clone();
            request.limit = 7;
            let page = super::query(dir.path(), &request).expect("page");
            assert!(page.lines.len() <= 7);
            seen.extend(page.lines.into_iter().map(|line| line.message));
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        assert_eq!(seen, expected);
    }

    #[test]
    fn postgres_lines_are_their_own_source_and_undated_lines_follow_dated_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("coppice.2026-10-14.jsonl"),
            // Appended in time order, as the server writes them.
            format!(
                "{}\n{}\n",
                jsonl(
                    "2026-10-14T01:00:00Z",
                    "INFO",
                    "cli",
                    "older",
                    r#","run_id":"run-1","stream":"stdout""#
                ),
                jsonl("2026-10-14T04:00:00Z", "INFO", "server", "newer", ""),
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
    fn rotated_postgres_files_are_read_and_exported_with_their_own_undated_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("postgresql-2026-10-14_030000.log"),
            "2026-10-14 03:00:00 UTC [1] LOG:  older-db\nplain-older\n",
        )
        .expect("older");
        fs::write(
            dir.path().join("postgresql-2026-10-15_050000.log"),
            "2026-10-15 05:00:00 UTC [1] LOG:  newer-db\nplain-newer\n",
        )
        .expect("newer");
        fs::write(
            dir.path().join("postgres.log"),
            "2026-10-14 04:00:00 UTC [1] LOG:  startup\n",
        )
        .expect("startup");
        let target = dir.path().join("secret.txt");
        fs::write(&target, "2026-10-15 06:00:00 UTC [1] LOG:  hidden\n").expect("secret");
        std::os::unix::fs::symlink(&target, dir.path().join("postgresql-2026-10-15_060000.log"))
            .expect("symlink");

        let page = query(dir.path(), &params(Some("postgres"), None, None, None)).expect("db");
        let messages: Vec<_> = page
            .lines
            .iter()
            .map(|line| line.message.as_str())
            .collect();
        let newer = messages
            .iter()
            .position(|m| m.contains("newer-db"))
            .unwrap();
        let plain_newer = messages.iter().position(|m| *m == "plain-newer").unwrap();
        let startup = messages.iter().position(|m| m.contains("startup")).unwrap();
        let older = messages
            .iter()
            .position(|m| m.contains("older-db"))
            .unwrap();
        let plain_older = messages.iter().position(|m| *m == "plain-older").unwrap();
        assert!(newer < plain_newer, "{messages:?}");
        assert!(plain_newer < startup, "{messages:?}");
        assert!(startup < older, "{messages:?}");
        assert!(older < plain_older, "{messages:?}");
        assert!(messages.iter().all(|message| !message.contains("hidden")));

        let filter = filter_from_params(&FilterParams::default()).expect("filter");
        let exported = export_lines(dir.path(), &filter).expect("export");
        assert!(exported.iter().any(|line| line.contains("newer-db")));
        assert!(exported.iter().any(|line| line.contains("older-db")));
        assert!(exported.iter().all(|line| !line.contains("hidden")));
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

    #[test]
    fn a_missing_log_directory_is_unavailable_and_an_empty_one_is_not() {
        let parent = tempfile::tempdir().expect("parent");
        let missing = query(&parent.path().join("gone"), &params(None, None, None, None));
        assert!(matches!(missing, Err(QueryError::StoreMissing)));

        let file = tempfile::NamedTempFile::new().expect("file");
        let as_file = query(file.path(), &params(None, None, None, None));
        assert!(matches!(as_file, Err(QueryError::StoreMissing)));

        let empty = tempfile::tempdir().expect("dir");
        let page = query(empty.path(), &params(None, None, None, None)).expect("empty");
        assert!(page.lines.is_empty());
    }
}
