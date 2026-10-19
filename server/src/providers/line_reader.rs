//! Line reading for agent CLI output with an upper bound on one line.
//!
//! One event can carry a whole conversation (a resumed session, a large tool
//! result), so lines are allowed to be large, but not unbounded: a CLI that
//! writes one endless line would otherwise grow memory until the process dies.

use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// Longest stdout line accepted from an agent CLI.
pub const MAX_LINE_BYTES: usize = 32 * 1024 * 1024;

pub fn line_too_long(limit: usize) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!(
            "the agent CLI wrote one output line longer than {} MiB",
            (limit / (1024 * 1024)).max(1)
        ),
    )
}

/// Lines without their trailing `\n` or `\r\n`. A partial line is kept on the
/// struct between calls, so [`CappedLines::next_line`] can be raced in
/// `tokio::select!` like `tokio::io::Lines::next_line`.
pub struct CappedLines<R> {
    reader: R,
    buf: Vec<u8>,
    max: usize,
}

impl<R: AsyncBufRead + Unpin> CappedLines<R> {
    pub fn new(reader: R, max: usize) -> Self {
        Self {
            reader,
            buf: Vec::new(),
            max,
        }
    }

    /// `Ok(None)` at end of stream. A line longer than the cap is an
    /// `InvalidData` error; stop reading after it.
    pub async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        loop {
            // The only await. Nothing is consumed until it returns, so a
            // dropped future loses no bytes.
            let available = self.reader.fill_buf().await?;
            if available.is_empty() {
                if self.buf.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(self.take_line()));
            }
            if let Some(pos) = available.iter().position(|&b| b == b'\n') {
                if self.buf.len() + pos > self.max {
                    self.buf.clear();
                    return Err(line_too_long(self.max));
                }
                self.buf.extend_from_slice(&available[..pos]);
                self.reader.consume(pos + 1);
                return Ok(Some(self.take_line()));
            }
            let len = available.len();
            if self.buf.len() + len > self.max {
                self.buf.clear();
                return Err(line_too_long(self.max));
            }
            self.buf.extend_from_slice(available);
            self.reader.consume(len);
        }
    }

    fn take_line(&mut self) -> String {
        if self.buf.last() == Some(&b'\r') {
            self.buf.pop();
        }
        let line = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn lines(input: &[u8], max: usize) -> Vec<std::io::Result<Option<String>>> {
        let mut reader = CappedLines::new(tokio::io::BufReader::with_capacity(4, input), max);
        let mut out = Vec::new();
        loop {
            let next = reader.next_line().await;
            let stop = !matches!(next, Ok(Some(_)));
            out.push(next);
            if stop {
                return out;
            }
        }
    }

    #[tokio::test]
    async fn reads_lines_across_small_buffers_and_strips_crlf() {
        let got = lines(b"first\r\nsecond line\nlast", 64).await;
        let got: Vec<_> = got.into_iter().map(|r| r.unwrap()).collect();
        assert_eq!(
            got,
            [
                Some("first".to_string()),
                Some("second line".to_string()),
                Some("last".to_string()),
                None
            ]
        );
    }

    #[tokio::test]
    async fn a_line_past_the_cap_is_an_error() {
        let got = lines(b"ok\n0123456789abcdef\nnext\n", 8).await;
        assert_eq!(got[0].as_ref().unwrap().as_deref(), Some("ok"));
        let err = got[1].as_ref().expect_err("too long");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(got.len(), 2);
    }

    #[tokio::test]
    async fn a_line_exactly_at_the_cap_is_kept() {
        let got = lines(b"12345678\n", 8).await;
        assert_eq!(got[0].as_ref().unwrap().as_deref(), Some("12345678"));
    }

    #[tokio::test]
    async fn a_multibyte_character_split_across_reads_decodes_whole() {
        let got = lines("aé€\n".as_bytes(), 64).await;
        assert_eq!(got[0].as_ref().unwrap().as_deref(), Some("aé€"));
    }
}
