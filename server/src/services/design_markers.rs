//! Shared `{{design:<id>}}` markers in plan markdown.
//! Pattern matches the web helper: UUID or 12-char `[0-9a-z]`.
use crate::domain::Id;

const PREFIX: &str = "{{design:";
const SUFFIX: &str = "}}";

fn is_marker_id(id: &str) -> bool {
    Id::parse_str(id).is_ok()
}

/// Walk `markdown`, calling `on_marker` for each well-formed marker. Returns text
/// with every marker (valid or not-well-formed prefix that looks like one) removed
/// when `strip` is true; otherwise returns the original string unchanged at the end.
fn for_each_marker<'a>(markdown: &'a str, mut on_marker: impl FnMut(usize, usize, &'a str)) {
    let mut start = 0;
    while start < markdown.len() {
        let Some(rel) = markdown[start..].find(PREFIX) else {
            break;
        };
        let marker_start = start + rel;
        let id_start = marker_start + PREFIX.len();
        let Some(suffix_rel) = markdown[id_start..].find(SUFFIX) else {
            break;
        };
        let id_end = id_start + suffix_rel;
        let id = &markdown[id_start..id_end];
        if is_marker_id(id) {
            let end = id_end + SUFFIX.len();
            on_marker(marker_start, end, id);
            start = end;
        } else {
            start = id_start;
        }
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
        if let Ok(parsed) = Id::parse_str(id) {
            ids.push(parsed);
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
    fn strips_short_id_markers() {
        let id = Id::parse_str("abc123xyz789").unwrap();
        let md = format!("before {{{{design:{id}}}}} after");
        assert_eq!(strip_design_markers(&md), "before  after");
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

    #[test]
    fn design_marker_ids_survive_ellipsis_and_arrows() {
        let id = Id::parse_str("abc123xyz789").unwrap();
        let md = format!("Scope…\n\nPick {{{{design:{id}}}}} → done.");
        assert_eq!(design_marker_ids(&md), vec![id]);
    }

    #[test]
    fn strip_design_markers_unicode_surrounding() {
        let id = Id::parse_str("abc123xyz789").unwrap();
        let md = format!("日本語…{{{{design:{id}}}}}→終");
        let out = strip_design_markers(&md);
        assert_eq!(out, "日本語…→終");
    }

    #[test]
    fn design_marker_ids_marker_at_start_and_end() {
        let id = Id::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let md = format!("{{{{design:{id}}}}} middle … {{{{design:{id}}}}}");
        assert_eq!(design_marker_ids(&md), vec![id, id]);
    }

    #[test]
    fn design_marker_ids_unterminated_ignored() {
        let id = Id::parse_str("abc123xyz789").unwrap();
        let md = format!("ok {{{{design:{id}}}}} bad {{design:open");
        assert_eq!(design_marker_ids(&md), vec![id]);
    }

    #[test]
    fn design_marker_ids_invalid_ids_ignored() {
        let good = Id::parse_str("abc123xyz789").unwrap();
        let md = format!("{{{{design:not-valid}}}} {{{{design:{good}}}}}");
        assert_eq!(design_marker_ids(&md), vec![good]);
    }

    #[test]
    fn never_panics_on_arbitrary_markdown() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        let id = Id::parse_str("abc123xyz789").unwrap();
        let corpus: Vec<String> = vec![
            String::new(),
            "{{design:".to_string(),
            format!("…{{{{design:{id}}}}}…"),
            "日本語日本語日本語".to_string(),
            format!("{{{{design:{id}}}}}{{{{design:{id}}}}}"),
            format!("Scope…\n\nPick {{{{design:{id}}}}} → done."),
        ];
        for md in &corpus {
            let md = md.as_str();
            assert!(catch_unwind(AssertUnwindSafe(|| design_marker_ids(md))).is_ok());
            assert!(catch_unwind(AssertUnwindSafe(|| strip_design_markers(md))).is_ok());
            assert!(catch_unwind(AssertUnwindSafe(|| section_for_set(md, id))).is_ok());
        }
    }
}
