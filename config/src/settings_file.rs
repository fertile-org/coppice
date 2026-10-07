//! Read and save the user's config file without rewriting comments or layout.
//!
//! Validation uses the same figment load and [`crate::KnowledgeConfig::validate`]
//! path as startup. The bytes written are the editor text, not a reserialization.

use std::io;
use std::path::{Path, PathBuf};

use figment::providers::{Format, Toml};
use sha2::{Digest, Sha256};
use toml_edit::Item;

use crate::connector_file::write_toml_atomic;
use crate::AppConfig;

/// A config file the editor can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDocument {
    pub text: String,
    pub revision: String,
    pub backup_available: bool,
}

/// Result of a verbatim save.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveOutcome {
    pub revision: String,
    pub restart_required: bool,
    pub backup_available: bool,
    pub parsed: AppConfig,
}

/// Parse or schema failure. `line` and `column` are 1-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigTextError {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

#[derive(Debug)]
pub enum SaveError {
    Invalid(ConfigTextError),
    Conflict { text: String, revision: String },
    Io(io::Error),
}

impl From<io::Error> for SaveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// `config.toml` → `config.toml.bak` in the same directory.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("config.toml"))
        .to_os_string();
    name.push(".bak");
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
        _ => PathBuf::from(name),
    }
}

/// Stable identity of the file bytes, used to detect edits made outside the editor.
pub fn content_revision(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn read_config_document(path: &Path) -> io::Result<ConfigDocument> {
    let text = if path.is_file() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    Ok(ConfigDocument {
        revision: content_revision(text.as_bytes()),
        backup_available: backup_path(path).is_file(),
        text,
    })
}

pub fn read_backup_text(path: &Path) -> io::Result<Option<String>> {
    let backup = backup_path(path);
    if !backup.is_file() {
        return Ok(None);
    }
    Ok(Some(std::fs::read_to_string(backup)?))
}

/// Same parser and checks as startup: defaults, then this TOML, then knowledge rules.
pub fn validate_config_text(text: &str) -> Result<AppConfig, ConfigTextError> {
    let figment = AppConfig::defaults_figment().merge(Toml::string(text));
    match AppConfig::load_figment(figment) {
        Ok(config) => Ok(config),
        Err(error) => Err(error_from_figment(text, &error)),
    }
}

/// True when applying `after` needs a process restart.
///
/// Connector settings are rebuilt in-process, except OpenCode's command and
/// serve hostname, which are fixed when that helper is started.
pub fn changes_need_restart(before: &AppConfig, after: &AppConfig) -> bool {
    if before.agent.connectors.opencode.command != after.agent.connectors.opencode.command
        || before.agent.connectors.opencode.serve_hostname
            != after.agent.connectors.opencode.serve_hostname
    {
        return true;
    }
    let mut before = before.clone();
    let mut after = after.clone();
    before.agent.connectors = crate::AgentConnectorsConfig::default();
    after.agent.connectors = crate::AgentConnectorsConfig::default();
    before != after
}

/// Validate `text`, then replace `path` with those exact bytes.
///
/// A stale `base_revision` writes nothing. A last-good backup is updated only
/// when the file currently on disk itself validates.
pub fn save_config_verbatim(
    path: &Path,
    text: &str,
    base_revision: &str,
) -> Result<SaveOutcome, SaveError> {
    let parsed = validate_config_text(text).map_err(SaveError::Invalid)?;
    let current_text = if path.is_file() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let current_revision = content_revision(current_text.as_bytes());
    if current_revision != base_revision {
        return Err(SaveError::Conflict {
            text: current_text,
            revision: current_revision,
        });
    }

    let restart_required = match validate_config_text(&current_text) {
        Ok(previous) => changes_need_restart(&previous, &parsed),
        Err(_) => true,
    };

    if path.is_file() && validate_config_text(&current_text).is_ok() {
        write_toml_atomic(&backup_path(path), &current_text)?;
    }
    write_toml_atomic(path, text)?;

    Ok(SaveOutcome {
        revision: content_revision(text.as_bytes()),
        restart_required,
        backup_available: backup_path(path).is_file(),
        parsed,
    })
}

fn error_from_figment(text: &str, error: &figment::Error) -> ConfigTextError {
    if let Err(syntax) = toml::from_str::<toml::Value>(text) {
        let (line, column) = syntax
            .span()
            .map(|span| offset_line_col(text, span.start))
            .unwrap_or((1, 1));
        return ConfigTextError {
            line,
            column,
            message: single_line(syntax.message()),
        };
    }
    let message = single_line(&error.kind.to_string());
    let key = if error.path.is_empty() {
        None
    } else {
        Some(error.path.join("."))
    };
    let (line, column) = key
        .as_deref()
        .and_then(|key| lookup_span(text, key))
        .or_else(|| line_col_for_message(text, &message))
        .unwrap_or((1, 1));
    ConfigTextError {
        line,
        column,
        message,
    }
}

fn single_line(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn line_col_for_message(text: &str, message: &str) -> Option<(u32, u32)> {
    let mut best: Option<&str> = None;
    for candidate in dotted_candidates(message).chain(quoted_candidates(message)) {
        let candidate = candidate.strip_prefix("default.").unwrap_or(candidate);
        if lookup_span(text, candidate).is_none() {
            continue;
        }
        if best.is_none_or(|current| candidate.len() > current.len()) {
            best = Some(candidate);
        }
    }
    best.and_then(|key| lookup_span(text, key))
}

fn dotted_candidates(message: &str) -> impl Iterator<Item = &str> {
    let bytes = message.as_bytes();
    let mut found = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if is_key_start(bytes[index]) {
            let start = index;
            index += 1;
            let mut has_dot = false;
            while index < bytes.len() && is_key_char(bytes[index]) {
                if bytes[index] == b'.' {
                    has_dot = true;
                }
                index += 1;
            }
            if has_dot {
                let mut end = index;
                while end > start && bytes[end - 1] == b'.' {
                    end -= 1;
                }
                if end > start && message[start..end].contains('.') {
                    found.push(&message[start..end]);
                }
            }
        } else {
            index += 1;
        }
    }
    found.into_iter()
}

fn quoted_candidates(message: &str) -> impl Iterator<Item = &str> {
    let mut found = Vec::new();
    let mut rest = message;
    while let Some(start) = rest.find('"') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('"') else {
            break;
        };
        found.push(&rest[..end]);
        rest = &rest[end + 1..];
    }
    found.into_iter()
}

