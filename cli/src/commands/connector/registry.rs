use std::path::{Path, PathBuf};

use coppice_connectors::ConnectorDescriptor;

pub fn parse_id(s: &str) -> anyhow::Result<&'static ConnectorDescriptor> {
    coppice_connectors::get(s).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown connector `{s}`; expected one of: {}",
            coppice_connectors::all()
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("/home/coppice"))
}

pub fn auth_present(meta: &ConnectorDescriptor, home: &Path) -> bool {
    for key in meta.install.auth_env {
        if std::env::var_os(key).is_some_and(|v| !v.is_empty()) {
            return true;
        }
    }
    for rel in meta.install.auth_paths {
        let p = home.join(rel);
        if path_looks_like_auth(&p) {
            return true;
        }
    }
    false
}

fn path_looks_like_auth(path: &Path) -> bool {
    if path.is_file() {
        return std::fs::metadata(path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
    }
    if path.is_dir() {
        return std::fs::read_dir(path)
            .ok()
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
    }
    false
}

pub fn binary_on_path(name: &str) -> Option<PathBuf> {
    which::which(name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn auth_rejects_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let auth = dir.path().join(".config/cursor/auth.json");
        fs::create_dir_all(auth.parent().unwrap()).unwrap();
        fs::File::create(&auth).unwrap(); // empty
        let m = coppice_connectors::get(coppice_connectors::CURSOR).unwrap();
        assert!(!auth_present(m, dir.path()));
    }

    #[test]
    fn auth_accepts_non_empty_auth_json() {
        let dir = tempfile::tempdir().unwrap();
        let auth = dir.path().join(".config/cursor/auth.json");
        fs::create_dir_all(auth.parent().unwrap()).unwrap();
        let mut f = fs::File::create(&auth).unwrap();
        writeln!(f, "{{ \"token\": \"x\" }}").unwrap();
        let m = coppice_connectors::get(coppice_connectors::CURSOR).unwrap();
        assert!(auth_present(m, dir.path()));
    }

    #[test]
    fn auth_rejects_empty_opencode_install_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".opencode/bin")).unwrap();
        let m = coppice_connectors::get(coppice_connectors::OPENCODE).unwrap();
        assert!(
            !auth_present(m, dir.path()),
            "install tree alone must not count as auth"
        );
    }

    #[test]
    fn auth_accepts_anthropic_api_key_env() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("ANTHROPIC_API_KEY", "test-key");
        let m = coppice_connectors::get(coppice_connectors::CLAUDE_CODE).unwrap();
        assert!(auth_present(m, dir.path()));
        std::env::remove_var("ANTHROPIC_API_KEY");
    }
}
