//! Append-only console and terminal files for one run.
//!
//! Every console event and terminal frame is written as it is published, so a
//! long run keeps its whole console, not only the live replay buffer. Each
//! file is capped by bytes: the active file rolls over into numbered segments
//! (`console.events.1.jsonl`, `console.events.2.jsonl`, …) and the oldest
//! segment is deleted once the total passes the cap.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const CONSOLE_STEM: &str = "console.events";
pub const CONSOLE_EXT: &str = "jsonl";
pub const TERMINAL_STEM: &str = "terminal";
pub const TERMINAL_EXT: &str = "log";

/// How many segments a capped file is split into.
const SEGMENTS_PER_CAP: u64 = 4;

/// A file capped at `max_bytes`, kept as an active file plus numbered segments.
pub struct RotatingFile {
    dir: PathBuf,
    stem: &'static str,
    ext: &'static str,
    max_bytes: u64,
    active: Option<File>,
    active_len: u64,
}

impl RotatingFile {
    pub fn new(dir: &Path, stem: &'static str, ext: &'static str, max_bytes: u64) -> Self {
        Self {
            dir: dir.to_path_buf(),
            stem,
            ext,
            max_bytes: max_bytes.max(1),
            active: None,
            active_len: 0,
        }
    }

    fn segment_max(&self) -> u64 {
        (self.max_bytes / SEGMENTS_PER_CAP).max(1)
    }

    fn active_path(&self) -> PathBuf {
        active_path(&self.dir, self.stem, self.ext)
    }

    fn open_active(&mut self) -> std::io::Result<&mut File> {
        if self.active.is_none() {
            std::fs::create_dir_all(&self.dir)?;
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.active_path())?;
            self.active_len = file.metadata()?.len();
            self.active = Some(file);
        }
        Ok(self.active.as_mut().expect("active file is open"))
    }

    pub fn append(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        // One record larger than a segment keeps only its tail.
        let segment_max = self.segment_max();
        let bytes = if bytes.len() as u64 > segment_max {
            &bytes[bytes.len() - segment_max as usize..]
        } else {
            bytes
        };
        self.open_active()?;
        if self.active_len > 0 && self.active_len + bytes.len() as u64 > segment_max {
            self.rotate()?;
            self.open_active()?;
        }
        let file = self.active.as_mut().expect("active file is open");
        file.write_all(bytes)?;
        self.active_len += bytes.len() as u64;
        Ok(())
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        if let Some(file) = self.active.as_mut() {
            file.flush()?;
        }
        Ok(())
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        self.active = None;
        self.active_len = 0;
        let next = numbered_segments(&self.dir, self.stem, self.ext)
            .last()
            .map(|(n, _)| n + 1)
            .unwrap_or(1);
        std::fs::rename(
            self.active_path(),
            self.dir.join(format!("{}.{next}.{}", self.stem, self.ext)),
        )?;
        self.prune()
    }

    /// Delete the oldest segments until the closed segments fit the cap with
    /// room for one active segment.
    fn prune(&self) -> std::io::Result<()> {
        let budget = self.max_bytes.saturating_sub(self.segment_max());
        let mut segments: Vec<(u64, PathBuf, u64)> = numbered_segments(&self.dir, self.stem, self.ext)
            .into_iter()
            .map(|(n, path)| {
                let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                (n, path, len)
            })
            .collect();
        let mut total: u64 = segments.iter().map(|(_, _, len)| len).sum();
        while total > budget && !segments.is_empty() {
            let (_, path, len) = segments.remove(0);
            std::fs::remove_file(path)?;
            total -= len;
        }
        Ok(())
    }
}

fn active_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    dir.join(format!("{stem}.{ext}"))
}

/// Closed segments, oldest first.
fn numbered_segments(dir: &Path, stem: &str, ext: &str) -> Vec<(u64, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let prefix = format!("{stem}.");
    let suffix = format!(".{ext}");
    let mut found: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let middle = name.strip_prefix(&prefix)?.strip_suffix(&suffix)?;
            let n: u64 = middle.parse().ok()?;
            Some((n, entry.path()))
        })
        .collect();
    found.sort_by_key(|(n, _)| *n);
    found
}

