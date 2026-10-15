//! Zip of the current Logs view, with a second redaction pass.

use std::io::{self, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::mask;
use super::read::{self, LogFilter, QueryError};

pub const ARCHIVE_NAME: &str = "coppice-logs.zip";
/// Log encoding, not interface text.
pub const REDACTED: &str = "\u{2039}redacted\u{203A}";

const HEADER_NAMES: &[&str] = &["authorization", "cookie"];
const ASSIGN_KEYS: &[&str] = &["access_token", "api_key", "password", "secret", "token"];

pub struct VersionFile {
    pub app_version: String,
    pub commit: String,
    pub os: String,
    pub arch: String,
    pub os_version: String,
    pub shell_version: Option<String>,
}

impl VersionFile {
    fn text(&self) -> String {
        let mut lines = vec![
            field("app_version", &self.app_version),
            field("commit", &self.commit),
            field("os", &self.os),
            field("arch", &self.arch),
            field("os_version", &self.os_version),
        ];
        if let Some(shell) = self
            .shell_version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            lines.push(field("shell_version", shell));
        }
        let mut text = lines.join("\n");
        text.push('\n');
        text
    }
}

fn field(name: &str, value: &str) -> String {
    let value = redact_export_line(value).replace(['\n', '\r'], " ");
    format!("{name}={value}")
}

/// Matching lines, newest first, after the export redaction pass.
pub fn build_zip(
    dir: &Path,
    filter: &LogFilter,
    versions: &VersionFile,
) -> Result<Vec<u8>, QueryError> {
    let lines = read::export_lines(dir, filter)?;
    zip_bytes(&lines, versions).map_err(QueryError::Io)
}

fn zip_bytes(lines: &[String], versions: &VersionFile) -> io::Result<Vec<u8>> {
    let redacted: Vec<String> = lines.iter().map(|line| redact_export_line(line)).collect();
    let mut logs = redacted.join("\n");
    if !logs.is_empty() {
        logs.push('\n');
    }
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644);
        zip.start_file("logs.jsonl", options)
            .map_err(io::Error::other)?;
        zip.write_all(logs.as_bytes())?;
        zip.start_file("versions.txt", options)
            .map_err(io::Error::other)?;
        zip.write_all(versions.text().as_bytes())?;
        zip.finish().map_err(io::Error::other)?;
    }
    Ok(cursor.into_inner())
}

/// Write-time mask again, then Authorization, Cookie, Bearer, and assignments.
pub fn redact_export_line(line: &str) -> String {
    let masked = mask::mask_bytes(line.as_bytes());
    let text = String::from_utf8_lossy(&masked);
    redact_patterns(&text)
}

fn redact_patterns(input: &str) -> String {
    let with_headers = redact_headers(input);
    let with_bearer = redact_bearer(&with_headers);
    redact_assignments(&with_bearer)
}

fn redact_headers(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(name) = header_at(bytes, index) {
            if let Some(span) = header_span(bytes, index, name.len()) {
                out.push_str(&input[index..span.copy_end]);
                out.push_str(REDACTED);
                if span.close_quote {
                    out.push('"');
                }
                index = span.resume;
                continue;
            }
        }
        push_one(input, bytes, &mut index, &mut out);
    }
    out
}

fn redact_bearer(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bearer_at(bytes, index) {
            let mut cursor = index + "bearer".len();
            let token_at = skip_space(bytes, cursor);
            if token_at > cursor && token_at < bytes.len() && !is_token_end(bytes[token_at]) {
                out.push_str(&input[index..token_at]);
                cursor = token_at;
                while cursor < bytes.len() && !is_token_end(bytes[cursor]) {
                    cursor += 1;
                }
                out.push_str(REDACTED);
                index = cursor;
                continue;
            }
        }
        push_one(input, bytes, &mut index, &mut out);
    }
    out
}

fn redact_assignments(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(key) = assign_at(bytes, index) {
            if let Some(span) = assignment_span(bytes, index, key.len()) {
                out.push_str(&input[index..span.copy_end]);
                out.push_str(REDACTED);
                if span.close_quote {
                    out.push('"');
                }
                index = span.resume;
                continue;
            }
        }
        push_one(input, bytes, &mut index, &mut out);
    }
    out
}

struct Span {
    copy_end: usize,
    resume: usize,
    close_quote: bool,
}

fn header_at(bytes: &[u8], index: usize) -> Option<&'static str> {
    HEADER_NAMES.iter().copied().find(|name| {
        starts_with_ignore_ascii_case(bytes, index, name)
            && boundary_before(bytes, index)
            && boundary_after(bytes, index + name.len())
    })
}

fn assign_at(bytes: &[u8], index: usize) -> Option<&'static str> {
    ASSIGN_KEYS.iter().copied().find(|name| {
        starts_with_ignore_ascii_case(bytes, index, name)
            && boundary_before(bytes, index)
            && boundary_after(bytes, index + name.len())
    })
}

