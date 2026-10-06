//! Process groups for agent CLI trees.
//!
//! Each agent Coppice spawns — claude, codex, cursor, opencode, kilo — is placed
//! in its own session with `setsid`. The pid, pgid, kernel start token, and
//! command are appended to `agent-processes.json` under the artifacts dir.
//! Stopping a run signals that group: SIGTERM, then SIGKILL after [`TERM_GRACE`].
//! Server shutdown stops every live group. The next launch reaps records left
//! by a crash, and only after the pid's start token and command still match.
//! A reused pid is left alone.
//!
//! Linux reads identity from `/proc/<pid>/stat` (starttime ticks) and
//! `/proc/<pid>/cmdline`. macOS has no `/proc`, so this same `cfg(unix)` code
//! falls back to `ps -o lstart=,command=`. CI runs the `/proc` path. The `ps`
//! parser and `ps` group scan are unit-tested on Linux; live macOS process
//! checks are not run in CI.
//!
//! Grandchildren that call `setsid` themselves leave the group and are not
//! stopped. Dev servers and shells started in the agent's session are.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

/// How long a group has to exit after SIGTERM before SIGKILL.
pub const TERM_GRACE: Duration = Duration::from_secs(3);

static ACTIVE: OnceLock<Mutex<std::sync::Arc<ProcessRegistry>>> = OnceLock::new();

/// `{artifacts_dir}/agent-processes.json`.
pub fn registry_path(artifacts_dir: &str) -> PathBuf {
    PathBuf::from(artifacts_dir).join("agent-processes.json")
}

/// Installs the process-wide registry the runners and shutdown share.
pub fn install(path: impl Into<PathBuf>) -> std::sync::Arc<ProcessRegistry> {
    let registry = ProcessRegistry::open(path);
    let slot = ACTIVE.get_or_init(|| Mutex::new(std::sync::Arc::clone(&registry)));
    *slot.lock().unwrap_or_else(|err| err.into_inner()) = std::sync::Arc::clone(&registry);
    registry
}

pub fn install_from_artifacts(artifacts_dir: &str) -> std::sync::Arc<ProcessRegistry> {
    install(registry_path(artifacts_dir))
}

/// The installed registry, or a per-process file under the temp dir before
/// `install` runs (tests and any spawn that races startup).
pub fn active() -> std::sync::Arc<ProcessRegistry> {
    if let Some(slot) = ACTIVE.get() {
        return std::sync::Arc::clone(&slot.lock().unwrap_or_else(|err| err.into_inner()));
    }
    let registry = ProcessRegistry::open(std::env::temp_dir().join(format!(
        "coppice-agent-processes-{}.json",
        std::process::id()
    )));
    let slot = ACTIVE.get_or_init(|| Mutex::new(std::sync::Arc::clone(&registry)));
    std::sync::Arc::clone(&slot.lock().unwrap_or_else(|err| err.into_inner()))
}

/// Run ids recorded on process trees this process still tracks.
pub fn tracked_run_ids() -> Vec<String> {
    active().tracked_run_ids()
}

/// SIGTERM then SIGKILL for every agent tree this process still tracks.
pub async fn shutdown_running_agents() {
    active().stop_all("server shutdown").await;
}

/// Stops agent trees recorded by a previous process that did not get to shut
/// down. Identity is checked first; a reused pid is not signaled.
pub async fn reap_orphaned_agents() {
    let reports = active().reap_orphans().await;
    let stopped = reports
        .iter()
        .filter(|report| report.outcome == "terminated" || report.outcome == "killed")
        .count();
    let skipped = reports
        .iter()
        .filter(|report| report.outcome == "skipped-identity")
        .count();
    if stopped > 0 || skipped > 0 {
        tracing::info!(stopped, skipped, "reaped orphaned agent processes");
    }
}

