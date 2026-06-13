/// Truncate `s` to at most `max_len` bytes, appending a single ellipsis character when shortened.
pub fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    if max_len == 0 {
        return String::new();
    }
    let ellipsis = '…';
    let ellipsis_len = ellipsis.len_utf8();
    if max_len <= ellipsis_len {
        return ellipsis.to_string().chars().take(max_len).collect();
    }
    let mut end = max_len - ellipsis_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ellipsis}", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string_adds_ellipsis() {
        let s = "a".repeat(100);
        let out = truncate_with_ellipsis(&s, 20);
        assert_eq!(out.len(), 20);
        assert!(out.ends_with('…'));
    }
}
