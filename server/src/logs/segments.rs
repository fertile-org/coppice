//! Size-capped daily log segments: `coppice.YYYY-MM-DD.N.jsonl`.
//!
//! The appender starts a new segment when the day changes or the active one
//! passes [`SEGMENT_MAX_BYTES`], and points `current.jsonl` at the active
//! segment. Readers walk segments newest first with [`ReverseLines`].

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use time::{Date, OffsetDateTime};

/// A segment rolls over once it passes this size.
pub const SEGMENT_MAX_BYTES: u64 = 32 * 1024 * 1024;

const CURRENT: &str = "current.jsonl";

/// A parsed JSONL log file name. `seg` is 0 for the old one-file-per-day name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogName {
    pub date: Date,
    pub seg: u32,
    pub compressed: bool,
}

/// `coppice.YYYY-MM-DD[.N].jsonl[.zst]`.
pub fn parse_log_name(name: &str) -> Option<LogName> {
    let rest = name.strip_prefix("coppice.")?;
    let (rest, compressed) = match rest.strip_suffix(".jsonl.zst") {
        Some(rest) => (rest, true),
        None => (rest.strip_suffix(".jsonl")?, false),
    };
    let (date, seg) = match rest.split_once('.') {
        Some((date, seg)) => (date, seg.parse().ok()?),
        None => (rest, 0),
    };
    Some(LogName {
        date: parse_date(date)?,
        seg,
        compressed,
    })
}

pub fn parse_date(value: &str) -> Option<Date> {
    let mut parts = value.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || value.len() != 10 {
        return None;
    }
    Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()
}

pub fn segment_file_name(date: Date, seg: u32) -> String {
    format!("coppice.{date}.{seg}.jsonl")
}

/// The file `current.jsonl` points at, as a file name in `dir`.
pub fn current_target_name(dir: &Path) -> Option<String> {
    let link = fs::read_link(dir.join(CURRENT)).ok()?;
    link.file_name()?.to_str().map(str::to_string)
}

struct Active {
    date: Date,
    seg: u32,
    file: File,
    len: u64,
}

/// Appends log lines to the active segment, rolling by day and by size.
pub struct SegmentedAppender {
    dir: PathBuf,
    max_bytes: u64,
    active: Mutex<Option<Active>>,
}

impl SegmentedAppender {
    pub fn new(dir: &Path) -> io::Result<Self> {
        Self::with_max_bytes(dir, SEGMENT_MAX_BYTES)
    }

    pub fn with_max_bytes(dir: &Path, max_bytes: u64) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let appender = Self {
            dir: dir.to_path_buf(),
            max_bytes: max_bytes.max(1),
            active: Mutex::new(None),
        };
        let today = OffsetDateTime::now_utc().date();
        let seg = appender.latest_seg(today).unwrap_or(0);
        *appender.active.lock().unwrap_or_else(|e| e.into_inner()) = Some(appender.open(today, seg)?);
        Ok(appender)
    }

    /// The highest segment number on disk for `date`, compressed or not.
    fn latest_seg(&self, date: Date) -> Option<u32> {
        fs::read_dir(&self.dir)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|entry| parse_log_name(entry.file_name().to_str()?))
            .filter(|name| name.date == date)
            .map(|name| name.seg)
            .max()
    }

    fn open(&self, date: Date, seg: u32) -> io::Result<Active> {
        let name = segment_file_name(date, seg);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(&name))?;
        let len = file.metadata()?.len();
        self.point_current(&name);
        Ok(Active {
            date,
            seg,
            file,
            len,
        })
    }

    fn point_current(&self, name: &str) {
        let link = self.dir.join(CURRENT);
        let tmp = self.dir.join(".current.jsonl.tmp");
        let _ = fs::remove_file(&tmp);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(name, &tmp);
        #[cfg(not(unix))]
        let made: io::Result<()> = Err(io::Error::other("no symlinks"));
        if made.is_ok() {
            let _ = fs::rename(&tmp, &link);
        }
    }

    /// Write one complete line (or several), rolling first when needed.
    pub fn write_record(&self, buf: &[u8]) -> io::Result<()> {
        let mut guard = self.active.lock().unwrap_or_else(|e| e.into_inner());
        let today = OffsetDateTime::now_utc().date();
        let roll = match guard.as_ref() {
            None => Some((today, self.latest_seg(today).unwrap_or(0))),
            Some(active) if active.date != today => {
                Some((today, self.latest_seg(today).map_or(0, |seg| seg + 1)))
            }
            Some(active) if active.len > 0 && active.len + buf.len() as u64 > self.max_bytes => {
                Some((active.date, active.seg + 1))
            }
            Some(_) => None,
        };
        if let Some((date, seg)) = roll {
            *guard = Some(self.open(date, seg)?);
        }
        let active = guard.as_mut().expect("active segment");
        active.file.write_all(buf)?;
        active.len += buf.len() as u64;
        Ok(())
    }

    pub fn writer(&self) -> SegmentWriter<'_> {
        SegmentWriter {
            appender: self,
            pending: Vec::new(),
        }
    }
}