#[derive(Debug, Clone)]
pub struct SpawnMeta {
    pub command: String,
    pub label: &'static str,
    pub run_id: Option<String>,
    pub task_log_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopReport {
    pub pid: u32,
    pub pgid: u32,
    pub command: String,
    pub run_id: Option<String>,
    pub outcome: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessRecord {
    pid: u32,
    pgid: u32,
    parent_pid: u32,
    identity_kind: String,
    start_token: String,
    command: String,
    label: String,
    run_id: Option<String>,
    #[serde(default)]
    task_log_dir: Option<PathBuf>,
}

struct Identity {
    kind: &'static str,
    token: String,
    command: String,
}

enum Claim {
    Ours,
    Orphans,
    Reused,
    Gone,
}

enum SignalError {
    Refused,
    Gone,
    Io,
}

pub struct ProcessRegistry {
    path: PathBuf,
    log_path: PathBuf,
    term_grace: Duration,
    records: Mutex<Vec<ProcessRecord>>,
}

impl ProcessRegistry {
    pub fn open(path: impl Into<PathBuf>) -> std::sync::Arc<Self> {
        Self::with_grace(path, TERM_GRACE)
    }

    pub fn with_grace(path: impl Into<PathBuf>, term_grace: Duration) -> std::sync::Arc<Self> {
        let path = path.into();
        let log_path = path.with_file_name("agent-process.log");
        std::sync::Arc::new(Self {
            path,
            log_path,
            term_grace,
            records: Mutex::new(Vec::new()),
        })
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    pub fn spawn(
        self: &std::sync::Arc<Self>,
        cmd: &mut Command,
        meta: SpawnMeta,
    ) -> io::Result<TrackedProcess> {
        // SAFETY: `setsid(2)` is async-signal-safe and runs in the forked child
        // before exec, so the agent becomes its own session and process group.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        let mut child = cmd.spawn()?;
        let pid = child
            .id()
            .ok_or_else(|| io::Error::other("spawned agent process did not report a pid"))?;
        // `setsid` makes the pgid equal the pid. If the process has already
        // exited, still record that pgid so a grandchild it left behind can be
        // reaped. Never record the server's own group.
        let pgid = pgid_of(pid).unwrap_or(pid);
        if pgid <= 1 || pgid == own_pgrp() {
            let _ = child.start_kill();
            return Err(io::Error::other(
                "agent process did not enter its own process group",
            ));
        }
        let identity = read_identity(pid);
        let record = ProcessRecord {
            pid,
            pgid,
            parent_pid: std::process::id(),
            identity_kind: identity
                .as_ref()
                .map(|id| id.kind.to_string())
                .unwrap_or_default(),
            start_token: identity
                .as_ref()
                .map(|id| id.token.clone())
                .unwrap_or_default(),
            command: identity
                .map(|id| id.command)
                .unwrap_or_else(|| meta.command.clone()),
            label: meta.label.to_string(),
            run_id: meta.run_id,
            task_log_dir: meta.task_log_dir,
        };
        self.insert(&record);
        Ok(TrackedProcess {
            child: Some(child),
            record,
            armed: true,
            registry: std::sync::Arc::clone(self),
        })
    }

    pub async fn stop_all(&self, reason: &str) -> Vec<StopReport> {
        self.stop_records(self.snapshot(), reason).await
    }

    /// Distinct non-empty run ids on the in-memory records.
    pub fn tracked_run_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for record in self.lock().iter() {
            let Some(run_id) = record.run_id.as_ref() else {
                continue;
            };
            if !run_id.is_empty() && !ids.contains(run_id) {
                ids.push(run_id.clone());
            }
        }
        ids
    }

    pub async fn stop_pid(&self, pid: u32, reason: &str) {
        if let Some(record) = self.find(pid) {
            self.stop_records(vec![record], reason).await;
        }
    }

    /// SIGKILL when a handle is dropped or a lease is abandoned. Still refuses
    /// a pid whose start token or command no longer matches.
    pub fn kill_pid_now(&self, pid: u32, reason: &str) {
        if let Some(record) = self.find(pid) {
            self.kill_now(&record, reason);
        }
    }

    pub async fn reap_orphans(&self) -> Vec<StopReport> {
        match load(&self.path) {
            Ok(records) => {
                *self.lock() = records;
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Vec::new(),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    path = %self.path.display(),
                    "cannot read agent process records; not killing anything"
                );
                return Vec::new();
            }
        }
        self.stop_all("orphan on startup").await
    }

    fn kill_now(&self, record: &ProcessRecord, reason: &str) {
        match classify(record) {
            Claim::Ours | Claim::Orphans => {
                let _ = signal_group(record.pgid, libc::SIGKILL);
                try_reap(record.pid);
                self.note(record, "killed", reason);
            }
            Claim::Reused => {
                self.note(record, "skipped-identity", reason);
            }
            Claim::Gone => {
                self.remove_and_persist(record.pid);
            }
        }
    }

    async fn stop_records(&self, records: Vec<ProcessRecord>, reason: &str) -> Vec<StopReport> {
        let mut signaled = Vec::new();
        let mut reports = Vec::new();
        for record in records {
            match classify(&record) {
                Claim::Gone => reports.push(self.note(&record, "already-gone", reason)),
                Claim::Reused => reports.push(self.note(&record, "skipped-identity", reason)),
                Claim::Ours | Claim::Orphans => match signal_group(record.pgid, libc::SIGTERM) {
                    Err(SignalError::Refused | SignalError::Io) => {
                        reports.push(self.note(&record, "skipped-identity", reason));
                    }
                    Err(SignalError::Gone) => {
                        reports.push(self.note(&record, "already-gone", reason));
                    }
                    Ok(()) => signaled.push(record),
                },
            }
        }
        wait_until_quiet(&signaled, self.term_grace).await;
        let mut killed = Vec::new();
        for record in signaled {
            if !group_has_live_members(record.pgid) {
                try_reap(record.pid);
                reports.push(self.note(&record, "terminated", reason));
                continue;
            }
            match classify(&record) {
                Claim::Reused => reports.push(self.note(&record, "skipped-identity", reason)),
                Claim::Gone => reports.push(self.note(&record, "terminated", reason)),
                Claim::Ours | Claim::Orphans => {
                    if signal_group(record.pgid, libc::SIGKILL).is_ok() {
                        killed.push(record.clone());
                    }
                    try_reap(record.pid);
                    reports.push(self.note(&record, "killed", reason));
                }
            }
        }
        wait_until_quiet(&killed, Duration::from_millis(500)).await;
        reports
    }

    fn note(&self, record: &ProcessRecord, outcome: &str, reason: &str) -> StopReport {
        let report = StopReport {
            pid: record.pid,
            pgid: record.pgid,
            command: record.command.clone(),
            run_id: record.run_id.clone(),
            outcome: outcome.to_string(),
            reason: reason.to_string(),
        };
        let line = serde_json::json!({
            "pid": report.pid,
            "pgid": report.pgid,
            "command": report.command,
            "runId": report.run_id,
            "label": record.label,
            "outcome": report.outcome,
            "reason": report.reason,
        });
        if let Ok(text) = serde_json::to_string(&line) {
            append_line(&self.log_path, &text);
            if let Some(dir) = &record.task_log_dir {
                append_line(&dir.join("process-stops.log"), &text);
            }
        }
        match outcome {
            "skipped-identity" => tracing::warn!(
                target: "coppice::agent_process",
                pid = report.pid,
                pgid = report.pgid,
                command = %report.command,
                run_id = report.run_id.as_deref().unwrap_or(""),
                outcome,
                reason,
                "left agent pid alone; identity did not match"
            ),
            "terminated" | "killed" => tracing::info!(
                target: "coppice::agent_process",
                pid = report.pid,
                pgid = report.pgid,
                command = %report.command,
                run_id = report.run_id.as_deref().unwrap_or(""),
                outcome,
                reason,
                "stopped agent process tree"
            ),
            _ => tracing::debug!(
                target: "coppice::agent_process",
                pid = report.pid,
                pgid = report.pgid,
                command = %report.command,
                run_id = report.run_id.as_deref().unwrap_or(""),
                outcome,
                reason,
                "agent process tree already gone"
            ),
        }
        self.remove_and_persist(record.pid);
        report
    }

    fn insert(&self, record: &ProcessRecord) {
        let mut records = self.lock();
        records.push(record.clone());
        if let Err(err) = persist(&self.path, &records) {
            tracing::warn!(
                error = %err,
                path = %self.path.display(),
                pid = record.pid,
                "failed to persist agent process record"
            );
        }
    }

    fn remove_and_persist(&self, pid: u32) {
        let mut records = self.lock();
        records.retain(|record| record.pid != pid);
        if let Err(err) = persist(&self.path, &records) {
            tracing::warn!(error = %err, pid, "failed to update agent process records");
        }
    }

    fn snapshot(&self) -> Vec<ProcessRecord> {
        self.lock().clone()
    }

    fn find(&self, pid: u32) -> Option<ProcessRecord> {
        self.lock().iter().find(|record| record.pid == pid).cloned()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<ProcessRecord>> {
        self.records.lock().unwrap_or_else(|err| err.into_inner())
    }
}

pub struct TrackedProcess {
    child: Option<Child>,
    record: ProcessRecord,
    armed: bool,
    registry: std::sync::Arc<ProcessRegistry>,
}

impl TrackedProcess {
    pub fn id(&self) -> u32 {
        self.record.pid
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.as_mut()?.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.as_mut()?.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.as_mut()?.stderr.take()
    }

    pub async fn shutdown(&mut self, reason: &str) {
        self.registry.stop_pid(self.record.pid, reason).await;
        if let Some(child) = self.child.as_mut() {
            let _ = child.wait().await;
        }
        self.armed = false;
    }

    pub async fn wait_and_reap(&mut self, reason: &str) -> io::Result<std::process::ExitStatus> {
        let status = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("agent process handle missing"))?
            .wait()
            .await?;
        self.registry.stop_pid(self.record.pid, reason).await;
        self.armed = false;
        Ok(status)
    }

    /// Leave the process running and the registry record on disk. Used to
    /// simulate a crash. The drop guard will not signal the group.
    pub fn abandon(mut self) -> u32 {
        self.armed = false;
        self.record.pid
    }

    /// Hand the child to a caller that stops the group itself (OpenCode).
    /// The registry record stays until `stop_pid` or `kill_pid_now`.
    pub fn detach(mut self) -> (Child, u32) {
        self.armed = false;
        let child = self
            .child
            .take()
            .expect("tracked child is present until detach");
        (child, self.record.pid)
    }
}

impl Drop for TrackedProcess {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.armed = false;
        self.registry
            .kill_now(&self.record, "agent process dropped");
    }
}

