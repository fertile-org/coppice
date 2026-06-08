pub fn slugify(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut out = String::new();
    let mut prev_hyphen = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_hyphen = false;
        } else if !prev_hyphen {
            out.push('-');
            prev_hyphen = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_agent_name() {
        assert_eq!(slugify("Frontend Engineer"), "frontend-engineer");
    }

    #[test]
    fn slugify_collapses_separators() {
        assert_eq!(slugify("foo---bar"), "foo-bar");
    }
}
