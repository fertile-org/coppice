//! Mermaid sketch checks for technical design options.

/// Hard cap on sketch source size (UTF-8 bytes).
pub const MAX_SKETCH_BYTES: usize = 4 * 1024;

/// Reject init / initialize directives (security and config overrides).
const INIT_NEEDLE: &str = "%%{";

/// Allowed diagram openers (first keyword after trim / comments).
const ALLOWED_TYPES: &[&str] = &["flowchart", "graph"];

/// Validate an optional mermaid sketch. Empty / missing is fine.
pub fn validate_sketch(sketch: Option<&str>) -> Result<(), String> {
    let Some(raw) = sketch.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    if raw.len() > MAX_SKETCH_BYTES {
        return Err(format!(
            "sketch exceeds {MAX_SKETCH_BYTES} bytes"
        ));
    }
    let lower = raw.to_ascii_lowercase();
    if lower.contains(INIT_NEEDLE) {
        return Err("sketch must not include mermaid init directives".into());
    }
    // Frontmatter config blocks can also set security-sensitive options.
    if raw.trim_start().starts_with("---") {
        return Err("sketch must not include frontmatter config".into());
    }
    let opener = first_diagram_keyword(raw).ok_or_else(|| {
        "sketch must start with an allowed mermaid diagram type (flowchart or graph)".to_string()
    })?;
    if !ALLOWED_TYPES.contains(&opener.as_str()) {
        return Err(format!(
            "sketch diagram type `{opener}` is not allowed; use flowchart or graph"
        ));
    }
    Ok(())
}

fn first_diagram_keyword(source: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        let word = trimmed
            .split(|c: char| c.is_whitespace() || c == ';')
            .next()
            .unwrap_or("")
            .trim();
        if word.is_empty() {
            continue;
        }
        return Some(word.to_ascii_lowercase());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_flowchart_and_graph() {
        assert!(validate_sketch(Some("flowchart LR\n  A --> B")).is_ok());
        assert!(validate_sketch(Some("graph TD\n  A-->B")).is_ok());
        assert!(validate_sketch(None).is_ok());
        assert!(validate_sketch(Some("  ")).is_ok());
    }

    #[test]
    fn rejects_init_directives_and_other_types() {
        assert!(validate_sketch(Some("%%{init: {'theme':'dark'}}%%\nflowchart LR\nA-->B"))
            .unwrap_err()
            .contains("init"));
        assert!(validate_sketch(Some("sequenceDiagram\nA->>B: hi"))
            .unwrap_err()
            .contains("not allowed"));
        assert!(validate_sketch(Some("---\nconfig:\n  theme: dark\n---\nflowchart LR\nA-->B"))
            .unwrap_err()
            .contains("frontmatter"));
    }

    #[test]
    fn rejects_oversized_sketch() {
        let big = format!("flowchart LR\n{}", "A".repeat(MAX_SKETCH_BYTES));
        assert!(validate_sketch(Some(&big))
            .unwrap_err()
            .contains("exceeds"));
    }
}