/// Every file for one capped log, oldest first, ending with the active file.
pub fn segment_paths(dir: &Path, stem: &str, ext: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = numbered_segments(dir, stem, ext)
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    let active = active_path(dir, stem, ext);
    if active.is_file() {
        paths.push(active);
    }
    paths
}

/// The newest console events that fit in `max_bytes`, oldest first. Files are
/// read line by line, so memory stays near `max_bytes`.
pub fn read_console_tail(dir: &Path, max_bytes: u64) -> Vec<serde_json::Value> {
    let mut kept: VecDeque<(u64, String)> = VecDeque::new();
    let mut total = 0u64;
    for path in segment_paths(dir, CONSOLE_STEM, CONSOLE_EXT) {
        let Ok(file) = File::open(&path) else {
            continue;
        };
        for line in BufReader::new(file).lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }
            let len = line.len() as u64 + 1;
            if len > max_bytes {
                continue;
            }
            total += len;
            kept.push_back((len, line));
            while total > max_bytes {
                let Some((dropped, _)) = kept.pop_front() else {
                    break;
                };
                total -= dropped;
            }
        }
    }
    kept.into_iter()
        .filter_map(|(_, line)| serde_json::from_str(&line).ok())
        .collect()
}

/// The last `max_bytes` of the terminal log across its segments.
pub fn read_terminal_tail(dir: &Path, max_bytes: u64) -> Option<Vec<u8>> {
    let paths = segment_paths(dir, TERMINAL_STEM, TERMINAL_EXT);
    let sizes: Vec<u64> = paths
        .iter()
        .map(|path| std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
        .collect();
    let total: u64 = sizes.iter().sum();
    if total == 0 {
        return None;
    }
    let mut skip = total.saturating_sub(max_bytes);
    let mut out = Vec::with_capacity(total.min(max_bytes) as usize);
    for (path, size) in paths.iter().zip(sizes) {
        if skip >= size {
            skip -= size;
            continue;
        }
        let Ok(mut file) = File::open(path) else {
            continue;
        };
        if file.seek(SeekFrom::Start(skip)).is_err() {
            continue;
        }
        skip = 0;
        let _ = file.read_to_end(&mut out);
    }
    (!out.is_empty()).then_some(out)
}

enum Item {
    Console(Vec<u8>),
    Frame(Vec<u8>),
    Flush(mpsc::Sender<()>),
}

/// The writer for one run. Publishing never blocks on disk: records go over a
/// channel to one writer thread per run.
pub struct RunArtifactLog {
    tx: Mutex<Option<mpsc::Sender<Item>>>,
    frames: Arc<AtomicU64>,
}

impl RunArtifactLog {
    pub fn start(dir: PathBuf, max_bytes: u64) -> Self {
        let (tx, rx) = mpsc::channel::<Item>();
        let frames = Arc::new(AtomicU64::new(0));
        std::thread::Builder::new()
            .name("run-artifact-log".into())
            .spawn(move || {
                let mut console = RotatingFile::new(&dir, CONSOLE_STEM, CONSOLE_EXT, max_bytes);
                let mut terminal = RotatingFile::new(&dir, TERMINAL_STEM, TERMINAL_EXT, max_bytes);
                let mut warned = false;
                for item in rx {
                    let result = match item {
                        Item::Console(line) => console.append(&line),
                        Item::Frame(data) => terminal.append(&data),
                        Item::Flush(ack) => {
                            let result = console.flush().and_then(|()| terminal.flush());
                            let _ = ack.send(());
                            result
                        }
                    };
                    if let Err(err) = result {
                        if !warned {
                            tracing::warn!(dir = %dir.display(), error = %err, "could not write run artifacts");
                            warned = true;
                        }
                    }
                }
            })
            .expect("spawn run artifact writer");
        Self {
            tx: Mutex::new(Some(tx)),
            frames,
        }
    }

    fn send(&self, item: Item) {
        if let Some(tx) = self.tx.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = tx.send(item);
        }
    }

    pub fn console(&self, event: &serde_json::Value) {
        if let Ok(mut line) = serde_json::to_vec(event) {
            line.push(b'\n');
            self.send(Item::Console(line));
        }
    }

    pub fn frame(&self, data: &[u8]) {
        self.frames.fetch_add(1, Ordering::Relaxed);
        self.send(Item::Frame(data.to_vec()));
    }

    pub fn frame_count(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    /// Wait until everything published so far is on disk.
    pub fn flush(&self) {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.send(Item::Flush(ack_tx));
        let _ = ack_rx.recv_timeout(Duration::from_secs(10));
    }
}

impl Drop for RunArtifactLog {
    fn drop(&mut self) {
        // Closing the channel ends the writer thread after it drains.
        self.tx.lock().unwrap_or_else(|e| e.into_inner()).take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dir_bytes(dir: &Path, stem: &str, ext: &str) -> u64 {
        segment_paths(dir, stem, ext)
            .iter()
            .map(|path| std::fs::metadata(path).unwrap().len())
            .sum()
    }

    #[test]
    fn every_event_is_kept_below_the_cap() {
        let temp = tempfile::tempdir().unwrap();
        let log = RunArtifactLog::start(temp.path().to_path_buf(), 64 * 1024 * 1024);
        for i in 0..5_000 {
            log.console(&json!({"type": "claude-code.console.text", "i": i}));
        }
        log.flush();
        let events = read_console_tail(temp.path(), u64::MAX);
        assert_eq!(events.len(), 5_000);
        assert_eq!(events[0]["i"], 0);
        assert_eq!(events[4_999]["i"], 4_999);
    }

    #[test]
    fn rotation_keeps_the_newest_events_within_the_cap() {
        let temp = tempfile::tempdir().unwrap();
        let cap = 64 * 1024;
        let log = RunArtifactLog::start(temp.path().to_path_buf(), cap);
        let pad = "x".repeat(200);
        for i in 0..5_000 {
            log.console(&json!({"type": "codex.console.text", "i": i, "pad": pad}));
        }
        log.flush();
        assert!(dir_bytes(temp.path(), CONSOLE_STEM, CONSOLE_EXT) <= cap);
        assert!(segment_paths(temp.path(), CONSOLE_STEM, CONSOLE_EXT).len() > 1);
        let events = read_console_tail(temp.path(), u64::MAX);
        assert!(!events.is_empty());
        assert_eq!(events.last().unwrap()["i"], 4_999);
        // Rotated segments are read in order.
        let ids: Vec<i64> = events.iter().map(|e| e["i"].as_i64().unwrap()).collect();
        assert!(ids.windows(2).all(|pair| pair[1] == pair[0] + 1));
    }

    #[test]
    fn recovery_reads_a_bounded_tail() {
        let temp = tempfile::tempdir().unwrap();
        let log = RunArtifactLog::start(temp.path().to_path_buf(), 1024 * 1024);
        for i in 0..1_000 {
            log.console(&json!({"type": "cursor.console.text", "i": i}));
        }
        log.flush();
        let tail = read_console_tail(temp.path(), 2_000);
        assert!(!tail.is_empty() && tail.len() < 1_000);
        assert_eq!(tail.last().unwrap()["i"], 999);
    }

    #[test]
    fn terminal_frames_append_and_tail_across_segments() {
        let temp = tempfile::tempdir().unwrap();
        let log = RunArtifactLog::start(temp.path().to_path_buf(), 64 * 1024);
        for i in 0..2_000 {
            log.frame(format!("line {i}\n").as_bytes());
        }
        log.flush();
        assert_eq!(log.frame_count(), 2_000);
        assert!(dir_bytes(temp.path(), TERMINAL_STEM, TERMINAL_EXT) <= 64 * 1024);
        let tail = read_terminal_tail(temp.path(), 100).unwrap();
        assert_eq!(tail.len(), 100);
        assert!(String::from_utf8(tail).unwrap().ends_with("line 1999\n"));
    }
}
