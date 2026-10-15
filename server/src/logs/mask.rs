//! Write-time replacement of secrets Coppice placed on a child process.

use std::io::{self, Write};
use std::sync::{Arc, Mutex, OnceLock};

/// Same floor as plugin output redaction: shorter values are ordinary text.
const MIN_SECRET_LEN: usize = 6;

struct Entry {
    id: u64,
    name: String,
    value: String,
}

struct Registry {
    next: u64,
    entries: Vec<Entry>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(Registry {
            next: 1,
            entries: Vec::new(),
        })
    })
}

struct GuardInner {
    ids: Vec<u64>,
}

impl Drop for GuardInner {
    fn drop(&mut self) {
        if self.ids.is_empty() {
            return;
        }
        let mut slot = registry().lock().unwrap_or_else(|err| err.into_inner());
        slot.entries.retain(|entry| !self.ids.contains(&entry.id));
    }
}

/// Keeps the registered values on the process list until the last clone is dropped.
/// Hold it until the child's pipes have been drained.
#[derive(Clone)]
#[must_use = "dropping the guard unregisters the secret"]
pub struct SecretGuard {
    _inner: Arc<GuardInner>,
}

impl SecretGuard {
    pub fn empty() -> Self {
        Self::register(std::iter::empty::<(String, String)>())
    }

    /// Skip empty values. Values shorter than 6 bytes stay on the list and are
    /// left unchanged when a line is written.
    pub fn register<I, N, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (N, V)>,
        N: Into<String>,
        V: Into<String>,
    {
        let mut ids = Vec::new();
        {
            let mut slot = registry().lock().unwrap_or_else(|err| err.into_inner());
            for (name, value) in pairs {
                let value = value.into();
                if value.is_empty() {
                    continue;
                }
                let id = slot.next;
                slot.next = slot.next.wrapping_add(1);
                slot.entries.push(Entry {
                    id,
                    name: name.into(),
                    value,
                });
                ids.push(id);
            }
        }
        Self {
            _inner: Arc::new(GuardInner { ids }),
        }
    }
}

fn snapshot() -> Vec<(String, String)> {
    let slot = registry().lock().unwrap_or_else(|err| err.into_inner());
    let mut pairs: Vec<(String, String)> = slot
        .entries
        .iter()
        .map(|entry| (entry.name.clone(), entry.value.clone()))
        .collect();
    pairs.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    pairs
}

pub(crate) fn mask_bytes(input: &[u8]) -> Vec<u8> {
    let mut out = input.to_vec();
    for (name, value) in snapshot() {
        if value.len() < MIN_SECRET_LEN {
            continue;
        }
        let needle = value.as_bytes();
        if !out.windows(needle.len()).any(|window| window == needle) {
            continue;
        }
        let replacement = format!("\u{2039}{name}\u{203A}").into_bytes();
        out = replace_all(&out, needle, &replacement);
    }
    out
}

fn replace_all(haystack: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    if needle.is_empty() {
        return haystack.to_vec();
    }
    let mut out = Vec::with_capacity(haystack.len());
    let mut index = 0;
    while index < haystack.len() {
        if haystack[index..].starts_with(needle) {
            out.extend_from_slice(replacement);
            index += needle.len();
        } else {
            out.push(haystack[index]);
            index += 1;
        }
    }
    out
}

/// Masks each complete line, then flushes it.
pub struct MaskingWriter<W: Write> {
    inner: W,
    buf: Vec<u8>,
}

impl<W: Write> MaskingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            buf: Vec::new(),
        }
    }

    fn drain_lines(&mut self) -> io::Result<()>
    where
        W: Write,
    {
        loop {
            let Some(pos) = self.buf.iter().position(|byte| *byte == b'\n') else {
                return Ok(());
            };
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            let (body, newline) = line.split_at(line.len() - 1);
            self.inner.write_all(&mask_bytes(body))?;
            self.inner.write_all(newline)?;
            self.inner.flush()?;
        }
    }

    fn flush_rest(&mut self) -> io::Result<()>
    where
        W: Write,
    {
        if !self.buf.is_empty() {
            let masked = mask_bytes(&self.buf);
            self.buf.clear();
            self.inner.write_all(&masked)?;
        }
        self.inner.flush()
    }
}

impl<W: Write> Write for MaskingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(buf);
        self.drain_lines()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.drain_lines()?;
        self.flush_rest()
    }
}

impl<W: Write> Drop for MaskingWriter<W> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_value_is_replaced_before_a_shorter_prefix() {
        let _guard = SecretGuard::register([("SHORT", "zq9abc"), ("LONG", "zq9abcdef")]);
        assert_eq!(mask_bytes(b"zq9abcdef"), "\u{2039}LONG\u{203A}".as_bytes());
    }

    #[test]
    fn five_byte_value_is_left_unchanged() {
        let _guard = SecretGuard::register([("TINY", "abcde")]);
        assert_eq!(mask_bytes(b"abcde"), b"abcde");
    }
}
