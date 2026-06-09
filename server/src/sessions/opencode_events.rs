use crate::providers::AgentRunResult;
use crate::sessions::session_snapshot::SessionSnapshot;
use crate::sessions::TerminalFrame;
use time::OffsetDateTime;

const COPPICE_RUN_PROMPT: &str = "Read .agent/context.md and complete the task described there. \
When finished, reply with ONLY a single JSON object matching the done or blocked contract \
from that file — use real values from your work, not placeholder text.";

pub fn coppice_run_prompt() -> &'static str {
    COPPICE_RUN_PROMPT
}

pub fn session_status_from_sse(event: &serde_json::Value, session_id: &str) -> Option<String> {
    if event.get("type")?.as_str()? != "session.status" {
        return None;
    }
    let props = event.get("properties")?;
    if props.get("sessionID")?.as_str()? != session_id {
        return None;
    }
    props
        .get("status")?
        .get("type")?
        .as_str()
        .map(str::to_string)
}

pub fn extract_result_from_messages(messages: &[serde_json::Value]) -> Option<AgentRunResult> {
    for msg in messages.iter().rev() {
        if msg.get("info")?.get("role")?.as_str()? != "assistant" {
            continue;
        }
        let parts = msg.get("parts")?.as_array()?;
        for part in parts.iter().rev() {
            if part.get("type")?.as_str()? != "text" {
                continue;
            }
            let text = part.get("text")?.as_str()?;
            if let Some(result) = extract_result_from_text(text) {
                if !looks_like_template_contract(&result) {
                    return Some(result);
                }
            }
        }
    }
    None
}

pub fn extract_result_from_snapshot(snapshot: &SessionSnapshot) -> Option<AgentRunResult> {
    extract_result_from_messages(&snapshot.messages_for_extraction())
}

fn looks_like_template_contract(result: &AgentRunResult) -> bool {
    let summary = match result {
        AgentRunResult::Done { summary, .. } | AgentRunResult::Blocked { summary, .. } => summary,
    };
    summary.contains('<') && summary.contains('>')
}

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
            if !looks_like_template_contract(&result) {
                return Some(result);
            }
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

fn try_parse_contract(text: &str) -> Option<AgentRunResult> {
    let result = serde_json::from_str::<AgentRunResult>(text.trim()).ok()?;
    if looks_like_template_contract(&result) {
        None
    } else {
        Some(result)
    }
}

fn extract_json_objects_from_text(text: &str) -> Vec<AgentRunResult> {
    let mut candidates = Vec::new();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    let bytes = text.as_bytes();

    for (i, &byte) in bytes.iter().enumerate() {
        if byte == b'{' {
            if depth == 0 {
                start = Some(i);
            }
            depth += 1;
        } else if byte == b'}' {
            if depth == 0 {
                continue;
            }
            depth -= 1;
            if depth == 0 {
                if let Some(s) = start {
                    if let Some(result) = try_parse_contract(&text[s..=i]) {
                        candidates.push(result);
                    }
                }
                start = None;
            }
        }
    }

    candidates
}

fn extract_result_from_text(text: &str) -> Option<AgentRunResult> {
    if let Some(result) = try_parse_contract(text) {
        return Some(result);
    }

    let concatenated = extract_json_objects_from_text(text);
    if let Some(result) = concatenated.into_iter().last() {
        return Some(result);
    }

    let mut search_from = 0;
    while let Some(start) = text[search_from..].find("```json") {
        let abs_start = search_from + start + 7;
        let after = &text[abs_start..];
        if let Some(end) = after.find("```") {
            let json_str = after[..end].trim();
            if let Some(result) = try_parse_contract(json_str) {
                return Some(result);
            }
            search_from = abs_start + end + 3;
        } else {
            break;
        }
    }

    for line in text.lines().rev() {
        let line = line.trim();
        if line.starts_with('{') {
            if let Some(result) = try_parse_contract(line) {
                return Some(result);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::session_snapshot::SessionSnapshot;
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
    fn extract_result_from_concatenated_duplicate_json() {
        let minimal = r#"{"status":"done","summary":"Done once.","nextStatus":"Done"}"#;
        let text = format!("{minimal}{minimal}{minimal}");
        let messages = vec![serde_json::json!({
            "info": { "role": "assistant" },
            "parts": [{ "type": "text", "text": text }]
        })];
        let result = extract_result_from_messages(&messages).expect("concatenated json");
        match result {
            AgentRunResult::Done { summary, .. } => assert_eq!(summary, "Done once."),
            _ => panic!("expected done"),
        }
    }

    #[test]
    fn extract_result_from_minimal_done_json() {
        let minimal = r#"{"status":"done","summary":"Test complete.","nextStatus":"In Review"}"#;
        let messages = vec![serde_json::json!({
            "info": { "role": "assistant" },
            "parts": [{ "type": "text", "text": minimal }]
        })];
        let result = extract_result_from_messages(&messages).expect("minimal done should parse");
        match result {
            AgentRunResult::Done {
                summary,
                next_status,
                changed_files,
                tests_run,
                mention_agents,
                blockers,
            } => {
                assert_eq!(summary, "Test complete.");
                assert_eq!(next_status, "In Review");
                assert!(changed_files.is_empty());
                assert!(tests_run.is_empty());
                assert!(mention_agents.is_empty());
                assert!(blockers.is_empty());
            }
            _ => panic!("expected done"),
        }
    }

    #[test]
    fn extract_result_from_messages_skips_template_codeblock() {
        let template = r#"{"status":"done","summary":"<markdown summary>","changedFiles":["<paths>"],"testsRun":[],"nextStatus":"In Review","mentionAgents":[],"blockers":[]}"#;
        let real = r#"{"status":"done","summary":"Research complete.","changedFiles":[],"testsRun":[],"nextStatus":"In Review","mentionAgents":[],"blockers":[]}"#;
        let messages = vec![serde_json::json!({
            "info": { "role": "assistant" },
            "parts": [{
                "type": "text",
                "text": format!("Templates:\n```json\n{template}\n```\n\nResult:\n{real}")
            }]
        })];
        let result = extract_result_from_messages(&messages).expect("result");
        match result {
            AgentRunResult::Done { summary, .. } => assert_eq!(summary, "Research complete."),
            _ => panic!("expected done"),
        }
    }

    #[test]
    fn session_status_from_sse_detects_idle() {
        let event = serde_json::json!({
            "type": "session.status",
            "properties": {
                "sessionID": "ses_test",
                "status": { "type": "idle" }
            }
        });
        assert_eq!(session_status_from_sse(&event, "ses_test"), Some("idle".into()));
    }

    #[test]
    fn extract_result_from_snapshot_helper() {
        let mut snap = SessionSnapshot::empty("ses_1");
        snap.messages.push(serde_json::json!({
            "id": "msg_1",
            "info": { "role": "assistant", "id": "msg_1" }
        }));
        snap.parts.insert(
            "msg_1".into(),
            vec![serde_json::json!({
                "type": "text",
                "text": r#"{"status":"done","summary":"Snap.","nextStatus":"In Review"}"#
            })],
        );
        let result = extract_result_from_snapshot(&snap).expect("snapshot extract");
        match result {
            AgentRunResult::Done { summary, .. } => assert_eq!(summary, "Snap."),
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
