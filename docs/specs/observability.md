# Observability: logs page, export, retention

Design for [issue #77](https://github.com/fertile-org/coppice/issues/77). This is the spec only. It does not change the server, the desktop shell, or the SPA.

Hung's asks: one place in the UI for server logs (including the request log) and agent CLI logs; an export he can attach to a GitHub issue; a setting for how long logs are kept; a small open-source library if one fits; keep it simple.

The store is daily JSON-lines files written by `tracing` and `tracing-appender`. A small sweep compresses past days with `zstd` and deletes by age and by a fixed size backstop. The page reads those files, plus `postgres.log`. Logs are not inserted into Postgres, and there is no index.

## Build order

Each step is one build PR. Later steps stay out of earlier PRs.

1. **Capture CLI output.** This is the first build PR. It writes agent CLI stderr and non-JSON stdout from `server/src/providers/cli_runner.rs`, and OpenCode stdout and stderr from `spawn` in `server/src/sessions/opencode_run_server.rs`, into the daily JSONL files. Every one of those lines carries that run's id (`fields.run_id`) and `fields.stream` (`stdout` or `stderr`). The same pump also writes JSON stdout lines, so the runner has one path. This PR adds the file writer those lines need and the write-time mask, so a secret printed by the CLI does not reach disk. It does not add the page, export, or the retention setting.
2. **Server lines.** The request log and other server events go into the same files, with `source` `http` or `server`.
3. **Read API, live tail, and the Logs page.** Admin-only. Filters include the `postgres` source. A run links to its lines.
4. **Export and the report link.** Zip of the current view, then the placeholder link that opens a prefilled GitHub issue.
5. **Retention.** The "Keep logs for" setting, the `zstd` sweep, the 256 MiB backstop, and deletion of `server.log*` once past retention. Electron stops appending to `server.log` in this PR.

## PM defaults

Pending Hung's confirmation. Until he says otherwise, the build uses these:

1. The size cap is a 256 MiB backstop inside the sweep. It is not a config key and not a control on Settings. The only retention control is the placeholder "Keep logs for".
2. The Logs page is admin-only, the same gate as Settings.
3. `postgres.log` is its own source, `postgres`, beside `server`, `http`, and `cli`.
4. Old `server.log` files are not imported. The sweep deletes `server.log` and `server.log.1` … `.3` once they are past retention.
5. Secret values shorter than 6 bytes are not masked. The reason is in Write-time mask.

## Line schema

One JSON object per line, produced by `tracing_subscriber::fmt().json()` with a UTC RFC3339 timer (`UtcTime` + `Rfc3339`). Add the `json` feature, and the `time` feature if `UtcTime` needs it. Do not write a second encoder.

```json
{"timestamp":"2026-10-14T03:35:01.123456Z","level":"INFO","target":"coppice_server::middleware::request_log","fields":{"message":"http request","source":"http","method":"GET","path":"/api/health","status":200,"duration_ms":4}}
```

| Field | Meaning |
| --- | --- |
| `timestamp` | UTC RFC3339. |
| `level` | Tracing level (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`). |
| `target` | Tracing target, kept for support. |
| `fields.message` | The event message. |
| `fields.source` | `server`, `http`, `cli`, or `postgres`. |
| `fields.run_id` | Run id when the event belongs to a run. Omitted otherwise. |
| `fields.ticket_id` | Ticket id when the caller knows it. Omitted otherwise. |
| `fields.stream` | `stdout` or `stderr`, on `cli` lines only. |

The request log already records `method`, `path`, `status`, and `duration_ms`. It keeps doing that, and it sets `source` to `http`. Other server events set `source` to `server`. A line in a `coppice.*.jsonl` file that is not valid JSON is skipped by the reader.

`postgres` lines are not written by this subscriber. They are lines read from `postgres.log`. See Where the files live.

## Where the files live

Desktop writes into the directory the server already calls `logs/` (`DataLayout.logs` in `server/src/desktop/layout.rs`):

| OS | Directory |
| --- | --- |
| macOS | `~/Library/Application Support/Coppice/logs` |
| Linux | `~/.config/Coppice/logs` |

That is Electron `userData` + `/logs`, the same folder as today's `server.log` and `postgres.log`.

A non-desktop process (Compose, or `coppice-server` without `desktop`) writes the same names under `[logs] dir`, default `./data/logs`. It also keeps the human-readable stdout subscriber, so a terminal and `docker logs` still show text. ANSI stays on only when stdout is a terminal. The piped stdout writer stays the flushing writer in `server/src/main.rs`.

| Name | What it is |
| --- | --- |
| `coppice.YYYY-MM-DD.jsonl` | The appender's daily file. Prefix `coppice`, suffix `jsonl`. The date is UTC, which is the calendar `tracing-appender` uses. |
| `current.jsonl` | Symlink to the file the appender has open, created with `latest_symlink("current.jsonl")`. |
| `coppice.YYYY-MM-DD.jsonl.zst` | A past day after the sweep compresses it. |

`postgres.log` stays the Postgres `-l` file at `DataLayout.pg_log` (`<data-dir>/logs/postgres.log` on desktop). The page reads it as source `postgres`. Pending Hung's confirmation. Each line is returned with `source` `postgres` and `message` set to the line. A leading timestamp, when the line has one, becomes `timestamp`. The line is not copied into the JSONL files, not compressed, and not deleted by the sweep: Postgres has the file open. Compose does not own that file, so the source is empty there.

`server.log` and `server.log.1` … `.3` are not imported and are not shown on the page. The sweep deletes them once they are past retention. Pending Hung's confirmation. See Migration from `server.log`.

## Writer

```rust
RollingFileAppender::builder()
    .rotation(Rotation::DAILY)
    .filename_prefix("coppice")
    .filename_suffix("jsonl")
    .latest_symlink("current.jsonl")
    .build(logs_dir)
```

Do not call `max_log_files`. The builder default is no limit, so the appender never deletes files.

`tracing-appender` only considers a file for deletion when it is a regular file and the name ends with the configured suffix (`jsonl`). After the sweep renames a day to `coppice.YYYY-MM-DD.jsonl.zst`, the name ends in `zst`, so the appender cannot see it, count it, or delete it. A limit set on the appender would only apply to plain files and would race the sweep. The sweep owns retention.

Both subscribers hang off one registry:

- Stdout: the current human formatter.
- File: JSON, ANSI off, wrapped in the same kind of flushing writer as today's pipe writer. Flush each line. Do not put a `BufWriter` or `tracing_appender::non_blocking` in front of the file. A crash must not leave the last line in a userspace buffer.

The env filter is unchanged: `RUST_LOG`, or `info` plus `rmcp=warn` unless `RUST_LOG` names `rmcp`.

The desktop ready line stays a plain `writeln!` to stdout (`server/src/desktop/mod.rs`). It is not a JSON log line. Electron still reads it from the pipe.

## CLI output

`server/src/providers/cli_runner.rs` pipes stdout and stderr. Today a non-JSON stdout line is skipped, and stderr is `tracing::debug!` (dropped by the default info filter) plus the first 40 lines kept in memory for error text. That 40-line tail stays. It is how connector errors are worded. It is not the log store.

Each stdout line, JSON or not, and each stderr line becomes one info event:

- `source` = `cli`
- `stream` = `stdout` or `stderr`
- `run_id` from the invocation
- `ticket_id` when the caller has it (`CliInvocation` gains an optional ticket id; the worker that already knows the ticket sets it)
- `message` = the line

Remove the debug stderr mirror so a line is not recorded twice. `run.log`, `process-stops.log`, `console.events.jsonl`, and `terminal.log` stay as they are. The Logs page does not read them.

OpenCode does not use that pump. `spawn` in `server/src/sessions/opencode_run_server.rs` leaves the child on the server's stdout and stderr. Pipe those two streams and emit the same `cli` events with the run id. The existing `log_launch` line stays.

The write-time mask below runs on the formatted line, so a secret the CLI prints is replaced before the file or the stdout pipe is written.

## Write-time mask

Keep a process list of `(name, value)` for every secret Coppice places on a child process:

- the per-run `COPPICE_MCP_TOKEN`
- connector auth env values it forwards (`install.auth_env` on the connector descriptors, including `ANTHROPIC_API_KEY` and `OPENAI_API_KEY`)
- plugin setting secrets it puts on a plugin MCP process
- forge tokens it puts on a git or forge command

Before a formatted line is written, to the file or to stdout, replace each value with `‹NAME›` (U+2039, the name, U+203A). Longest value first. Skip empty values. The value stays on the list until that child's pipes have been drained.

Skip a value shorter than 6 bytes. Pending Hung's confirmation. This is the same floor as `MIN_OUTPUT_SECRET_LEN` in `server/src/mcp/proxy/client.rs`. A secret that short is also a substring of ordinary log text: small numbers, short words, path segments. Replacing every occurrence would cut holes in lines that are not that secret. Six bytes is long enough that a match is the injected value. The disk test registers a value of at least 6 bytes and asserts those bytes never appear under the logs directory. A value of 5 bytes is left unchanged, so the floor is visible in the test.

`‹NAME›` and `‹redacted›` are log encoding, not UI copy.

Export runs the same substitution again, then replaces what is left of these shapes with `‹redacted›`:

- an `Authorization` header value
- a `Bearer` token
- a `Cookie` header value
- assignments of `token`, `password`, `secret`, `api_key`, and `access_token`

The request log still does not read the query, the body, cookies, or auth headers. `sanitize_failure`, `McpAccess`'s `Debug`, and the plugin MCP `Redactor` stay as they are.

Test: register a token longer than the floor, emit a line that contains it, and read every file in the logs directory. The token bytes are absent. `‹COPPICE_MCP_TOKEN›` is present.

## Read API

Admin only, the same gate as Settings. Pending Hung's confirmation. The session cookie is required. `POST` export also requires `X-CSRF-Token`.

`GET /api/logs`

| Query | Meaning |
| --- | --- |
| `source` | `server`, `http`, `cli`, or `postgres`. Omit for all four. |
| `level` | Minimum level. Default `info`. |
| `from`, `to` | UTC RFC3339 bounds. Omit for no bound. |
| `run` | Run id. |
| `q` | Case-insensitive substring of `fields.message` and of the raw line. |
| `cursor` | Opaque. Omit for the newest page. |
| `limit` | Default 200, maximum 1000. |

```json
{
  "lines": [
    {
      "timestamp": "2026-10-14T03:35:01.123456Z",
      "level": "INFO",
      "source": "http",
      "target": "coppice_server::middleware::request_log",
      "message": "http request",
      "runId": null,
      "ticketId": null,
      "stream": null,
      "fields": { "method": "GET", "path": "/api/health", "status": 200, "duration_ms": 4 }
    }
  ],
  "nextCursor": null
}
```

No index. For `server`, `http`, and `cli`, choose the `coppice.YYYY-MM-DD.jsonl` and `coppice.YYYY-MM-DD.jsonl.zst` files whose UTC dates overlap the range, or every retained file when there is no range. Walk dates from newest to oldest. Read a plain file as text. Inflate a compressed day with `zstd::stream::read::Decoder` for that request only, and do not write the plain text back to disk. For `postgres`, scan `postgres.log` the same way (newest page, then older). A query with no `source` merges that file with the JSONL days by timestamp, and lines with no parsed timestamp sort as a group after the dated lines from that file.

The first page is the newest `limit` matching lines. `nextCursor` names the oldest line on that page (file date + line index). The next request returns the next older page. A compressed day is scanned from the start of its stream. The boundary line is not returned twice.

`GET /api/logs/meta` returns `retentionDays`, the server package version, the build commit, `os`, `arch`, and `osVersion`. The page and the report link use it. The 256 MiB backstop is a server constant, not a field the client can change.

`osVersion` is `PRETTY_NAME` from `/etc/os-release` when that file exists, and `sw_vers -productVersion` on macOS. It is empty when that read fails. No extra crate.

## Live tail

`GET /ws/logs`, with the same session middleware as `/ws/events`.

The client sends one JSON text frame with the same filter fields as the query (`source`, `level`, `from`, `to`, `run`, `q`) and no cursor. The server polls the file `current.jsonl` points at every 500 ms, reads the new bytes, and sends matching lines as JSON text frames. It does not replay history. The page loads history with `GET /api/logs`.

Use a WebSocket because the SPA already authenticates `/ws/events` and `/ws/agent-runs/{run_id}/live` with the session cookie.

## Page

New route `/logs`, admin only, with a nav entry beside Settings. Pending Hung's confirmation.

Every quoted string in this section and in Export is a placeholder for Content. None of them is final copy.

| Slot | Placeholder |
| --- | --- |
| The only retention control | "Keep logs for" |
| Nav and page title | "Logs" |
| Source filter | "Source" |
| Level filter | "Level" |
| Time filter | "Time" |
| Run filter | "Run" |
| Text search | "Search" |
| Live tail control | "Live" |
| Export control | "Export" |
| Link beside Export | "Report a problem" |
| Link from a run to `/logs?run=<id>` | "Logs" |

Empty, loading, and error states need their own placeholders when the UI is built. This spec does not name them.

## Export and the report link

`POST /api/logs/export` takes the same filter as the current view and returns every matching line, not one page. Retention bounds how large that can be.

The zip contains:

- `logs.jsonl` — the matching lines after the export redaction pass
- `versions.txt` — `app_version`, `commit`, `os`, `arch`, `os_version`, and `shell_version` when the client sends the desktop version from `coppice:app-info`

The control beside Export is a link. Its label placeholder is "Report a problem". It opens a new browser tab at:

`https://github.com/fertile-org/coppice/issues/new?title=…&body=…`

The prefilled title and body are placeholders for Content. The body includes the app version, the OS version, and a note that the person has to attach the zip themselves. A URL cannot attach a file. The page does not call the GitHub API.

## Retention

`[logs]` in `config.toml`. Missing keys use the defaults.

```toml
[logs]
# How many days of logs to keep, including today. Default 14.
retention_days = 14
# Non-desktop only. Desktop always uses <data-dir>/logs.
# dir = "./data/logs"
```

`retention_days` is `1..=365`. There is no `max_bytes` key. Desktop ignores `dir` and uses `DataLayout.logs`.

The Settings page gains one numeric control. Its label placeholder is "Keep logs for". Pending Hung's confirmation. That is the only retention setting. Saving goes through the existing config-file save. Each sweep reads `retention_days` from the on-disk config, so a save applies on the next pass without a restart.

### Sweep

On process start, and every hour after that.

JSONL candidates are regular files matching `coppice.YYYY-MM-DD.jsonl` or `coppice.YYYY-MM-DD.jsonl.zst`. Use `symlink_metadata` and skip symlinks, so `current.jsonl` is never a candidate. `postgres.log` is not a candidate. `server.log`, `server.log.1`, `server.log.2`, and `server.log.3` are candidates for deletion only.

1. Compress a plain file whose UTC date is before today. Stream it with `zstd` at the crate's default level to `coppice.YYYY-MM-DD.jsonl.zst.tmp` in the same directory, sync, rename to `coppice.YYYY-MM-DD.jsonl.zst`, then delete the plain file. If a non-empty `.zst` for that day is already there, delete the plain file and do not compress again. Never compress today's file. The appender has it open.
2. Age. A file's age is today's UTC date minus the date in its name, in calendar days. Delete plain and compressed files whose age is greater than or equal to `retention_days`. With the default 14, ages 0 through 13 stay (fourteen dates, including today). A compressed file at age 14 is deleted.
3. Old `server.log` files. Pending Hung's confirmation. Do not import them. Delete `server.log` and `server.log.1` … `.3` when the file's mtime is at least `retention_days` old. While Electron is still appending, `server.log` stays fresh and is kept.
4. Size backstop. Pending Hung's confirmation. The cap is 256 MiB (`268435456` bytes), a constant in the sweep, not a setting. Sum the JSONL candidates that remain, plus any `server.log*` files that remain. While the sum is over 256 MiB, delete the oldest JSONL date's file and subtract its size. Never delete today's JSONL file. If today's file alone is over the cap, leave it and write a warning. Do not delete `postgres.log` to get under the cap.

The appender does not delete files. The sweep is the only thing that does.

## Crates

Use:

- `tracing` and `tracing-subscriber`, already in the server. Add `json` (and `time` if the UTC timer needs it).
- `tracing-appender` 0.2. `RollingFileAppender::builder`, daily rotation, `latest_symlink`. Leave the file limit unset.
- `zstd`, for the sweep and for reading a compressed day. `tracing-appender` cannot compress.

Rejected:

- **timberfs.** A FUSE filesystem with its own chunk store and a write-time index. The desktop app would depend on FUSE, and the logs would not be plain JSONL the sweep can delete.
- **logdb.** An embedded log database (segments, indexes, background threads, Linux-first). That is a second database next to Postgres, and the lines would not stay plain JSONL.
- **OpenTelemetry.** An SDK and a collector pipeline. It does not store or query a local file.
- **Loki.** A log server. This page reads files in the app data directory.
- **Vector.** A log router. That is another process, not a library that writes a file.

## Migration from `server.log`

Today Electron pipes the child's stdout and stderr into `<userData>/logs/server.log` and rotates that file at 10 MB, keeping three older files (`desktop/src/rotatingLog.mjs`, `desktop/src/serverProcess.mjs`). The failure window uses an in-memory ring (500 lines, and it shows the last 50). It does not read the file back.

After the server writes JSONL:

- Electron keeps the pipe, the ready-line parse, and the ring.
- Electron stops appending the pipe to `server.log`.
- `server.log`, `server.log.1`, `server.log.2`, and `server.log.3` are not imported. Pending Hung's confirmation. The sweep deletes each of them once its mtime is at least `retention_days` old.
- Lines printed before tracing starts (`eprintln!` in `server/src/main.rs`) stay in the ring only.
- `postgres.log` is unchanged on disk. The page reads it as source `postgres`.

## Test plan

- **Mask.** A line that contains an injected token is stored only as `‹NAME›`. A directory read of the logs folder does not contain the token bytes.
- **Compressed files expire.** A `coppice.YYYY-MM-DD.jsonl.zst` whose age is `>= retention_days` is deleted. A newer `.jsonl.zst` remains. Today's plain file is not compressed and is not deleted.
- **Size backstop.** The cap is the constant 256 MiB, with no Settings control and no config key. Over the cap, the oldest JSONL date is deleted first. Today's file remains even when the directory is still over the cap. `postgres.log` is not deleted to get under the cap.
- **Old `server.log`.** A file older than retention is deleted. Its bytes are not copied into a JSONL file. A `server.log` whose mtime is inside the window remains.
- **Appender limit unset.** Opening the appender does not delete an older `.jsonl.zst`.
- **Query.** A filter (source, level, run, text) returns a line from a compressed day. The cursor's next page does not repeat the boundary line.
- **Export.** The zip contains `logs.jsonl` and `versions.txt`. A buffer that still contains a registered secret does not contain it after the export pass.
- **Request log.** The existing middleware test still requires method, path, status, duration, and no query string.
- **CLI.** A non-JSON stdout line and a stderr line are stored with that run's id. OpenCode stdout and stderr are stored the same way, each with that run's id.
- **Postgres source.** A line from `postgres.log` is returned when `source` is `postgres`, and not returned when `source` is `cli`.
- **Page.** Filters, the tail socket, the export download, and the report link's query string. Any assertion on a label uses the placeholder in this spec and is marked temporary until Content replaces it.

Out of scope: importing `server.log` into the JSONL files, replacing the live run console or the per-run artifact files, an index, a Postgres log table, OpenTelemetry, Loki, and Vector.
