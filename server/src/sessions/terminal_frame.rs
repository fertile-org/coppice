use serde_json::json;
use time::OffsetDateTime;

use crate::sessions::terminal_encoding::terminal_bytes_to_ws_string;

#[derive(Debug, Clone)]
pub struct TerminalFrame {
    pub seq: u64,
    pub data: Vec<u8>,
    pub ts: OffsetDateTime,
}

impl TerminalFrame {
    pub fn to_ws_json(&self) -> serde_json::Value {
        json!({
            "type": "frame",
            "seq": self.seq,
            "data": terminal_bytes_to_ws_string(&self.data),
        })
    }

    pub fn end_message(status: &str) -> serde_json::Value {
        json!({ "type": "end", "status": status })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_message_roundtrip() {
        let frame = TerminalFrame {
            seq: 1,
            data: b"Mock agent starting...\n".to_vec(),
            ts: OffsetDateTime::now_utc(),
        };
        let json = frame.to_ws_json();
        assert_eq!(json["type"], "frame");
        assert_eq!(json["seq"], 1);
        assert!(json["data"].as_str().unwrap().contains("Mock agent"));
    }
}
