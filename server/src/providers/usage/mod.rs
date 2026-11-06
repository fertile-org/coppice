//! Readers that turn a connector's own output into normalized usage events.
//!
//! A reader is chosen by the connector descriptor's [`UsageFormat`], so every
//! connector that speaks one wire format shares one reader. Adding a connector
//! means writing a reader for its format (if new) and setting `usage` on its
//! descriptor; storage, API and UI need no change.

mod claude_stream_json;

use crate::domain::usage::UsageEvent;
use coppice_connectors::UsageFormat;
use serde_json::Value;

pub use claude_stream_json::ClaudeStreamJsonReader;

/// Reads one output event at a time. Readers may keep state across events
/// (pending requests, cumulative totals).
pub trait UsageReader: Send {
    fn read(&mut self, event: &Value) -> Vec<UsageEvent>;
}

/// The reader for a format, or `None` when Coppice cannot read it yet.
pub fn reader_for(format: UsageFormat) -> Option<Box<dyn UsageReader>> {
    match format {
        UsageFormat::ClaudeStreamJson => Some(Box::new(ClaudeStreamJsonReader::default())),
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