/// True when `pid` is running. Zombies count as gone. On macOS, where `/proc`
/// is absent, `kill(pid, 0)` is the check.
pub fn process_alive(pid: u32) -> bool {
    if pid <= 1 {
        return false;
    }
    if let Some(state) = proc_state(pid) {
        return state != 'Z' && state != 'X';
    }
    if Path::new("/proc/self/stat").is_file() {
        return false;
    }
    let rc = unsafe { libc::kill(pid as i32, 0) };
    if rc == 0 {
        return true;
    }
    io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn own_pgrp() -> u32 {
    // SAFETY: `getpgrp(2)` only reads the calling process's group id.
    let pgid = unsafe { libc::getpgrp() };
    if pgid < 0 {
        0
    } else {
        pgid as u32
    }
}

fn pgid_of(pid: u32) -> Option<u32> {
    // SAFETY: `getpgid(2)` only reads the given process's group id.
    let pgid = unsafe { libc::getpgid(pid as i32) };
    if pgid <= 1 {
        None
    } else {
        Some(pgid as u32)
    }
}

fn signal_group(pgid: u32, sig: i32) -> Result<(), SignalError> {
    if pgid <= 1 || pgid > i32::MAX as u32 || pgid == own_pgrp() {
        tracing::error!(
            pgid,
            "refusing to signal a process group Coppice does not own"
        );
        return Err(SignalError::Refused);
    }
    // SAFETY: `pgid` is greater than 1 and is not this process's group, so the
    // negative pid signals only that session's processes.
    let rc = unsafe { libc::kill(-(pgid as i32), sig) };
    if rc == 0 {
        return Ok(());
    }
    let err = io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        Err(SignalError::Gone)
    } else {
        tracing::warn!(pgid, error = %err, "failed to signal agent process group");
        Err(SignalError::Io)
    }
}

