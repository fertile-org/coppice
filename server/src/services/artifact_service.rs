use serde::Serialize;
use std::path::PathBuf;

pub struct RunArtifactPaths {
    pub terminal_log: PathBuf,
    pub console_events: PathBuf,
    pub meta_json: PathBuf,
    pub session_snapshot: PathBuf,
}

impl RunArtifactPaths {
    pub fn new(artifacts_dir: &str, run_id: &str) -> Self {
        let base = PathBuf::from(artifacts_dir).join("runs").join(run_id);
        Self {
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

    pub fn read_console_events(paths: &RunArtifactPaths) -> Vec<serde_json::Value> {
        let Ok(raw) = std::fs::read_to_string(&paths.console_events) else {
            return Vec::new();
        };
        raw.lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    pub fn write_meta(paths: &RunArtifactPaths, meta: &RunArtifactMeta) -> std::io::Result<()> {
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
}