fn bearer_at(bytes: &[u8], index: usize) -> bool {
    starts_with_ignore_ascii_case(bytes, index, "bearer")
        && boundary_before(bytes, index)
        && boundary_after(bytes, index + "bearer".len())
}

fn header_span(bytes: &[u8], start: usize, name_len: usize) -> Option<Span> {
    let mut cursor = start + name_len;
    if cursor < bytes.len() && bytes[cursor] == b'"' {
        cursor += 1;
    }
    cursor = skip_space(bytes, cursor);
    if cursor >= bytes.len() || bytes[cursor] != b':' {
        return None;
    }
    cursor += 1;
    cursor = skip_space(bytes, cursor);
    value_span(bytes, cursor, inside_string(bytes, start))
}

fn assignment_span(bytes: &[u8], start: usize, key_len: usize) -> Option<Span> {
    let mut cursor = start + key_len;
    if cursor < bytes.len() && bytes[cursor] == b'"' {
        cursor += 1;
    }
    cursor = skip_space(bytes, cursor);
    if cursor >= bytes.len() || (bytes[cursor] != b'=' && bytes[cursor] != b':') {
        return None;
    }
    cursor += 1;
    cursor = skip_space(bytes, cursor);
    if cursor < bytes.len() && bytes[cursor] == b'"' {
        return quoted_value(bytes, cursor);
    }
    let copy_end = cursor;
    while cursor < bytes.len() && !is_assign_end(bytes[cursor]) {
        cursor += 1;
    }
    Some(Span {
        copy_end,
        resume: cursor,
        close_quote: false,
    })
}

/// Quoted value, or the rest of a surrounding string, or the rest of the line.
fn value_span(bytes: &[u8], value_at: usize, inside_string: bool) -> Option<Span> {
    if value_at < bytes.len() && bytes[value_at] == b'"' {
        return quoted_value(bytes, value_at);
    }
    let copy_end = value_at;
    let mut cursor = value_at;
    if inside_string {
        while cursor < bytes.len() && bytes[cursor] != b'"' && bytes[cursor] != b'\n' {
            if bytes[cursor] == b'\\' && cursor + 1 < bytes.len() {
                cursor += 2;
                continue;
            }
            cursor += 1;
        }
    } else {
        while cursor < bytes.len() && bytes[cursor] != b'\n' && bytes[cursor] != b'\r' {
            cursor += 1;
        }
    }
    Some(Span {
        copy_end,
        resume: cursor,
        close_quote: false,
    })
}

fn quoted_value(bytes: &[u8], quote_at: usize) -> Option<Span> {
    let mut cursor = quote_at + 1;
    while cursor < bytes.len() && bytes[cursor] != b'"' && bytes[cursor] != b'\n' {
        if bytes[cursor] == b'\\' && cursor + 1 < bytes.len() {
            cursor += 2;
            continue;
        }
        cursor += 1;
    }
    let close_quote = cursor < bytes.len() && bytes[cursor] == b'"';
    let resume = if close_quote { cursor + 1 } else { cursor };
    Some(Span {
        copy_end: quote_at + 1,
        resume,
        close_quote,
    })
}

fn inside_string(bytes: &[u8], end: usize) -> bool {
    let mut quotes = 0;
    let mut index = 0;
    while index < end {
        if bytes[index] == b'\\' {
            index += 2;
            continue;
        }
        if bytes[index] == b'"' {
            quotes += 1;
        }
        index += 1;
    }
    quotes % 2 == 1
}

fn skip_space(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t') {
        index += 1;
    }
    index
}

fn boundary_before(bytes: &[u8], index: usize) -> bool {
    index == 0 || !is_ident(bytes[index - 1])
}

fn boundary_after(bytes: &[u8], index: usize) -> bool {
    index >= bytes.len() || !is_ident(bytes[index])
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_token_end(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'"' | b',' | b'}' | b';')
}

fn is_assign_end(byte: u8) -> bool {
    is_token_end(byte) || byte == b'&'
}

fn starts_with_ignore_ascii_case(bytes: &[u8], index: usize, name: &str) -> bool {
    let end = index + name.len();
    end <= bytes.len() && bytes[index..end].eq_ignore_ascii_case(name.as_bytes())
}