/// Holds bytes until a line is complete, so each record written to a segment
/// is whole lines.
pub struct SegmentWriter<'a> {
    appender: &'a SegmentedAppender,
    pending: Vec<u8>,
}

impl Write for SegmentWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        if let Some(last) = self.pending.iter().rposition(|byte| *byte == b'\n') {
            let rest = self.pending.split_off(last + 1);
            let lines = std::mem::replace(&mut self.pending, rest);
            self.appender.write_record(&lines)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for SegmentWriter<'_> {
    fn drop(&mut self) {
        if !self.pending.is_empty() {
            let _ = self.appender.write_record(&std::mem::take(&mut self.pending));
        }
    }
}

const BLOCK: u64 = 64 * 1024;

/// Lines of a plain file, newest first, read in blocks from the end. Each
/// item is the line's starting byte offset and its text.
pub struct ReverseLines {
    file: File,
    /// File offset of `chunk[0]`.
    start: u64,
    /// Unreturned bytes `[start, start + chunk.len())`.
    chunk: Vec<u8>,
    bytes_read: u64,
}

impl ReverseLines {
    /// Lines that start before `end` (the whole file when `end` is past it).
    pub fn before(path: &Path, end: u64) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        let end = end.min(len);
        Ok(Self {
            file,
            start: end,
            chunk: Vec::new(),
            bytes_read: 0,
        })
    }

    fn read_block(&mut self) -> io::Result<()> {
        let new_start = self.start.saturating_sub(BLOCK);
        let size = usize::try_from(self.start - new_start).unwrap_or(0);
        let mut block = vec![0u8; size];
        self.file.seek(SeekFrom::Start(new_start))?;
        self.file.read_exact(&mut block)?;
        block.extend_from_slice(&self.chunk);
        self.chunk = block;
        self.start = new_start;
        self.bytes_read += size as u64;
        Ok(())
    }

    /// Bytes read from disk so far.
    #[cfg(test)]
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

impl Iterator for ReverseLines {
    type Item = io::Result<(u64, String)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let line_end = if self.chunk.last() == Some(&b'\n') {
                self.chunk.len() - 1
            } else {
                self.chunk.len()
            };
            if let Some(newline) = self.chunk[..line_end].iter().rposition(|b| *b == b'\n') {
                let text = String::from_utf8_lossy(&self.chunk[newline + 1..line_end]).into_owned();
                let offset = self.start + newline as u64 + 1;
                self.chunk.truncate(newline + 1);
                return Some(Ok((offset, text)));
            }
            if self.start == 0 {
                if self.chunk.is_empty() {
                    return None;
                }
                let text = String::from_utf8_lossy(&self.chunk[..line_end]).into_owned();
                self.chunk.clear();
                return Some(Ok((0, text)));
            }
            if let Err(err) = self.read_block() {
                self.chunk.clear();
                self.start = 0;
                return Some(Err(err));
            }
        }
    }
}

