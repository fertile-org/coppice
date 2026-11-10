//! Readers that turn a connector's own output into normalized usage events.
//!
//! A reader is chosen by the connector descriptor's [`UsageFormat`], so every
//! connector that speaks one wire format shares one reader. Adding a connector
//! means writing a reader for its format (if new) and setting `usage` on its
//! descriptor; storage, API and UI need no change.

mod claude_stream_json;
mod cursor_stream_json;

use crate::domain::usage::UsageEvent;
use coppice_connectors::UsageFormat;
use serde_json::Value;

pub use claude_stream_json::ClaudeStreamJsonReader;
pub use cursor_stream_json::CursorStreamJsonReader;

/// What a reader knows about the CLI process whose output follows. New facts
/// a format needs become fields here, so the reader trait keeps its shape.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessContext {
    /// The process continues an earlier session, whose usage some CLIs fold
    /// into their totals.
    pub resumed: bool,
    /// The CLI version detected at launch, when known. Formats change
    /// between versions (Claude Code's resumed totals do).
    pub cli_version: Option<String>,
}

impl ProcessContext {
    /// A process that resumes `resume_session_id` when one is given.
    pub fn new(resume_session_id: Option<&str>, cli_version: Option<&str>) -> Self {
        Self {
            resumed: resume_session_id.is_some_and(|sid| !sid.is_empty()),
            cli_version: cli_version.map(str::to_string),
        }
    }
}

/// Reads one output event at a time. Readers may keep state across events
/// (pending requests, cumulative totals).
pub trait UsageReader: Send {
    /// A CLI process starts writing to this run. One run can start several
    /// (connector-check phases, retries).
    fn start_process(&mut self, _process: &ProcessContext) -> Vec<UsageEvent> {
        Vec::new()
    }

    fn read(&mut self, event: &Value) -> Vec<UsageEvent>;
}

/// The reader for a format, or `None` when Coppice cannot read it yet.
pub fn reader_for(format: UsageFormat) -> Option<Box<dyn UsageReader>> {
    match format {
        UsageFormat::ClaudeStreamJson => Some(Box::new(ClaudeStreamJsonReader::default())),
        UsageFormat::CursorStreamJson => Some(Box::new(CursorStreamJsonReader::default())),
        UsageFormat::Normalized => Some(Box::new(NormalizedReader)),
        UsageFormat::None => None,
    }
}

/// Events already shaped as [`UsageEvent`] JSON. Anything else is ignored.
struct NormalizedReader;

impl UsageReader for NormalizedReader {
    fn read(&mut self, event: &Value) -> Vec<UsageEvent> {
        serde_json::from_value(event.clone()).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn process_context_reads_resume_and_version() {
        let fresh = ProcessContext::new(None, Some("2.1.318"));
        assert!(!fresh.resumed);
        assert_eq!(fresh.cli_version.as_deref(), Some("2.1.318"));
        assert!(!ProcessContext::new(Some(""), None).resumed);
        assert!(ProcessContext::new(Some("sid"), None).resumed);
    }

    #[test]
    fn none_format_has_no_reader() {
        assert!(reader_for(UsageFormat::None).is_none());
    }

    #[test]
    fn normalized_reader_skips_foreign_events() {
        let mut reader = reader_for(UsageFormat::Normalized).unwrap();
        assert!(reader.read(&json!({ "type": "assistant" })).is_empty());
        assert_eq!(
            reader
                .read(&json!({ "kind": "runTotal", "tokens": { "output": 3 } }))
                .len(),
            1
        );
    }
}
