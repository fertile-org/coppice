//! Follow `current.jsonl` from the end and return lines written after that.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub struct FileTail {
    dir: PathBuf,
    target: Option<PathBuf>,
    offset: u64,
    pending: Vec<u8>,
}

impl FileTail {
    /// Start at the end of the file `current.jsonl` points at, so history is
    /// not replayed. The page loads history with `GET /api/logs`.
    pub fn start(dir: &Path) -> Self {
        let mut tail = Self {
            dir: dir.to_path_buf(),
            target: None,
            offset: 0,
            pending: Vec::new(),
        };
        if let Some(path) = tail.current_target() {
            if let Ok(meta) = fs::metadata(&path) {
                tail.offset = meta.len();
                tail.target = Some(path);
            }
        }
        tail
    }

    /// Complete lines appended since the previous poll.
    pub fn poll_lines(&mut self) -> io::Result<Vec<String>> {
        let Some(path) = self.current_target() else {
            return Ok(Vec::new());
        };
        if self.target.as_ref() != Some(&path) {
            if self.target.is_none() {
                let len = fs::metadata(&path)?.len();
                self.target = Some(path);
                self.offset = len;
                return Ok(Vec::new());
            }
            self.target = Some(path);
            self.offset = 0;
            self.pending.clear();
        }
        let path = self
            .target
            .clone()
            .ok_or_else(|| io::Error::other("log tail has no file"))?;
        let mut file = File::open(&path)?;
        let len = file.metadata()?.len();
        if self.offset > len {
            self.offset = 0;
            self.pending.clear();
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        self.offset += u64::try_from(buf.len()).unwrap_or(u64::MAX);
        self.pending.extend(buf);
        Ok(self.drain_lines())
    }

    fn current_target(&self) -> Option<PathBuf> {
        let current = self.dir.join("current.jsonl");
        let meta = fs::symlink_metadata(&current).ok()?;
        let path = if meta.file_type().is_symlink() {
            let link = fs::read_link(&current).ok()?;
            if link.is_absolute() {
                link
            } else {
                self.dir.join(link)
            }
        } else if meta.is_file() {
            current
        } else {
            return None;
        };
        fs::canonicalize(&path).ok().or(Some(path))
    }

    fn drain_lines(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Some(pos) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.pending.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                continue;
            }
            lines.push(String::from_utf8_lossy(&line).into_owned());
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link_current(dir: &Path, target_name: &str) {
        let link = dir.join("current.jsonl");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(target_name, &link).expect("symlink");
    }

    #[test]
    fn tail_reads_only_bytes_appended_after_start() {
        let dir = tempfile::tempdir().expect("tempdir");
        let day = dir.path().join("coppice.2026-10-14.jsonl");
        fs::write(&day, "old line\n").expect("write");
        link_current(dir.path(), "coppice.2026-10-14.jsonl");

        let mut tail = FileTail::start(dir.path());
        assert!(tail.poll_lines().expect("poll").is_empty());

        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&day)
            .expect("open");
        writeln!(file, "new line").expect("append");
        drop(file);

        assert_eq!(
            tail.poll_lines().expect("poll"),
            vec!["new line".to_string()]
        );
        assert!(tail.poll_lines().expect("poll").is_empty());
    }

    #[test]
    fn tail_waits_for_a_newline_and_follows_a_new_day() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = dir.path().join("coppice.2026-10-14.jsonl");
        fs::write(&first, "kept\n").expect("write");
        link_current(dir.path(), "coppice.2026-10-14.jsonl");
        let mut tail = FileTail::start(dir.path());

        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&first)
            .expect("open");
        write!(file, "partial").expect("partial");
        drop(file);
        assert!(tail.poll_lines().expect("poll").is_empty());

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&first)
            .expect("open");
        writeln!(file).expect("newline");
        drop(file);
        assert_eq!(
            tail.poll_lines().expect("poll"),
            vec!["partial".to_string()]
        );

        let second = dir.path().join("coppice.2026-10-15.jsonl");
        fs::write(&second, "next day\n").expect("write");
        link_current(dir.path(), "coppice.2026-10-15.jsonl");
        assert_eq!(
            tail.poll_lines().expect("poll"),
            vec!["next day".to_string()]
        );
    }
}