/// Lines of a compressed file that start before `end`, read forward. Only the
/// newest `keep` lines that pass `accept` are held. Returned newest first.
pub fn compressed_lines_before(
    path: &Path,
    end: u64,
    keep: usize,
    mut accept: impl FnMut(&str) -> bool,
) -> io::Result<Vec<(u64, String)>> {
    use std::io::BufRead;
    let decoder = zstd::stream::read::Decoder::new(File::open(path)?)?;
    let mut reader = io::BufReader::new(decoder);
    let mut ring: std::collections::VecDeque<(u64, String)> = std::collections::VecDeque::new();
    let mut offset = 0u64;
    let mut raw = Vec::new();
    loop {
        raw.clear();
        let read = reader.read_until(b'\n', &mut raw)?;
        if read == 0 || offset >= end {
            break;
        }
        let line_start = offset;
        offset += read as u64;
        let mut line = &raw[..];
        if line.last() == Some(&b'\n') {
            line = &line[..line.len() - 1];
        }
        let text = String::from_utf8_lossy(line);
        if text.trim().is_empty() || !accept(&text) {
            continue;
        }
        ring.push_back((line_start, text.into_owned()));
        if ring.len() > keep {
            ring.pop_front();
        }
    }
    Ok(ring.into_iter().rev().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_with_and_without_a_segment() {
        let date = parse_date("2026-10-18").unwrap();
        assert_eq!(
            parse_log_name("coppice.2026-10-18.3.jsonl"),
            Some(LogName { date, seg: 3, compressed: false })
        );
        assert_eq!(
            parse_log_name("coppice.2026-10-18.jsonl.zst"),
            Some(LogName { date, seg: 0, compressed: true })
        );
        assert_eq!(parse_log_name("current.jsonl"), None);
        assert_eq!(parse_log_name("coppice.2026-10-18.x.jsonl"), None);
    }

    #[test]
    fn the_appender_rolls_by_size_and_moves_current() {
        let dir = tempfile::tempdir().unwrap();
        let appender = SegmentedAppender::with_max_bytes(dir.path(), 100).unwrap();
        for i in 0..50 {
            appender.write_record(format!("line {i:04}\n").as_bytes()).unwrap();
        }
        let today = OffsetDateTime::now_utc().date();
        let mut segs: Vec<u32> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| parse_log_name(e.unwrap().file_name().to_str()?))
            .map(|name| name.seg)
            .collect();
        segs.sort();
        assert!(segs.len() > 3, "{segs:?}");
        for seg in &segs {
            let len = fs::metadata(dir.path().join(segment_file_name(today, *seg))).unwrap().len();
            assert!(len <= 100, "segment {seg} is {len} bytes");
        }
        assert_eq!(
            current_target_name(dir.path()),
            Some(segment_file_name(today, *segs.last().unwrap()))
        );
        // A restart continues the latest segment.
        drop(appender);
        let again = SegmentedAppender::with_max_bytes(dir.path(), 100).unwrap();
        again.write_record(b"after restart\n").unwrap();
        let latest = fs::read_to_string(dir.path().join(segment_file_name(today, *segs.last().unwrap())));
        let next = fs::read_to_string(dir.path().join(segment_file_name(today, segs.last().unwrap() + 1)));
        assert!(
            latest.unwrap_or_default().contains("after restart")
                || next.unwrap_or_default().contains("after restart")
        );
    }

    #[test]
    fn reverse_lines_walk_back_across_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.jsonl");
        let mut text = String::new();
        for i in 0..20_000 {
            text.push_str(&format!("line {i}\n"));
        }
        text.push_str("partial");
        fs::write(&path, &text).unwrap();
        let lines: Vec<(u64, String)> = ReverseLines::before(&path, u64::MAX)
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(lines.len(), 20_001);
        assert_eq!(lines[0].1, "partial");
        assert_eq!(lines[1].1, "line 19999");
        assert_eq!(lines.last().unwrap(), &(0, "line 0".to_string()));
        let (offset, _) = lines[100].clone();
        let resumed: Vec<String> = ReverseLines::before(&path, offset)
            .unwrap()
            .take(2)
            .map(|line| line.unwrap().1)
            .collect();
        assert_eq!(resumed, vec![lines[101].1.clone(), lines[102].1.clone()]);
    }

    #[test]
    fn a_page_from_a_large_file_reads_only_its_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.jsonl");
        let line = format!("{}\n", "x".repeat(99));
        let mut file = File::create(&path).unwrap();
        for _ in 0..500_000 {
            file.write_all(line.as_bytes()).unwrap();
        }
        drop(file);
        assert_eq!(fs::metadata(&path).unwrap().len(), 50_000_000);
        let mut lines = ReverseLines::before(&path, u64::MAX).unwrap();
        for _ in 0..200 {
            lines.next().unwrap().unwrap();
        }
        assert!(lines.bytes_read() <= 2 * BLOCK, "read {} bytes", lines.bytes_read());
    }

    #[test]
    fn compressed_lines_keep_only_the_newest_before_an_offset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.jsonl.zst");
        let mut text = String::new();
        for i in 0..1_000 {
            text.push_str(&format!("line {i}\n"));
        }
        fs::write(&path, zstd::encode_all(text.as_bytes(), 0).unwrap()).unwrap();
        let newest = compressed_lines_before(&path, u64::MAX, 3, |_| true).unwrap();
        assert_eq!(
            newest.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(),
            vec!["line 999", "line 998", "line 997"]
        );
        let before = compressed_lines_before(&path, newest[2].0, 1, |_| true).unwrap();
        assert_eq!(before[0].1, "line 996");
    }
}