fn try_reap(pid: u32) {
    // SAFETY: `waitpid` with `WNOHANG` reaps only this pid, and only if it is
    // our child. An unrelated pid returns `ECHILD` and changes nothing.
    unsafe {
        let mut status = 0;
        libc::waitpid(pid as i32, &mut status, libc::WNOHANG);
    }
}

async fn wait_until_quiet(records: &[ProcessRecord], grace: Duration) {
    if records.is_empty() {
        return;
    }
    let deadline = tokio::time::Instant::now() + grace;
    loop {
        if records
            .iter()
            .all(|record| !group_has_live_members(record.pgid))
        {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn classify(record: &ProcessRecord) -> Claim {
    if record.pgid <= 1 || record.pgid == own_pgrp() {
        return Claim::Reused;
    }
    if let Some(identity) = read_identity(record.pid) {
        if same_process(record, &identity) {
            return Claim::Ours;
        }
        if record.start_token.is_empty()
            && record.parent_pid == std::process::id()
            && ppid_of(record.pid) == Some(record.parent_pid)
        {
            return Claim::Ours;
        }
        return Claim::Reused;
    }
    if group_has_live_members(record.pgid) && !record.start_token.is_empty() {
        return Claim::Orphans;
    }
    if group_has_live_members(record.pgid)
        && record.start_token.is_empty()
        && record.parent_pid == std::process::id()
    {
        return Claim::Orphans;
    }
    Claim::Gone
}

fn same_process(record: &ProcessRecord, identity: &Identity) -> bool {
    !record.start_token.is_empty()
        && record.identity_kind == identity.kind
        && record.start_token == identity.token
        && !record.command.is_empty()
        && record.command == identity.command
}

fn read_identity(pid: u32) -> Option<Identity> {
    proc_identity(pid).or_else(|| ps_identity(pid))
}

fn proc_identity(pid: u32) -> Option<Identity> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let parsed = parse_proc_stat(&stat)?;
    let command = proc_command(pid).unwrap_or(parsed.comm);
    if command.is_empty() {
        return None;
    }
    Some(Identity {
        kind: "proc-ticks",
        token: parsed.starttime.to_string(),
        command,
    })
}

fn proc_command(pid: u32) -> Option<String> {
    let raw = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
    let argv0 = String::from_utf8_lossy(&raw[..end]).trim().to_string();
    if argv0.is_empty() {
        None
    } else {
        Some(argv0)
    }
}

fn proc_state(pid: u32) -> Option<char> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_proc_stat(&stat).map(|parsed| parsed.state)
}

fn ppid_of(pid: u32) -> Option<u32> {
    if let Some(parsed) = fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| parse_proc_stat(&stat))
    {
        return Some(parsed.ppid);
    }
    ps_ppid(pid)
}

