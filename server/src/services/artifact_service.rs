use crate::sessions::run_artifact_log::{read_console_tail, read_terminal_tail};
use serde::Serialize;
use std::path::PathBuf;

pub struct RunArtifactPaths {
    pub dir: PathBuf,
    pub terminal_log: PathBuf,
    pub console_events: PathBuf,
    pub meta_json: PathBuf,
    pub session_snapshot: PathBuf,
}

impl RunArtifactPaths {
    pub fn new(artifacts_dir: &str, run_id: &str) -> Self {
        let base = PathBuf::from(artifacts_dir).join("runs").join(run_id);
        Self {
            dir: base.clone(),
            terminal_log: base.join("terminal.log"),
            console_events: base.join("console.events.jsonl"),
            meta_json: base.join("meta.json"),
            session_snapshot: base.join("session.snapshot.json"),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunArtifactMeta {
    pub provider: String,
    pub session_id: Option<String>,
    pub frame_count: u64,
    pub ended_at: String,
}

pub struct ArtifactService;

impl ArtifactService {
    pub fn write_terminal_log(paths: &RunArtifactPaths, content: &[u8]) -> std::io::Result<()> {
        if let Some(parent) = paths.terminal_log.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&paths.terminal_log, content)
    }

    pub fn write_console_events(
        paths: &RunArtifactPaths,
        events: &[serde_json::Value],
    ) -> std::io::Result<()> {
        if let Some(parent) = paths.console_events.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut lines = String::new();
        for event in events {
            lines.push_str(&serde_json::to_string(event)?);
            lines.push('\n');
        }
        std::fs::write(&paths.console_events, lines)
    }

    /// Every stored console event, oldest first, across rotated segments.
    pub fn read_console_events(paths: &RunArtifactPaths) -> Vec<serde_json::Value> {
        read_console_tail(&paths.dir, u64::MAX)
    }

    /// The newest console events that fit in `max_bytes`, streamed from disk.
    pub fn read_console_events_tail(
        paths: &RunArtifactPaths,
        max_bytes: u64,
    ) -> Vec<serde_json::Value> {
        read_console_tail(&paths.dir, max_bytes)
    }

    /// The last `max_bytes` of the terminal log, without reading the rest.
    pub fn read_terminal_log_tail(paths: &RunArtifactPaths, max_bytes: u64) -> Option<Vec<u8>> {
        read_terminal_tail(&paths.dir, max_bytes)
    }

    /// Append to the terminal log without rewriting it.
    pub fn append_terminal_log(paths: &RunArtifactPaths, content: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        std::fs::create_dir_all(&paths.dir)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&paths.terminal_log)?;
        file.write_all(content)
    }

    pub fn write_meta(paths: &RunArtifactPaths, meta: &RunArtifactMeta) -> std::io::Result<()> {
        if let Some(parent) = paths.meta_json.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_vec_pretty(meta)?;
        std::fs::write(&paths.meta_json, raw)
    }

    pub fn write_session_snapshot(
        paths: &RunArtifactPaths,
        snapshot: &serde_json::Value,
    ) -> std::io::Result<()> {
        if let Some(parent) = paths.session_snapshot.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = paths.session_snapshot.with_extension("json.tmp");
        let raw = serde_json::to_vec_pretty(snapshot)?;
        std::fs::write(&tmp, raw)?;
        std::fs::rename(tmp, &paths.session_snapshot)
    }

    pub fn read_session_snapshot(paths: &RunArtifactPaths) -> Option<serde_json::Value> {
        let raw = std::fs::read_to_string(&paths.session_snapshot).ok()?;
        serde_json::from_str(&raw).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_artifact_paths() {
        let paths = RunArtifactPaths::new("/data/artifacts", "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            paths.terminal_log.display().to_string(),
            "/data/artifacts/runs/550e8400-e29b-41d4-a716-446655440000/terminal.log"
        );
        assert_eq!(
            paths.meta_json.display().to_string(),
            "/data/artifacts/runs/550e8400-e29b-41d4-a716-446655440000/meta.json"
        );
    }

    #[test]
    fn run_artifact_paths_includes_session_snapshot() {
        let paths = RunArtifactPaths::new("/data/artifacts", "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(
            paths.session_snapshot.display().to_string(),
            "/data/artifacts/runs/550e8400-e29b-41d4-a716-446655440000/session.snapshot.json"
        );
    }

    #[test]
    fn write_meta_creates_missing_parent_dirs() {
        let temp = tempfile::tempdir().expect("tempdir");
        let artifacts_dir = temp.path().join("artifacts");
        let run_id = "550e8400-e29b-41d4-a716-446655440000";
        let paths = RunArtifactPaths::new(
            artifacts_dir.to_str().expect("utf8"),
            run_id,
        );
        assert!(!paths.meta_json.parent().expect("parent").exists());

        ArtifactService::write_meta(
            &paths,
            &RunArtifactMeta {
                provider: "cursor".into(),
                session_id: None,
                frame_count: 0,
                ended_at: "2026-09-07T00:00:00Z".into(),
            },
        )
        .expect("write meta");

        assert!(paths.meta_json.is_file());
    }
}
