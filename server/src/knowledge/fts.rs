//! Full-text query construction for knowledge retrieval.
//!
//! Ticket text is long, so terms are OR-joined: AND semantics
//! (`plainto_tsquery` / `websearch_to_tsquery`) would almost never match.
//! Accent folding happens in SQL via `coppice_unaccent`, matching the
//! generated `search_vector` column.

use std::collections::HashSet;

pub const MAX_QUERY_TERMS: usize = 32;
const MIN_TERM_CHARS: usize = 2;
const MAX_TERM_CHARS: usize = 64;

const STOPWORDS: &[&str] = &[
    // English
    "a", "about", "after", "all", "also", "an", "and", "any", "are", "as", "at", "be", "been",
    "before", "but", "by", "can", "could", "do", "does", "for", "from", "has", "have", "how", "if",
    "in", "into", "is", "it", "its", "may", "more", "must", "no", "not", "of", "on", "or", "our",
    "should", "so", "some", "such", "than", "that", "the", "their", "then", "there", "these",
    "they", "this", "to", "up", "use", "was", "we", "were", "what", "when", "which", "will",
    "with", "would", "you", "your", // Vietnamese function words
    "và", "của", "là", "các", "cho", "với", "trong", "được", "có", "không", "này", "một", "những",
    "để", "khi", "thì", "đã", "sẽ", "từ", "theo", "cũng", "như", "nếu", "vào", "ra",
];

/// Build an OR-joined `to_tsquery('simple', …)` expression, title terms first.
/// Returns `None` when no usable terms remain.
pub fn build_tsquery(title: &str, body: &str) -> Option<String> {
    let mut seen = HashSet::new();
    let mut terms = Vec::new();
    for text in [title, body] {
        for raw in text.split(|c: char| !c.is_alphanumeric()) {
            if terms.len() >= MAX_QUERY_TERMS {
                break;
            }
            let term = raw.to_lowercase();
            let chars = term.chars().count();
            if !(MIN_TERM_CHARS..=MAX_TERM_CHARS).contains(&chars)
                || STOPWORDS.contains(&term.as_str())
            {
                continue;
            }
            if seen.insert(term.clone()) {
                terms.push(term);
            }
        }
    }
    (!terms.is_empty()).then(|| terms.join(" | "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_terms_with_or_and_dedupes() {
        assert_eq!(
            build_tsquery("CSRF token", "Null csrf causes 403"),
            Some("csrf | token | null | causes | 403".into())
        );
    }

    #[test]
    fn title_terms_come_first_and_cap_applies() {
        let body = (0..100)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let query = build_tsquery("alpha beta", &body).unwrap();
        let terms: Vec<_> = query.split(" | ").collect();
        assert_eq!(terms.len(), MAX_QUERY_TERMS);
        assert_eq!(&terms[..2], &["alpha", "beta"]);
    }

    #[test]
    fn strips_tsquery_operators_and_punctuation() {
        assert_eq!(
            build_tsquery("a & b | !(c:*) <-> 'd'", "make test-unit"),
            Some("make | test | unit".into())
        );
    }

    #[test]
    fn drops_stopwords_and_short_tokens() {
        assert_eq!(build_tsquery("the and of", "a x"), None);
        assert_eq!(build_tsquery("", "   "), None);
    }

    #[test]
    fn keeps_vietnamese_content_words() {
        assert_eq!(
            build_tsquery("Kiểm thử và triển khai", ""),
            Some("kiểm | thử | triển | khai".into())
        );
    }
}