struct ProcStat {
    comm: String,
    state: char,
    ppid: u32,
    pgrp: u32,
    starttime: u64,
}

fn parse_proc_stat(stat: &str) -> Option<ProcStat> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    if close < open {
        return None;
    }
    let comm = stat[open + 1..close].to_string();
    let fields: Vec<&str> = stat[close + 1..].split_whitespace().collect();
    let state = fields.first()?.chars().next()?;
    let ppid = fields.get(1)?.parse().ok()?;
    let pgrp = fields.get(2)?.parse().ok()?;
    let starttime = fields.get(19)?.parse().ok()?;
    Some(ProcStat {
        comm,
        state,
        ppid,
        pgrp,
        starttime,
    })
}

fn proc_group_members(pgid: u32) -> Vec<u32> {
    let mut members = Vec::new();
    let Ok(dir) = fs::read_dir("/proc") else {
        return members;
    };
    for entry in dir.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let Some(parsed) = parse_proc_stat(&stat) else {
            continue;
        };
        if parsed.pgrp == pgid && parsed.state != 'Z' && parsed.state != 'X' {
            members.push(pid);
        }
    }
    members
}

fn group_has_live_members(pgid: u32) -> bool {
    !live_members(pgid).is_empty()
}

fn live_members(pgid: u32) -> Vec<u32> {
    if Path::new("/proc/self/stat").is_file() {
        proc_group_members(pgid)
    } else {
        ps_group_members(pgid)
    }
}

fn ps_identity(pid: u32) -> Option<Identity> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart=", "-o", "command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    let (lstart, command_line) = split_ps_lstart(line)?;
    let command = command_line.split_whitespace().next()?.to_string();
    if command.is_empty() {
        return None;
    }
    Some(Identity {
        kind: "ps-lstart",
        token: lstart.to_string(),
        command,
    })
}

fn ps_ppid(pid: u32) -> Option<u32> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "ppid="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn ps_group_members(pgid: u32) -> Vec<u32> {
    let Ok(output) = std::process::Command::new("ps")
        .args(["-ax", "-o", "pid=,pgid=,state="])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let mut members = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut cols = line.split_whitespace();
        let Ok(pid) = cols.next().unwrap_or("").parse::<u32>() else {
            continue;
        };
        let Ok(group) = cols.next().unwrap_or("").parse::<u32>() else {
            continue;
        };
        let state = cols.next().unwrap_or("");
        if group == pgid && !state.starts_with('Z') {
            members.push(pid);
        }
    }
    members
}

