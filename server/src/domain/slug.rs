/// Fold Latin letters with diacritics (Vietnamese first) to ASCII before slugify.
fn fold_latin(ch: char) -> char {
    match ch {
        'à' | 'á' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ằ' | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' | 'â' | 'ầ' | 'ấ' | 'ẩ'
        | 'ẫ' | 'ậ' | 'À' | 'Á' | 'Ả' | 'Ã' | 'Ạ' | 'Ă' | 'Ằ' | 'Ắ' | 'Ẳ' | 'Ẵ' | 'Ặ' | 'Â'
        | 'Ầ' | 'Ấ' | 'Ẩ' | 'Ẫ' | 'Ậ' => 'a',
        'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ề' | 'ế' | 'ể' | 'ễ' | 'ệ' | 'È' | 'É' | 'Ẻ'
        | 'Ẽ' | 'Ẹ' | 'Ê' | 'Ề' | 'Ế' | 'Ể' | 'Ễ' | 'Ệ' => 'e',
        'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' | 'Ì' | 'Í' | 'Ỉ' | 'Ĩ' | 'Ị' => 'i',
        'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ồ' | 'ố' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ờ' | 'ớ'
        | 'ở' | 'ỡ' | 'ợ' | 'Ò' | 'Ó' | 'Ỏ' | 'Õ' | 'Ọ' | 'Ô' | 'Ồ' | 'Ố' | 'Ổ' | 'Ỗ' | 'Ộ'
        | 'Ơ' | 'Ờ' | 'Ớ' | 'Ở' | 'Ỡ' | 'Ợ' => 'o',
        'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' | 'Ù' | 'Ú' | 'Ủ'
        | 'Ũ' | 'Ụ' | 'Ư' | 'Ừ' | 'Ứ' | 'Ử' | 'Ữ' | 'Ự' => 'u',
        'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' | 'Ỳ' | 'Ý' | 'Ỷ' | 'Ỹ' | 'Ỵ' => 'y',
        'đ' | 'Đ' => 'd',
        other => other,
    }
}

pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_hyphen = false;
    for ch in input.chars() {
        let folded = fold_latin(ch);
        let lower = folded.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_hyphen = false;
        } else if !prev_hyphen {
            out.push('-');
            prev_hyphen = true;
        }
    }
    out.trim_matches(['-', '.']).to_string()
}

/// Like [`slugify`], then truncated to `max_len` on a char boundary (no trailing `-`/`.`).
pub fn slugify_truncated(input: &str, max_len: usize) -> String {
    let slug = slugify(input);
    if slug.len() <= max_len {
        return slug;
    }
    let mut end = max_len;
    while end > 0 && !slug.is_char_boundary(end) {
        end -= 1;
    }
    slug[..end].trim_end_matches(['-', '.']).to_string()
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

    #[test]
    fn slugify_vietnamese_including_d() {
        assert_eq!(slugify("Đăng nhập lại"), "dang-nhap-lai");
        assert_eq!(slugify("Sửa lỗi"), "sua-loi");
    }

    #[test]
    fn slugify_truncated_trims_edge() {
        assert_eq!(slugify_truncated("Fix login redirect please now", 10), "fix-login");
    }
}
