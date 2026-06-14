/// Encode terminal frame bytes for JSON WebSocket transport.
///
/// ANSI escape sequences contain `0x1b`, which is not valid UTF-8. Using
/// `from_utf8_lossy` replaces those bytes and xterm never sees color codes.
pub fn terminal_bytes_to_ws_string(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len());
    let mut index = 0;
    while index < data.len() {
        let slice = &data[index..];
        match std::str::from_utf8(slice) {
            Ok(text) => {
                out.push_str(text);
                break;
            }
            Err(err) => {
                let valid = err.valid_up_to();
                if valid > 0 {
                    out.push_str(unsafe { std::str::from_utf8_unchecked(&slice[..valid]) });
                    index += valid;
                    continue;
                }
                let byte = slice[0];
                if let Some(ch) = char::from_u32(u32::from(byte)) {
                    out.push(ch);
                }
                index += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_ansi_escape_bytes() {
        let raw = b"\x1b[32mok\x1b[0m\n";
        let encoded = terminal_bytes_to_ws_string(raw);
        assert_eq!(encoded.as_bytes(), raw);
    }

    #[test]
    fn preserves_utf8_text() {
        let raw = "→ Claude session started\n".as_bytes();
        assert_eq!(terminal_bytes_to_ws_string(raw), "→ Claude session started\n");
    }
}