/// `ps -o lstart=` is `Tue Oct  6 15:57:05 2026` followed by the command.
/// The time and year anchor the split so a padded day still parses.
fn split_ps_lstart(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index + 13 <= bytes.len() {
        if is_time_then_year(&bytes[index..]) {
            let end = index + 13;
            if end < line.len() && line[end..].starts_with(' ') {
                let lstart = line[..end].trim();
                let command = line[end..].trim();
                if !lstart.is_empty() && !command.is_empty() {
                    return Some((lstart, command));
                }
            }
        }
        index += 1;
    }
    None
}

fn is_time_then_year(bytes: &[u8]) -> bool {
    bytes.len() >= 13
        && bytes[2] == b':'
        && bytes[5] == b':'
        && bytes[8] == b' '
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && bytes[7].is_ascii_digit()
        && bytes[9].is_ascii_digit()
        && bytes[10].is_ascii_digit()
        && bytes[11].is_ascii_digit()
        && bytes[12].is_ascii_digit()
}

fn load(path: &Path) -> io::Result<Vec<ProcessRecord>> {
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&raw).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn persist(path: &Path, records: &[ProcessRecord]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let raw = serde_json::to_vec_pretty(records)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn append_line(path: &Path, line: &str) {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = fs::create_dir_all(parent);
        }
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    struct KillGroup(u32);

    impl Drop for KillGroup {
        fn drop(&mut self) {
            if self.0 > 1 && self.0 != own_pgrp() {
                unsafe {
                    libc::kill(-(self.0 as i32), libc::SIGKILL);
                }
            }
        }
    }

    fn registry(dir: &Path) -> std::sync::Arc<ProcessRegistry> {
        ProcessRegistry::with_grace(dir.join("agent-processes.json"), Duration::from_millis(400))
    }

    fn meta(dir: &Path) -> SpawnMeta {
        SpawnMeta {
            command: "sh".into(),
            label: "cli",
            run_id: Some("run-42".into()),
            task_log_dir: Some(dir.join("task")),
        }
    }

    fn shell(script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }

    async fn wait_dead(pid: u32) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            if !process_alive(pid) {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }

    async fn wait_for_file(path: &Path) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            if fs::read_to_string(path).is_ok_and(|text| !text.trim().is_empty()) {
                return;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("file was not written: {}", path.display());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn read_child_pid(path: &Path) -> u32 {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(text) = fs::read_to_string(path) {
                if let Ok(pid) = text.trim().parse::<u32>() {
                    return pid;
                }
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("child pid file was not written: {}", path.display());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[test]
    fn parse_proc_stat_reads_comm_with_spaces_and_starttime() {
        let stat = "123 (my cmd) S 1 99 99 0 0 0 0 0 0 0 0 0 0 0 20 0 1 0 4242 0";
        let parsed = parse_proc_stat(stat).expect("parse");
        assert_eq!(parsed.comm, "my cmd");
        assert_eq!(parsed.state, 'S');
        assert_eq!(parsed.ppid, 1);
        assert_eq!(parsed.pgrp, 99);
        assert_eq!(parsed.starttime, 4242);
    }

    #[test]
    fn split_ps_lstart_keeps_the_padded_day_and_the_command() {
        let line = "Tue Oct  6 15:57:05 2026 /bin/sleep 1000";
        let (lstart, command) = split_ps_lstart(line).expect("split");
        assert_eq!(lstart, "Tue Oct  6 15:57:05 2026");
        assert_eq!(command, "/bin/sleep 1000");
    }

    #[test]
    fn signal_group_refuses_the_server_process_group() {
        let err = signal_group(own_pgrp(), libc::SIGTERM);
        assert!(matches!(err, Err(SignalError::Refused)));
        assert!(process_alive(std::process::id()));
    }

    #[tokio::test]
    async fn ps_identity_and_group_scan_see_a_live_process() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("sleep");
        let pid = child.id();
        let identity = ps_identity(pid).expect("ps identity");
        assert!(!identity.token.is_empty());
        assert!(
            identity.command == "sleep" || identity.command.ends_with("/sleep"),
            "{}",
            identity.command
        );
        let pgid = pgid_of(pid).expect("pgid");
        assert!(ps_group_members(pgid).contains(&pid));
        let _ = child.kill();
        let _ = child.wait();
    }

    #[tokio::test]
    async fn server_shutdown_stops_background_child_and_logs_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let registry = registry(dir.path());
        let pid_file = dir.path().join("child.pid");
        // 20s outlives the grace period. stop_all signals the group; it does
        // not block until this sleep exits.
        let script = format!(
            "sleep 20 >/dev/null 2>&1 & echo $! > '{}'; wait",
            pid_file.display()
        );
        let tracked = registry
            .spawn(&mut shell(&script), meta(dir.path()))
            .expect("spawn");
        let leader = tracked.id();
        let _cleanup = KillGroup(tracked.record.pgid);
        assert_ne!(tracked.record.pgid, own_pgrp());
        let child = read_child_pid(&pid_file).await;
        assert!(process_alive(child));

        let reports = registry.stop_all("server shutdown").await;

        assert!(wait_dead(leader).await, "leader {leader} still running");
        assert!(wait_dead(child).await, "background {child} still running");
        assert!(
            reports
                .iter()
                .any(|report| report.pid == leader && report.reason == "server shutdown"),
            "{reports:?}"
        );
        let task_log = fs::read_to_string(dir.path().join("task").join("process-stops.log"))
            .expect("task log");
        assert!(task_log.contains("\"runId\":\"run-42\""), "{task_log}");
        assert!(
            task_log.contains("\"outcome\":\"terminated\"")
                || task_log.contains("\"outcome\":\"killed\""),
            "{task_log}"
        );
        let agent_log = fs::read_to_string(registry.log_path()).expect("agent log");
        assert!(agent_log.contains(&leader.to_string()), "{agent_log}");
    }

    #[tokio::test]
    async fn restart_reaps_orphaned_background_child() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("agent-processes.json");
        let registry = ProcessRegistry::with_grace(&path, Duration::from_millis(400));
        let pid_file = dir.path().join("child.pid");
        let script = format!(
            "sleep 20 >/dev/null 2>&1 & echo $! > '{}'; wait",
            pid_file.display()
        );
        let tracked = registry
            .spawn(&mut shell(&script), meta(dir.path()))
            .expect("spawn");
        let leader = tracked.id();
        let pgid = tracked.record.pgid;
        let _cleanup = KillGroup(pgid);
        let child = read_child_pid(&pid_file).await;
        tracked.abandon();
        assert!(process_alive(leader));
        assert!(process_alive(child));

        let restarted = ProcessRegistry::with_grace(&path, Duration::from_millis(400));
        let reports = restarted.reap_orphans().await;

        assert!(wait_dead(leader).await, "leader {leader} survived reap");
        assert!(wait_dead(child).await, "background {child} survived reap");
        assert!(
            reports.iter().any(|report| {
                report.pid == leader
                    && report.reason == "orphan on startup"
                    && (report.outcome == "terminated" || report.outcome == "killed")
            }),
            "{reports:?}"
        );
    }

    #[tokio::test]
    async fn restart_reaps_a_child_whose_leader_already_exited() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("agent-processes.json");
        let registry = ProcessRegistry::with_grace(&path, Duration::from_millis(400));
        let pid_file = dir.path().join("child.pid");
        let script = format!(
            "sleep 20 >/dev/null 2>&1 & echo $! > '{}'",
            pid_file.display()
        );
        let tracked = registry
            .spawn(&mut shell(&script), meta(dir.path()))
            .expect("spawn");
        let leader = tracked.id();
        let _cleanup = KillGroup(tracked.record.pgid);
        let child = read_child_pid(&pid_file).await;
        tracked.abandon();
        assert!(wait_dead(leader).await, "leader should exit on its own");
        assert!(
            process_alive(child),
            "background child exited with the leader"
        );

        let reports = ProcessRegistry::with_grace(&path, Duration::from_millis(400))
            .reap_orphans()
            .await;

        assert!(wait_dead(child).await, "background {child} survived reap");
        assert!(
            reports.iter().any(|report| {
                report.pid == leader
                    && (report.outcome == "terminated" || report.outcome == "killed")
            }),
            "{reports:?}"
        );
    }

    #[tokio::test]
    async fn reap_does_not_kill_a_reused_pid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("agent-processes.json");
        let registry = ProcessRegistry::open(&path);
        let tracked = registry
            .spawn(&mut shell("sleep 20"), meta(dir.path()))
            .expect("spawn");
        let leader = tracked.id();
        let pgid = tracked.record.pgid;
        let _cleanup = KillGroup(pgid);
        tracked.abandon();
        assert!(process_alive(leader));

        let mut records: Vec<ProcessRecord> =
            serde_json::from_str(&fs::read_to_string(&path).expect("records")).expect("json");
        records[0].start_token = "not-the-recorded-start".into();
        fs::write(&path, serde_json::to_vec_pretty(&records).expect("encode")).expect("write");

        let restarted = ProcessRegistry::open(&path);
        let reports = restarted.reap_orphans().await;

        assert!(
            process_alive(leader),
            "reused-looking pid {leader} was killed"
        );
        assert!(
            reports
                .iter()
                .any(|report| report.pid == leader && report.outcome == "skipped-identity"),
            "{reports:?}"
        );
        let task_log = fs::read_to_string(dir.path().join("task").join("process-stops.log"))
            .expect("task log");
        assert!(task_log.contains("skipped-identity"), "{task_log}");
    }

    #[tokio::test]
    async fn reap_of_a_corrupt_record_file_does_not_kill_a_bystander() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("agent-processes.json");
        fs::write(&path, "{not json").expect("write");
        let mut bystander = std::process::Command::new("sleep")
            .arg("1000")
            .spawn()
            .expect("sleep");
        let pid = bystander.id();

        let reports = ProcessRegistry::open(&path).reap_orphans().await;

        assert!(reports.is_empty());
        assert!(process_alive(pid));
        let _ = bystander.kill();
        let _ = bystander.wait();
    }

    #[tokio::test]
    async fn stopping_one_group_leaves_an_untracked_process_running() {
        let dir = tempfile::tempdir().expect("tempdir");
        let registry = registry(dir.path());
        let tracked = registry
            .spawn(&mut shell("sleep 20"), meta(dir.path()))
            .expect("spawn");
        let _cleanup = KillGroup(tracked.record.pgid);
        let mut bystander = std::process::Command::new("sleep")
            .arg("1000")
            .spawn()
            .expect("sleep");
        let bystander_pid = bystander.id();

        registry.stop_all("server shutdown").await;

        assert!(wait_dead(tracked.id()).await);
        assert!(process_alive(bystander_pid));
        let _ = bystander.kill();
        let _ = bystander.wait();
    }

    #[tokio::test]
    async fn term_ignored_is_sigkilled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let registry = registry(dir.path());
        let ready = dir.path().join("ready");
        let script = format!(
            "trap '' TERM; echo ready > '{}'; while true; do sleep 30; done",
            ready.display()
        );
        let tracked = registry
            .spawn(&mut shell(&script), meta(dir.path()))
            .expect("spawn");
        let leader = tracked.id();
        let _cleanup = KillGroup(tracked.record.pgid);
        wait_for_file(&ready).await;
        let started = std::time::Instant::now();

        let reports = registry.stop_all("cancelled").await;

        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(wait_dead(leader).await, "leader {leader} ignored SIGKILL");
        assert!(
            reports
                .iter()
                .any(|report| report.pid == leader && report.outcome == "killed"),
            "{reports:?}"
        );
    }

    /// `install` replaces the process-wide registry. Put the default temp file
    /// back so a later test in this process does not keep a deleted path.
    struct RestoreRegistry;

    impl Drop for RestoreRegistry {
        fn drop(&mut self) {
            install(std::env::temp_dir().join(format!(
                "coppice-agent-processes-{}.json",
                std::process::id()
            )));
        }
    }

    #[tokio::test]
    async fn shutdown_entry_point_stops_a_tracked_tree() {
        let _guard = GLOBAL_TEST_LOCK
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let _restore = RestoreRegistry;
        let dir = tempfile::tempdir().expect("tempdir");
        install(dir.path().join("agent-processes.json"));
        let tracked = active()
            .spawn(&mut shell("sleep 20"), meta(dir.path()))
            .expect("spawn");
        let leader = tracked.id();
        let _cleanup = KillGroup(tracked.record.pgid);

        shutdown_running_agents().await;

        assert!(wait_dead(leader).await, "leader {leader} survived shutdown");
    }

    static GLOBAL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}
