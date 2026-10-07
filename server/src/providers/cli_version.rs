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

/// Version token, or `"unknown"` when the binary is missing or the output has none.
pub async fn detect(program: &str) -> String {
    let resolved = resolve_program(program);
    let mtime = std::fs::metadata(&resolved)
        .and_then(|meta| meta.modified())
        .ok();
    let key = CacheKey {
        path: resolved,
        mtime,
    };
    if let Some(hit) = cache_get(&key) {
        return hit;
    }
    let version = probe(program).await;
    cache_put(key, version.clone());
    version
}

/// `None` when the probe did not find a version, so gated flags are omitted.
pub fn for_gate(detected: &str) -> Option<&str> {
    if detected == "unknown" {
        None
    } else {
        Some(detected)
    }
}

fn cache_get(key: &CacheKey) -> Option<String> {
    CACHE.lock().ok()?.as_ref()?.get(key).cloned()
}

fn cache_put(key: CacheKey, version: String) {
    if let Ok(mut guard) = CACHE.lock() {
        guard.get_or_insert_with(HashMap::new).insert(key, version);
    }
}

fn resolve_program(program: &str) -> PathBuf {
    let path = Path::new(program);
    if program.contains('/') || program.contains('\\') {
        return path.to_path_buf();
    }
    let Some(path_var) = std::env::var_os("PATH") else {
        return PathBuf::from(program);
    };
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(program)
}

async fn probe(program: &str) -> String {
    let Ok(dir) = tempfile::tempdir() else {
        return "unknown".into();
    };
    let mut cmd = tokio::process::Command::new(program);
    cmd.arg("--version")
        .current_dir(dir.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let Ok(child) = cmd.spawn() else {
        return "unknown".into();
    };
    let Ok(finished) = tokio::time::timeout(PROBE_TIMEOUT, child.wait_with_output()).await else {
        return "unknown".into();
    };
    let Ok(output) = finished else {
        return "unknown".into();
    };
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&output.stderr).into_owned();
    }
    version_token(&text).unwrap_or("unknown").to_string()
}
