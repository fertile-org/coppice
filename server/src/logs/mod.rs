//! Size-capped daily JSONL for server, HTTP, and agent CLI lines, and the write-time secret mask.

pub(crate) mod export;
mod mask;
pub(crate) mod meta;
pub(crate) mod postgres_files;
pub(crate) mod read;
pub(crate) mod retain;
pub(crate) mod segments;
pub(crate) mod tail;

use std::io::{self, IsTerminal, Write};
use std::path::Path;

use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

pub use mask::SecretGuard;

/// Tracing target for agent CLI lines.
const CLI_LOG_TARGET: &str = "coppice_server::cli";

/// Longest CLI line copied into the server log, in bytes.
pub const CLI_LINE_MAX_BYTES: usize = 4 * 1024;

/// `line` cut to [`CLI_LINE_MAX_BYTES`] on a character boundary, with a note
/// saying how much was left out.
fn capped_cli_line(line: &str) -> std::borrow::Cow<'_, str> {
    if line.len() <= CLI_LINE_MAX_BYTES {
        return std::borrow::Cow::Borrowed(line);
    }
    let mut end = CLI_LINE_MAX_BYTES;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    std::borrow::Cow::Owned(format!(
        "{}…[truncated {} bytes]",
        &line[..end],
        line.len() - end
    ))
}

/// One info event for a CLI stdout or stderr line, cut to
/// [`CLI_LINE_MAX_BYTES`]. `stream` is `stdout` or `stderr`. `run_id` and
/// `ticket_id` are omitted when absent.
pub fn log_cli_line(
    run_id: Option<&str>,
    ticket_id: Option<&str>,
    stream: &'static str,
    line: &str,
) {
    let line = capped_cli_line(line);
    let line = line.as_ref();
    let run_id = run_id.filter(|id| !id.is_empty());
    let ticket_id = ticket_id.filter(|id| !id.is_empty());
    match (run_id, ticket_id) {
        (Some(run_id), Some(ticket_id)) => {
            tracing::info!(target: CLI_LOG_TARGET, source = "cli", stream, run_id, ticket_id, "{line}");
        }
        (Some(run_id), None) => {
            tracing::info!(target: CLI_LOG_TARGET, source = "cli", stream, run_id, "{line}");
        }
        (None, Some(ticket_id)) => {
            tracing::info!(target: CLI_LOG_TARGET, source = "cli", stream, ticket_id, "{line}");
        }
        (None, None) => {
            tracing::info!(target: CLI_LOG_TARGET, source = "cli", stream, "{line}");
        }
    }
}

/// Read a piped CLI stream until EOF and record each line.
pub async fn pump_lines(
    reader: impl tokio::io::AsyncRead + Unpin,
    run_id: Option<String>,
    ticket_id: Option<String>,
    stream: &'static str,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        log_cli_line(run_id.as_deref(), ticket_id.as_deref(), stream, &line);
    }
}

/// Secrets placed on a child that inherits the server environment:
/// `COPPICE_MCP_TOKEN`, connector `install.auth_env` values passed in `env`,
/// and those auth values when the child inherits them from this process.
pub fn guard_for_child_env<I, K, V>(env: I) -> SecretGuard
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut pairs = Vec::new();
    for (key, value) in env {
        let key = key.as_ref();
        if is_forwarded_secret(key) {
            pairs.push((key.to_string(), value.as_ref().to_string()));
        }
    }
    for key in auth_env_keys() {
        if pairs.iter().any(|(name, _)| name == key) {
            continue;
        }
        if let Ok(value) = std::env::var(key) {
            pairs.push((key.to_string(), value));
        }
    }
    SecretGuard::register(pairs)
}

fn is_forwarded_secret(key: &str) -> bool {
    key == "COPPICE_MCP_TOKEN" || auth_env_keys().contains(&key)
}

fn auth_env_keys() -> Vec<&'static str> {
    let mut keys = Vec::new();
    for connector in coppice_connectors::all() {
        for key in connector.install.auth_env {
            if !keys.contains(key) {
                keys.push(*key);
            }
        }
    }
    keys
}

/// rmcp logs plugin-controlled content below warn, so it stays at warn unless
/// the operator's `RUST_LOG` names it.
fn log_directives(rust_log: Option<&str>) -> String {
    let base = rust_log.filter(|s| !s.trim().is_empty()).unwrap_or("info");
    if base.contains("rmcp") {
        base.to_string()
    } else {
        format!("{base},rmcp=warn")
    }
}

fn env_filter() -> tracing_subscriber::EnvFilter {
    let rust_log = std::env::var(tracing_subscriber::EnvFilter::DEFAULT_ENV).ok();
    let directives = log_directives(rust_log.as_deref());
    tracing_subscriber::EnvFilter::try_new(&directives)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_directives(None)))
}

fn rolling(logs_dir: &Path) -> io::Result<segments::SegmentedAppender> {
    segments::SegmentedAppender::new(logs_dir)
}

