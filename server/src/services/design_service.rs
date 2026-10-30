const ALLOWED_EXT: &[&str] = &["html", "css", "png", "jpg", "jpeg", "gif", "webp"];
#[allow(dead_code)]
pub(crate) const MAX_FILE_BYTES: u64 = 512 * 1024;
#[allow(dead_code)]
pub(crate) const MAX_SET_BYTES: u64 = 2 * 1024 * 1024;

pub fn reject_rel_path(path: &str) -> Option<&'static str> {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return Some("path must be relative");
    }
    if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Some("path escapes the design folder");
    }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !ALLOWED_EXT.iter().any(|a| a.eq_ignore_ascii_case(ext)) {
        return Some("file type not allowed");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::reject_rel_path;

    #[test]
    fn reject_rel_path_blocks_escape_and_bad_types() {
        assert_eq!(reject_rel_path("a/index.html"), None);
        assert_eq!(
            reject_rel_path("../x.html"),
            Some("path escapes the design folder")
        );
        assert_eq!(
            reject_rel_path("/tmp/x.html"),
            Some("path must be relative")
        );
        assert_eq!(
            reject_rel_path("x.svg"),
            Some("file type not allowed")
        );
        assert_eq!(
            reject_rel_path("x.exe"),
            Some("file type not allowed")
        );
    }
}
