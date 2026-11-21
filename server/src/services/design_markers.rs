//! Shared `{{design:<uuid>}}` markers in plan markdown.
//! Pattern matches the web helper: `\{\{design:([0-9a-f-]{36})\}\}`.
use crate::domain::Id;

const PREFIX: &str = "{{design:";
const SUFFIX: &str = "}}";
const ID_LEN: usize = 36;

fn is_marker_id(id: &str) -> bool {
    id.len() == ID_LEN
        && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'-'))
        && Id::parse_str(id).is_ok()
}

/// Walk `markdown`, calling `on_marker` for each well-formed marker. Returns text
/// with every marker (valid or not-well-formed prefix that looks like one) removed
/// when `strip` is true; otherwise returns the original string unchanged at the end.
fn for_each_marker<'a>(markdown: &'a str, mut on_marker: impl FnMut(usize, usize, &'a str)) {
    let bytes = markdown.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if markdown[i..].starts_with(PREFIX) {
            let id_start = i + PREFIX.len();
            let id_end = id_start + ID_LEN;
            if id_end + SUFFIX.len() <= markdown.len()
                && markdown[id_end..].starts_with(SUFFIX)
                && is_marker_id(&markdown[id_start..id_end])
            {
                let end = id_end + SUFFIX.len();
                on_marker(i, end, &markdown[id_start..id_end]);
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

/// Remove every design marker from markdown (approved description, MCP output).
pub fn strip_design_markers(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut last = 0;
    for_each_marker(markdown, |start, end, _| {
        out.push_str(&markdown[last..start]);
        last = end;
    });
    out.push_str(&markdown[last..]);
    collapse_blank_lines(&out)
}

/// Each marker's set id in document order.
pub fn design_marker_ids(markdown: &str) -> Vec<Id> {
    let mut ids = Vec::new();
    for_each_marker(markdown, |_, _, id| {
        if let Ok(uuid) = Id::parse_str(id) {
            ids.push(uuid);
        }
    });
    ids
}

/// Nearest preceding `##` heading above the first marker for `set_id`.
pub fn section_for_set(markdown: &str, set_id: Id) -> Option<String> {
    let needle = format!("{{{{design:{set_id}}}}}");
    let pos = markdown.find(&needle)?;
    let before = &markdown[..pos];
    let mut heading = None;
    for line in before.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("## ") {
            if !rest.starts_with('#') {
                heading = Some(rest.trim().to_string());
            }
        }
    }
    heading
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank <= 2 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(line);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markers_and_keeps_prose() {
        let id = Id::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let md = format!("## UI design\n\nPick here.\n\n{{{{design:{id}}}}}\n\n### Steps\n- [ ] Ship");
        let out = strip_design_markers(&md);
        assert!(!out.contains("{{design:"));
        assert!(out.contains("## UI design"));
        assert!(out.contains("### Steps"));
    }

    #[test]
    fn section_reads_nearest_h2() {
        let id = Id::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let md = format!("## Approach\nGo.\n\n## UI design\n\n{{{{design:{id}}}}}\n\n## Steps\nDone.");
        assert_eq!(section_for_set(&md, id).as_deref(), Some("UI design"));
    }

    #[test]
    fn ignores_malformed_markers() {
        assert_eq!(
            strip_design_markers("keep {{design:not-a-uuid}} text"),
            "keep {{design:not-a-uuid}} text"
        );
    }
}
