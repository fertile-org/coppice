use crate::providers::AgentRunResult;
use crate::sessions::TerminalFrame;
use time::OffsetDateTime;

pub fn event_line_to_frame(seq: u64, line: &str) -> Option<TerminalFrame> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let text = value
        .get("text")
        .or_else(|| value.get("content"))
        .and_then(|v| v.as_str())?;
    Some(TerminalFrame {
        seq,
        data: format!("{text}\n").into_bytes(),
        ts: OffsetDateTime::now_utc(),
    })
}

pub fn extract_result_from_events(lines: &[String]) -> Option<AgentRunResult> {
    for line in lines.iter().rev() {
        if let Ok(result) = serde_json::from_str::<AgentRunResult>(line) {
            return Some(result);
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            let text = value
                .get("text")
                .or_else(|| value.get("content"))
                .and_then(|v| v.as_str());
            if let Some(text) = text {
                if let Some(result) = extract_result_from_text(text) {
                    return Some(result);
                }
            }
        }
    }
    None
}

fn extract_result_from_text(text: &str) -> Option<AgentRunResult> {
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            let json_str = after[..end].trim();
            if let Ok(result) = serde_json::from_str::<AgentRunResult>(json_str) {
                return Some(result);
            }
        }
    }
    serde_json::from_str(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/opencode-events")
    }

    #[test]
    fn event_line_to_frame_extracts_text_field() {
        let line = r#"{"type":"text","text":"Starting agent..."}"#;
        let frame = event_line_to_frame(0, line).expect("frame");
        assert_eq!(frame.seq, 0);
        assert_eq!(String::from_utf8_lossy(&frame.data), "Starting agent...\n");
    }

    #[test]
    fn event_line_to_frame_extracts_content_field() {
        let line = r#"{"type":"message","content":"Reading context.md"}"#;
        let frame = event_line_to_frame(1, line).expect("frame");
        assert_eq!(String::from_utf8_lossy(&frame.data), "Reading context.md\n");
    }

    #[test]
    fn event_line_to_frame_ignores_non_display_lines() {
        assert!(event_line_to_frame(0, r#"{"type":"tool","name":"read"}"#).is_none());
        assert!(event_line_to_frame(0, "not json").is_none());
    }

    #[test]
    fn extract_result_from_direct_json_line() {
        let lines = vec![
            r#"{"type":"text","text":"working"}"#.into(),
            r#"{"status":"done","summary":"All good.","changedFiles":[],"testsRun":[],"nextStatus":"In Review","mentionAgents":[],"blockers":[]}"#.into(),
        ];
        let result = extract_result_from_events(&lines).expect("result");
        match result {
            AgentRunResult::Done { summary, .. } => assert_eq!(summary, "All good."),
            _ => panic!("expected done"),
        }
    }

    #[test]
    fn extract_result_from_json_codeblock() {
        let contract = r#"{"status":"done","summary":"From code block.","changedFiles":[],"testsRun":[],"nextStatus":"In Review","mentionAgents":[],"blockers":[]}"#;
        let text = format!("```json\n{contract}\n```");
        let line = serde_json::json!({
            "type": "text",
            "text": text,
        })
        .to_string();
        let lines = vec![line];
        let result = extract_result_from_events(&lines).expect("result");
        match result {
            AgentRunResult::Done { summary, .. } => assert_eq!(summary, "From code block."),
            _ => panic!("expected done"),
        }
    }

    #[test]
    fn sample_jsonl_fixture_parses_frames_and_result() {
        let path = fixtures_root().join("sample.jsonl");
        let raw = std::fs::read_to_string(path).expect("read sample.jsonl");
        let lines: Vec<String> = raw.lines().map(str::to_string).collect();
        assert!(lines.len() >= 3);

        let mut frame_count = 0;
        for (i, line) in lines.iter().enumerate() {
            if event_line_to_frame(i as u64, line).is_some() {
                frame_count += 1;
            }
        }
        assert!(frame_count >= 2);

        let result = extract_result_from_events(&lines).expect("result from sample");
        match result {
            AgentRunResult::Done { summary, .. } => {
                assert_eq!(summary, "OpenCode run complete.");
            }
            _ => panic!("expected done"),
        }
    }
}