fn push_one(input: &str, bytes: &[u8], index: &mut usize, out: &mut String) {
    if bytes[*index].is_ascii() {
        out.push(bytes[*index] as char);
        *index += 1;
        return;
    }
    let ch = input[*index..].chars().next().unwrap_or('\u{FFFD}');
    out.push(ch);
    *index += ch.len_utf8();
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use crate::logs::SecretGuard;

    fn versions(shell: Option<&str>) -> VersionFile {
        VersionFile {
            app_version: "0.1.0".into(),
            commit: "abc123".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            os_version: "Debian GNU/Linux 12".into(),
            shell_version: shell.map(str::to_string),
        }
    }

    fn unzip(bytes: &[u8]) -> (String, String) {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
        let mut logs = String::new();
        archive
            .by_name("logs.jsonl")
            .expect("logs.jsonl")
            .read_to_string(&mut logs)
            .expect("read logs");
        let mut versions = String::new();
        archive
            .by_name("versions.txt")
            .expect("versions.txt")
            .read_to_string(&mut versions)
            .expect("read versions");
        (logs, versions)
    }

    fn filter_source(source: &str) -> LogFilter {
        read::filter_from_params(&read::FilterParams {
            source: Some(source.to_string()),
            level: Some("info".to_string()),
            from: None,
            to: None,
            run: None,
            q: None,
        })
        .expect("filter")
    }

    #[test]
    fn zip_contains_the_filtered_lines_and_versions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let http = r#"{"timestamp":"2026-10-14T03:35:01Z","level":"INFO","target":"t","fields":{"message":"http request","source":"http"}}"#;
        let cli = r#"{"timestamp":"2026-10-14T03:35:02Z","level":"INFO","target":"t","fields":{"message":"cli-only-marker","source":"cli"}}"#;
        std::fs::write(
            dir.path().join("coppice.2026-10-14.jsonl"),
            format!("{http}\n{cli}\n"),
        )
        .expect("write");

        let bytes =
            build_zip(dir.path(), &filter_source("http"), &versions(Some("1.4.2"))).expect("zip");
        let (logs, versions) = unzip(&bytes);
        assert!(logs.contains("http request"), "{logs}");
        assert!(!logs.contains("cli-only-marker"), "{logs}");
        assert!(versions.contains("app_version=0.1.0\n"), "{versions}");
        assert!(versions.contains("commit=abc123\n"), "{versions}");
        assert!(versions.contains("os=linux\n"), "{versions}");
        assert!(versions.contains("arch=x86_64\n"), "{versions}");
        assert!(
            versions.contains("os_version=Debian GNU/Linux 12\n"),
            "{versions}"
        );
        assert!(versions.contains("shell_version=1.4.2\n"), "{versions}");
    }

    #[test]
    fn versions_omit_shell_version_when_the_client_does_not_send_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("coppice.2026-10-14.jsonl"), "").expect("write");
        let bytes = build_zip(dir.path(), &filter_source("server"), &versions(None)).expect("zip");
        let (_logs, versions) = unzip(&bytes);
        assert!(!versions.contains("shell_version"), "{versions}");
    }

    #[test]
    fn export_removes_a_registered_secret_and_header_shapes() {
        let token = "coppice-export-7kQ9mP2x";
        let _guard = SecretGuard::register([("COPPICE_MCP_TOKEN", token)]);
        let line = format!(
            r#"{{"message":"connector returned {token} Authorization: Bearer export-bearer-token Cookie: session=abc token=still-here"}}"#
        );
        let redacted = redact_export_line(&line);
        assert!(
            !redacted.contains(token),
            "registered secret remained: {redacted}"
        );
        assert!(
            redacted.contains("\u{2039}COPPICE_MCP_TOKEN\u{203A}"),
            "{redacted}"
        );
        assert!(!redacted.contains("export-bearer-token"), "{redacted}");
        assert!(!redacted.contains("session=abc"), "{redacted}");
        assert!(!redacted.contains("still-here"), "{redacted}");
        assert!(redacted.contains(REDACTED), "{redacted}");
        assert_eq!(
            redact_export_line("Cookie: session=abc"),
            format!("Cookie: {REDACTED}")
        );
        assert_eq!(
            redact_export_line(r#""Authorization":"Bearer export-bearer-token""#),
            format!(r#""Authorization":"{REDACTED}""#)
        );
    }

    #[test]
    fn export_redacts_each_assignment_shape() {
        let line = redact_export_line(
            r#"password = "hunter2xx" api_key=sk-123456 access_token: xyz123 Token=Case"#,
        );
        assert!(!line.contains("hunter2xx"), "{line}");
        assert!(!line.contains("sk-123456"), "{line}");
        assert!(!line.contains("xyz123"), "{line}");
        assert!(!line.contains("Case"), "{line}");
        assert!(line.contains("password = \""), "{line}");
        assert!(line.contains(REDACTED), "{line}");
    }

    #[test]
    fn ordinary_words_are_not_redacted() {
        let line = "the secret plan and tokenize stay, and my_secret=abc stays";
        assert_eq!(redact_export_line(line), line);
    }

    #[test]
    fn a_bearer_token_outside_a_header_is_redacted() {
        let line = redact_export_line("user sent Bearer eyJhbGci.payload");
        assert_eq!(line, format!("user sent Bearer {REDACTED}"));
    }
}