fn is_key_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_key_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
}

fn lookup_span(text: &str, dotted: &str) -> Option<(u32, u32)> {
    let dotted = dotted.strip_prefix("default.").unwrap_or(dotted);
    let doc = toml_edit::ImDocument::<&str>::parse(text).ok()?;
    let mut table = doc.as_table();
    let keys: Vec<&str> = dotted.split('.').filter(|key| !key.is_empty()).collect();
    if keys.is_empty() {
        return None;
    }
    for (index, key) in keys.iter().enumerate() {
        let child = table.get(key)?;
        if index + 1 == keys.len() {
            let start = item_span_start(child).or_else(|| key_span_start(table, key))?;
            return Some(offset_line_col(text, start));
        }
        table = child.as_table()?;
    }
    None
}

fn item_span_start(item: &Item) -> Option<usize> {
    item.span().map(|span| span.start)
}

fn key_span_start(table: &toml_edit::Table, key: &str) -> Option<usize> {
    table
        .key(key)
        .and_then(|key| key.span())
        .map(|span| span.start)
}

fn offset_line_col(text: &str, offset: usize) -> (u32, u32) {
    let mut line = 1u32;
    let mut column = 1u32;
    for (index, ch) in text.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn commented_config_is_saved_exactly_and_backs_up_the_previous_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "# keep me\n\n[server]\nport = 5000 # inline\n";
        write(&path, original);
        let next = "# keep me\n\n[server]\nport = 5001 # inline\n\n# tail\n";

        let outcome = save_config_verbatim(&path, next, &content_revision(original.as_bytes()))
            .expect("save");

        assert_eq!(std::fs::read_to_string(&path).unwrap(), next);
        assert_eq!(
            std::fs::read_to_string(backup_path(&path)).unwrap(),
            original
        );
        assert!(outcome.restart_required);
        assert!(outcome.backup_available);
        assert_eq!(outcome.parsed.server.port, 5001);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().all(|name| !name.contains("writing")),
            "{names:?}"
        );
    }

    #[test]
    fn comment_only_edit_does_not_need_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "# keep\n[server]\nport = 5000\n";
        write(&path, original);
        let next = "# keep\n# added\n[server]\nport = 5000\n";

        let outcome = save_config_verbatim(&path, next, &content_revision(original.as_bytes()))
            .expect("save");

        assert_eq!(std::fs::read_to_string(&path).unwrap(), next);
        assert!(!outcome.restart_required);
    }

    #[test]
    fn connector_flag_edit_does_not_need_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "# note\n[agent.connectors.claude-code]\nenabled = false\n";
        write(&path, original);
        let next = "# note\n[agent.connectors.claude-code]\nenabled = true\n";

        let outcome = save_config_verbatim(&path, next, &content_revision(original.as_bytes()))
            .expect("save");

        assert!(
            !outcome.restart_required,
            "connector changes are applied live"
        );
        assert!(outcome.parsed.agent.connectors.claude_code.enabled);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), next);
    }

    #[test]
    fn opencode_hostname_change_needs_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "# note\n";
        write(&path, original);
        let next = "# note\n[agent.connectors.opencode]\nserve_hostname = \"localhost\"\n";

        let outcome = save_config_verbatim(&path, next, &content_revision(original.as_bytes()))
            .expect("save");

        assert!(outcome.restart_required);
    }

    #[test]
    fn invalid_text_does_not_write_or_replace_the_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "# keep me\n[server]\nport = 5000\n";
        write(&path, original);
        let backup = "# last good\n[server]\nport = 1\n";
        write(&backup_path(&path), backup);
        let bad = "# keep me\n[server]\nport = [\n";

        let error = save_config_verbatim(&path, bad, &content_revision(original.as_bytes()))
            .expect_err("invalid");

        let SaveError::Invalid(detail) = error else {
            panic!("expected invalid");
        };
        assert_eq!(detail.line, 4, "{}", detail.message);
        assert_eq!(detail.message, "invalid array expected `]`");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), backup);
    }

    #[test]
    fn schema_error_points_at_the_key_and_uses_the_validator_message() {
        let text = "# head\n[knowledge.retrieval]\ntop_k = 0\n";
        let error = validate_config_text(text).expect_err("top_k");
        assert_eq!(error.line, 3, "{}", error.message);
        assert_eq!(
            error.message,
            "knowledge.retrieval.top_k must be between 1 and 20"
        );
    }

    #[test]
    fn type_error_points_at_the_value_line() {
        let text = "# head\n[server]\nport = \"nope\"\n";
        let error = validate_config_text(text).expect_err("type");
        assert_eq!(error.line, 3, "{}", error.message);
        assert_eq!(
            error.message,
            "invalid type: found string \"nope\", expected u16"
        );
    }

    #[test]
    fn stale_revision_does_not_clobber() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let original = "# disk\n[server]\nport = 5000\n";
        write(&path, original);

        let error = save_config_verbatim(&path, "# disk\n[server]\nport = 5002\n", "stale")
            .expect_err("conflict");

        let SaveError::Conflict { text, revision } = error else {
            panic!("expected conflict");
        };
        assert_eq!(text, original);
        assert_eq!(revision, content_revision(original.as_bytes()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(!backup_path(&path).exists());
    }

    #[test]
    fn invalid_file_on_disk_does_not_become_the_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let good = "# good\n[server]\nport = 9\n";
        write(&backup_path(&path), good);
        let bad = "port = [\n";
        write(&path, bad);
        let next = "# fixed\n[server]\nport = 9\n";

        let outcome =
            save_config_verbatim(&path, next, &content_revision(bad.as_bytes())).expect("save");

        assert!(outcome.restart_required);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), next);
        assert_eq!(std::fs::read_to_string(backup_path(&path)).unwrap(), good);
    }
}