struct MaskingAppender {
    inner: segments::SegmentedAppender,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for MaskingAppender {
    type Writer = mask::MaskingWriter<segments::SegmentWriter<'a>>;

    fn make_writer(&'a self) -> Self::Writer {
        // The masking writer hands whole lines to the segment writer, so a
        // segment never ends in the middle of a line.
        mask::MaskingWriter::file(self.inner.writer())
    }
}

fn json_file_layer<S>(
    logs_dir: &Path,
    respect_env: bool,
) -> io::Result<Box<dyn Layer<S> + Send + Sync>>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let appender = MaskingAppender {
        inner: rolling(logs_dir)?,
    };
    let layer = tracing_subscriber::fmt::layer()
        .json()
        .with_ansi(false)
        .with_timer(UtcTime::rfc_3339())
        .with_writer(appender);
    // The JSON formatter only emits fields the event recorded. CLI and HTTP
    // events set `source` themselves. A line with no `source` is a server event.
    if respect_env {
        Ok(Box::new(layer.with_filter(env_filter())))
    } else {
        Ok(Box::new(layer.with_filter(
            tracing_subscriber::filter::LevelFilter::INFO,
        )))
    }
}

/// Human stdout subscriber plus the JSONL file. The env filter is `RUST_LOG`,
/// or `info` plus `rmcp=warn`. The file records every event that passes it.
/// Set once the desktop app has printed its ready line.
static STDOUT_QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// From now on stdout carries only warnings and errors. The JSONL files keep
/// everything.
pub fn quiet_stdout() {
    STDOUT_QUIET.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn stdout_allows(level: &tracing::Level) -> bool {
    !STDOUT_QUIET.load(std::sync::atomic::Ordering::Relaxed) || *level <= tracing::Level::WARN
}

pub fn init(logs_dir: &Path) -> io::Result<()> {
    use tracing_subscriber::filter::FilterExt;
    let terminal = io::stdout().is_terminal();
    let stdout = tracing_subscriber::fmt::layer()
        .with_ansi(terminal)
        .with_writer(move || {
            mask::MaskingWriter::new(if terminal {
                StdoutWriter::Term(io::stdout())
            } else {
                StdoutWriter::Pipe(FlushingWriter {
                    inner: io::stdout(),
                })
            })
        })
        .with_filter(env_filter().and(tracing_subscriber::filter::filter_fn(|meta| {
            stdout_allows(meta.level())
        })));
    tracing_subscriber::registry()
        .with(stdout)
        .with(json_file_layer(logs_dir, true)?)
        .init();
    Ok(())
}

/// Run `f` with the JSONL subscriber pointed at `logs_dir`.
/// The test subscriber records info and above for every target, even when
/// `RUST_LOG` is narrower.
pub fn with_cli_logs<R>(logs_dir: &Path, f: impl FnOnce() -> R) -> io::Result<R> {
    let layer = json_file_layer(logs_dir, false)?;
    Ok(tracing::subscriber::with_default(
        tracing_subscriber::registry().with(layer),
        f,
    ))
}

/// Flushes after every write so a block-buffered pipe cannot keep a log line.
pub struct FlushingWriter<W> {
    inner: W,
}

impl<W: Write> Write for FlushingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.inner.flush()?;
        Ok(n)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.inner.write_all(buf)?;
        self.inner.flush()
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

enum StdoutWriter {
    Term(io::Stdout),
    Pipe(FlushingWriter<io::Stdout>),
}

impl Write for StdoutWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Term(writer) => writer.write(buf),
            Self::Pipe(writer) => writer.write(buf),
        }
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            Self::Term(writer) => writer.write_all(buf),
            Self::Pipe(writer) => writer.write_all(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Term(writer) => writer.flush(),
            Self::Pipe(writer) => writer.flush(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_the_appender_does_not_delete_a_compressed_day() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zst = dir.path().join("coppice.2020-01-01.jsonl.zst");
        std::fs::write(&zst, b"kept-compressed").expect("write");
        rolling(dir.path()).expect("appender");
        assert_eq!(std::fs::read(&zst).expect("zst"), b"kept-compressed");
    }

    #[test]
    fn quiet_stdout_keeps_only_warnings_and_errors() {
        assert!(stdout_allows(&tracing::Level::INFO));
        quiet_stdout();
        assert!(!stdout_allows(&tracing::Level::INFO));
        assert!(!stdout_allows(&tracing::Level::DEBUG));
        assert!(stdout_allows(&tracing::Level::WARN));
        assert!(stdout_allows(&tracing::Level::ERROR));
        STDOUT_QUIET.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    #[test]
    fn long_cli_lines_are_cut_on_a_character_boundary() {
        let short = "fine";
        assert_eq!(capped_cli_line(short), short);
        let long = format!("{}é{}", "a".repeat(CLI_LINE_MAX_BYTES - 1), "b".repeat(100));
        let capped = capped_cli_line(&long);
        assert!(capped.starts_with(&"a".repeat(CLI_LINE_MAX_BYTES - 1)));
        assert!(capped.ends_with("…[truncated 102 bytes]"), "{capped}");
    }

    #[test]
    fn rmcp_defaults_to_warn_unless_named() {
        assert_eq!(log_directives(None), "info,rmcp=warn");
        assert_eq!(log_directives(Some(" ")), "info,rmcp=warn");
        assert_eq!(log_directives(Some("debug")), "debug,rmcp=warn");
        assert_eq!(log_directives(Some("info,rmcp=debug")), "info,rmcp=debug");
    }

    #[test]
    fn flushing_writer_pushes_a_line_through_a_block_buffer() {
        use std::sync::mpsc;
        use std::time::Duration;

        let (reader, writer) = std::io::pipe().expect("pipe");
        let buffered = std::io::BufWriter::with_capacity(8 * 1024, writer);
        let mut out = FlushingWriter { inner: buffered };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 32];
            let n = std::io::Read::read(&mut reader, &mut buf).unwrap_or(0);
            let _ = tx.send(buf[..n].to_vec());
        });
        out.write_all(b"hello\n").expect("write");
        let got = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("line was not flushed");
        assert_eq!(got, b"hello\n");
    }

    #[test]
    fn printed_secret_is_stored_only_as_its_name() {
        let token = "coppice-mask-7kQ9mP2x";
        let short = "abcde";
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = SecretGuard::register([("COPPICE_MCP_TOKEN", token), ("TINY", short)]);
        with_cli_logs(dir.path(), || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            runtime.block_on(async {
                let mut handler = Nop;
                let invocation = crate::providers::cli_runner::CliInvocation {
                    program: "sh".into(),
                    args: vec![
                        "-c".into(),
                        "printf '%s\\n' \"$COPPICE_MCP_TOKEN $TINY\"".into(),
                    ],
                    env: vec![
                        ("COPPICE_MCP_TOKEN".into(), token.into()),
                        ("TINY".into(), short.into()),
                    ],
                    cwd: std::env::temp_dir(),
                    timeout: std::time::Duration::from_secs(10),
                    run_id: Some("run-mask".into()),
                    artifacts_dir: None,
                    result_before_exit: false,
                    result_exit_grace: None,
                    ticket_id: None,
                };
                crate::providers::cli_runner::run_cli(
                    invocation,
                    &mut handler,
                    crate::providers::cli_runner::RunIo {
                        cancel_rx: None,
                        session_created_tx: None,
                    },
                )
                .await
                .expect("print secret");
            });
        })
        .expect("subscriber");

        let stored = directory_bytes(dir.path());
        assert!(
            !stored
                .windows(token.len())
                .any(|window| window == token.as_bytes()),
            "token bytes reached the logs directory"
        );
        let text = String::from_utf8_lossy(&stored);
        assert!(text.contains("\u{2039}COPPICE_MCP_TOKEN\u{203A}"), "{text}");
        assert!(text.contains(short), "{text}");
    }

    #[test]
    fn server_event_secret_is_stored_only_as_its_name() {
        let token = "coppice-mask-7kQ9mP2x";
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = SecretGuard::register([("COPPICE_MCP_TOKEN", token)]);
        with_cli_logs(dir.path(), || {
            tracing::warn!("connector returned {token}");
        })
        .expect("subscriber");

        let stored = directory_bytes(dir.path());
        assert!(
            !stored
                .windows(token.len())
                .any(|window| window == token.as_bytes()),
            "token bytes reached the logs directory"
        );
        let text = String::from_utf8_lossy(&stored);
        assert!(text.contains("\u{2039}COPPICE_MCP_TOKEN\u{203A}"), "{text}");
        let jsonl = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .map(|entry| entry.expect("entry").path())
            .find(|path| {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                name.starts_with("coppice.") && name.ends_with(".jsonl")
            })
            .expect("daily file");
        let file = std::fs::read_to_string(&jsonl).expect("jsonl");
        let value: serde_json::Value = serde_json::from_str(
            file.lines()
                .find(|line| line.contains("connector returned"))
                .expect("server line"),
        )
        .expect("json");
        assert_eq!(value["fields"]["source"], "server");
        assert!(
            value["fields"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("\u{2039}COPPICE_MCP_TOKEN\u{203A}"),
            "{value}"
        );
    }
}

#[cfg(test)]
struct Nop;

#[cfg(test)]
impl crate::providers::cli_runner::LineHandler for Nop {
    fn on_json(&mut self, _value: &serde_json::Value) -> crate::providers::cli_runner::LineStep {
        crate::providers::cli_runner::LineStep::default()
    }
}

#[cfg(test)]
fn directory_bytes(dir: &Path) -> Vec<u8> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        let meta = std::fs::symlink_metadata(&path).expect("metadata");
        out.extend_from_slice(path.to_string_lossy().as_bytes());
        if meta.file_type().is_dir() {
            for entry in std::fs::read_dir(&path).expect("read_dir") {
                stack.push(entry.expect("entry").path());
            }
            continue;
        }
        if meta.file_type().is_symlink() {
            if let Ok(target) = std::fs::read_link(&path) {
                out.extend_from_slice(target.to_string_lossy().as_bytes());
            }
        }
        if let Ok(bytes) = std::fs::read(&path) {
            out.extend_from_slice(&bytes);
        }
    }
    out
}
