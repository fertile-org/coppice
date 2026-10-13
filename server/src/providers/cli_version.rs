//! `<cli> --version` once per binary path and mtime.
//!
//! The probe runs in a temp directory so a worktree-local env file cannot
//! change it. Only the version token is kept.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use coppice_connectors::version_token;

const PROBE_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Hash, Eq, PartialEq)]
struct CacheKey {
    path: PathBuf,
    mtime: Option<SystemTime>,
}

static CACHE: Mutex<Option<HashMap<CacheKey, String>>> = Mutex::new(None);

/// Shown when a version probe fails, so version-gated flags are not dropped quietly.
pub fn version_unavailable(program: &str, err: &str) -> String {
    format!(
        "could not read `{program}` version ({err}); version-specific flags were not applied"
    )
}

/// Version token. Failures are not cached.
pub async fn detect(program: &str) -> Result<String, String> {
    let host = crate::host_env::current();
    let resolved = if program.contains('/') || program.contains('\\') {
        PathBuf::from(program)
    } else {
        host.resolve(program).map_err(|err| err.to_string())?
    };
    if !resolved.is_file() {
        return Err(format!("`{program}` was not found"));
    }
    let mtime = std::fs::metadata(&resolved)
        .and_then(|meta| meta.modified())
        .ok();
    let key = CacheKey {
        path: resolved.clone(),
        mtime,
    };
    if let Some(hit) = cache_get(&key) {
        return Ok(hit);
    }
    let version = probe(&resolved).await?;
    cache_put(key, version.clone());
    Ok(version)
}

fn cache_get(key: &CacheKey) -> Option<String> {
    CACHE.lock().ok()?.as_ref()?.get(key).cloned()
}

fn cache_put(key: CacheKey, version: String) {
    if let Ok(mut guard) = CACHE.lock() {
        guard.get_or_insert_with(HashMap::new).insert(key, version);
    }
}

async fn probe(program: &Path) -> Result<String, String> {
    let dir = tempfile::tempdir().map_err(|err| err.to_string())?;
    let host = crate::host_env::current();
    let mut cmd = tokio::process::Command::new(program);
    cmd.arg("--version")
        .current_dir(dir.path())
        .env("PATH", &host.path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if !host.home.as_os_str().is_empty() {
        cmd.env("HOME", &host.home);
    }
    let child = cmd
        .spawn()
        .map_err(|err| format!("failed to spawn `{}`: {err}", program.display()))?;
    let finished = tokio::time::timeout(PROBE_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| {
            format!(
                "`{} --version` timed out after {}s",
                program.display(),
                PROBE_TIMEOUT.as_secs()
            )
        })?
        .map_err(|err| err.to_string())?;
    if !finished.status.success() {
        let line = crate::host_env::first_stderr_line(&finished.stderr)
            .or_else(|| crate::host_env::first_stderr_line(&finished.stdout))
            .unwrap_or_else(|| format!("exit {}", finished.status));
        return Err(line);
    }
    let mut text = String::from_utf8_lossy(&finished.stdout).into_owned();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&finished.stderr).into_owned();
    }
    version_token(&text)
        .map(str::to_string)
        .ok_or_else(|| "no version token in --version output".to_string())
}
