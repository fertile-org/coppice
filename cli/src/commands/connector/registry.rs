use std::ffi::OsString;
use std::path::{Path, PathBuf};

use coppice_connectors::probe::{augment_path, auth_path_present, resolve_binary};
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

/// Any set, non-empty value counts, including non-UTF-8 ones.
pub fn env_lookup(key: &str) -> Option<String> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string_lossy().into_owned())
}

/// `$PATH` plus the common bin dirs desktop launchers leave off.
pub fn search_path(home: &Path) -> OsString {
    augment_path(home, &std::env::var_os("PATH").unwrap_or_default())
}

/// Lookup on the process `$PATH` only (no common bin dirs).
pub fn binary_on_path(name: &str) -> Option<PathBuf> {
    resolve_binary(name, &std::env::var_os("PATH").unwrap_or_default())
}

pub fn auth_present(meta: &ConnectorDescriptor, home: &Path) -> bool {
    meta.install
        .auth_env
        .iter()
        .any(|key| env_lookup(key).is_some())
        || meta
            .install
            .auth_paths
            .iter()
            .any(|rel| auth_path_present(&home.join(rel)))
}

#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Env vars are process-global: every test that sets them goes through here.
#[cfg(test)]
pub fn with_env_vars(vars: &[(&str, Option<String>)], f: impl FnOnce()) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved: Vec<(String, Option<std::ffi::OsString>)> = vars
        .iter()
        .map(|(key, _)| ((*key).to_string(), std::env::var_os(key)))
        .collect();
    for (key, val) in vars {
        match val {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
    f();
    for (key, prev) in saved {
        match prev {
            Some(v) => std::env::set_var(&key, v),
            None => std::env::remove_var(&key),
        }
    }
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

    #[cfg(unix)]
    #[test]
    fn env_lookup_counts_non_utf8_values_as_set() {
        use std::os::unix::ffi::OsStrExt;
        let raw = std::ffi::OsStr::from_bytes(b"key-\xff").to_os_string();
        with_env_vars(&[("COPPICE_TEST_NON_UTF8", None)], || {
            assert_eq!(env_lookup("COPPICE_TEST_NON_UTF8"), None);
            std::env::set_var("COPPICE_TEST_NON_UTF8", "");
            assert_eq!(env_lookup("COPPICE_TEST_NON_UTF8"), None);
            std::env::set_var("COPPICE_TEST_NON_UTF8", &raw);
            assert!(env_lookup("COPPICE_TEST_NON_UTF8").is_some());
        });
    }

    #[test]
    fn auth_accepts_anthropic_api_key_env() {
        let dir = tempfile::tempdir().unwrap();
        let m = coppice_connectors::get(coppice_connectors::CLAUDE_CODE).unwrap();
        with_env_vars(&[("ANTHROPIC_API_KEY", Some("test-key".into()))], || {
            assert!(auth_present(m, dir.path()));
        });
    }
}
