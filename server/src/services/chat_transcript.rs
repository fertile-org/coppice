//! Keep a chat transcript inside the context budget.
//!
//! The person still sees every message. The model sees recent turns in full
//! and a short extract of anything older.

use crate::services::context_budget::{ByteTokenCounter, TokenCounter};

const EARLIER_HEADER: &str = "### Earlier messages\n\n";

/// Join rendered `### Role` blocks, summarizing older ones when they would
/// exceed `max_tokens`. `max_tokens == 0` leaves the transcript unchanged.
pub fn bound_rendered_transcript(blocks: &[String], max_tokens: usize) -> String {
    if blocks.is_empty() {
        return "(No previous messages)".into();
    }
    let counter = ByteTokenCounter;
    let full = blocks.concat();
    if max_tokens == 0 || counter.count(&full) <= max_tokens {
        return full;
    }

    let last = truncate_to_tokens(blocks.last().expect("non-empty"), max_tokens, &counter);
    if blocks.len() == 1 {
        return last;
    }

    // Newest first. Piece counts are not additive (integer division), so each
    // candidate is measured as the joined string.
    let mut kept_rev = vec![last];
    let mut first_kept = blocks.len() - 1;
    for index in (0..blocks.len() - 1).rev() {
        let mut candidate = blocks[index].clone();
        for piece in kept_rev.iter().rev() {
            candidate.push_str(piece);
        }
        let reserve = if index > 0 {
            counter.count(EARLIER_HEADER)
        } else {
            0
        };
        if counter.count(&candidate).saturating_add(reserve) <= max_tokens {
            kept_rev.push(blocks[index].clone());
            first_kept = index;
        } else {
            break;
        }
    }
    kept_rev.reverse();
    let recent = kept_rev.concat();
    if counter.count(&recent) > max_tokens {
        return truncate_to_tokens(&recent, max_tokens, &counter);
    }
    if first_kept == 0 {
        return recent;
    }
    let summary = summarize_blocks(
        &blocks[..first_kept],
        max_tokens.saturating_sub(counter.count(&recent)),
        &counter,
    );
    let mut out = summary;
    out.push_str(&recent);
    if counter.count(&out) > max_tokens {
        return recent;
    }
    out
}

fn summarize_blocks(blocks: &[String], budget: usize, counter: &dyn TokenCounter) -> String {
    if budget == 0 || blocks.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = blocks.iter().map(|block| summary_line(block)).collect();
    loop {
        let mut body = String::from(EARLIER_HEADER);
        for line in &lines {
            body.push_str(line);
            body.push('\n');
        }
        body.push('\n');
        if counter.count(&body) <= budget {
            return body;
        }
        if lines.is_empty() {
            return String::new();
        }
        lines.remove(0);
    }
}

fn summary_line(block: &str) -> String {
    let mut lines = block.lines();
    let header = lines.next().unwrap_or("").trim();
    let role = header.strip_prefix("### ").unwrap_or(header);
    let text = lines
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Attachment");
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let clipped: String = collapsed.chars().take(160).collect();
    format!("- {role}: {clipped}")
}

fn truncate_to_tokens(text: &str, max_tokens: usize, counter: &dyn TokenCounter) -> String {
    if counter.count(text) <= max_tokens {
        return text.to_string();
    }
    let ellipsis = "…";
    if counter.count(ellipsis) > max_tokens {
        return String::new();
    }
    let mut lo = 0usize;
    let mut hi = text.len();
    while lo < hi {
        let mut mid = lo + (hi - lo).div_ceil(2);
        while mid > lo && !text.is_char_boundary(mid) {
            mid -= 1;
        }
        if mid == lo {
            break;
        }
        let head = &text[..mid];
        let candidate = format!("{head}{ellipsis}");
        if counter.count(&candidate) <= max_tokens {
            lo = mid;
        } else {
            hi = mid - 1;
            while hi > lo && !text.is_char_boundary(hi) {
                hi -= 1;
            }
        }
    }
    if lo == 0 {
        return String::new();
    }
    let head = &text[..lo];
    format!("{head}{ellipsis}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_transcript_uses_the_existing_placeholder() {
        assert_eq!(bound_rendered_transcript(&[], 100), "(No previous messages)");
    }

    #[test]
    fn a_short_transcript_is_unchanged() {
        let blocks = vec![
            "### Human\n\nHello\n\n".to_string(),
            "### Agent\n\nHi\n\n".to_string(),
        ];
        let joined = blocks.concat();
        assert_eq!(bound_rendered_transcript(&blocks, 4_000), joined);
        assert_eq!(bound_rendered_transcript(&blocks, 0), joined);
    }

    #[test]
    fn a_long_transcript_stays_within_the_budget_and_keeps_the_latest_turn() {
        let older = format!(
            "### Human\n\nALPHA unique older question\n\n{}",
            "word ".repeat(2_000)
        );
        let latest = "### Human\n\nLATEST please remember this\n\n".to_string();
        let bound = bound_rendered_transcript(&[older, latest], 200);
        assert!(
            ByteTokenCounter.count(&bound) <= 200,
            "tokens {}",
            ByteTokenCounter.count(&bound)
        );
        assert!(bound.contains("LATEST please remember this"));
        assert!(bound.contains("### Earlier messages"));
        assert!(bound.contains("ALPHA unique older question"));
        assert!(!bound.contains("word word word"));
    }

    #[test]
    fn a_single_oversized_turn_is_truncated_to_the_budget() {
        let block = format!("### Human\n\n{}", "abcdef ".repeat(500));
        let bound = bound_rendered_transcript(&[block], 40);
        assert!(ByteTokenCounter.count(&bound) <= 40);
        assert!(bound.ends_with('…'));
    }
}
