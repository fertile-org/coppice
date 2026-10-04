use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use rand::RngCore;

use super::layout::DataLayout;
use crate::config::AppConfig;

const DESKTOP_ADMIN_EMAIL: &str = "admin@localhost";
const SECRET_BYTES: usize = 32;

#[derive(Debug, Clone)]
pub struct DesktopSecrets {
    pub session_secret: String,
    pub master_key: String,
    pub pg_password: String,
    pub admin_password: String,
}

/// One file per secret in `dir`; generated on first run (mode `0600`) and reused after.
pub fn load_or_create_secrets(dir: &Path) -> io::Result<DesktopSecrets> {
    Ok(DesktopSecrets {
        session_secret: load_or_create_secret(&dir.join("session_secret"))?,
        master_key: load_or_create_secret(&dir.join("master_key"))?,
        pg_password: load_or_create_secret(&dir.join("pg_password"))?,
        admin_password: load_or_create_secret(&dir.join("admin_password"))?,
    })
}

fn load_or_create_secret(path: &Path) -> io::Result<String> {
    let mut bytes = [0u8; SECRET_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let value = hex::encode(bytes);
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(mut file) => {
            if let Err(err) = file
                .write_all(value.as_bytes())
                .and_then(|()| file.sync_all())
            {
                let _ = std::fs::remove_file(path);
                return Err(err);
            }
            Ok(value)
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => read_secret(path),
        Err(err) => Err(err),
    }
}

/// An unreadable secret is fatal: regenerating `master_key` would orphan encrypted data.
fn read_secret(path: &Path) -> io::Result<String> {
    let value = std::fs::read_to_string(path)?.trim().to_string();
    if value.len() != SECRET_BYTES * 2 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "secret file {} is corrupt (expected {} hex chars)",
                path.display(),
                SECRET_BYTES * 2
            ),
        ));
    }
    Ok(value)
}

/// Writes the desktop `config.toml` if missing; returns true when created.
/// An existing file is never touched so user edits survive upgrades.
pub fn ensure_config_file(layout: &DataLayout) -> io::Result<bool> {
    let contents = generated_config(layout)?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&layout.config_file)
    {
        Ok(mut file) => {
            file.write_all(contents.as_bytes())?;
            Ok(true)
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(err) => Err(err),
    }
}

fn generated_config(layout: &DataLayout) -> io::Result<String> {
    let path = |p: &Path| toml::Value::String(p.to_string_lossy().into_owned());
    let table = |entries: Vec<(&str, toml::Value)>| {
        toml::Value::Table(
            entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    };
    let doc = table(vec![
        (
            "storage",
            table(vec![("artifacts_dir", path(&layout.artifacts))]),
        ),
        (
            "agent",
            table(vec![
                (
                    "default_connector",
                    toml::Value::String(coppice_connectors::MOCK.into()),
                ),
                ("worktrees_path", path(&layout.worktrees)),
            ]),
        ),
        ("plugins", table(vec![("dir", path(&layout.plugins))])),
        (
            "mcp",
            table(vec![("builtin_plugins_dir", path(&layout.builtin_plugins))]),
        ),
    ]);
    let body = toml::to_string(&doc).map_err(io::Error::other)?;
    Ok(format!(
        "# Coppice desktop configuration. Edits here survive upgrades.\n\
         # Secrets (session secret, encryption key, passwords) live in secrets/, not here.\n\
         # The server port, database URL and auth mode are set by the desktop app at startup.\n\n\
         {body}"
    ))
}

/// Config for a desktop run: the data dir's `config.toml` over defaults, with
/// the values desktop mode depends on forced regardless of the file.
pub fn desktop_config(
    layout: &DataLayout,
    secrets: &DesktopSecrets,
    pg_url: &str,
    server_port: u16,
) -> anyhow::Result<AppConfig> {
    let mut config = AppConfig::load_file_only(&layout.config_file)
        .map_err(|e| anyhow::anyhow!("invalid config {}: {e}", layout.config_file.display()))?;
    config.server.port = server_port;
    config.database.url = pg_url.to_string();
    config.auth.desktop_mode = true;
    config.auth.desktop_allowed_hosts = vec![
        format!("127.0.0.1:{server_port}"),
        format!("localhost:{server_port}"),
    ];
    config.auth.cookie_secure = false;
    config.auth.session_secret = secrets.session_secret.clone();
    config.auth.bootstrap_admin_email = Some(DESKTOP_ADMIN_EMAIL.into());
    config.auth.bootstrap_admin_password = Some(secrets.admin_password.clone());
    config.secrets.master_key = secrets.master_key.clone();
    config.mcp.base_url = None;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn layout_in(dir: &tempfile::TempDir) -> DataLayout {
        let layout = DataLayout::new(&dir.path().join("Application Support").join("Coppice"));
        layout.ensure_dirs().expect("ensure dirs");
        layout
    }

    #[test]
    fn ensure_config_file_creates_once_and_keeps_user_edits() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = layout_in(&dir);

        assert!(ensure_config_file(&layout).expect("create"));
        let mut contents = std::fs::read_to_string(&layout.config_file).unwrap();
        contents.push_str("\n# user edit\n");
        std::fs::write(&layout.config_file, &contents).unwrap();

        assert!(!ensure_config_file(&layout).expect("second call"));
        assert_eq!(
            std::fs::read_to_string(&layout.config_file).unwrap(),
            contents
        );
    }

    #[test]
    fn generated_config_points_at_layout_paths_with_spaces() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = layout_in(&dir);
        ensure_config_file(&layout).expect("create");

        let contents = std::fs::read_to_string(&layout.config_file).unwrap();
        assert!(contents.starts_with('#'), "missing header comment");
        assert!(contents.contains("secrets/"));

        let cfg = AppConfig::load_file_only(&layout.config_file).expect("valid toml");
        assert_eq!(
            Path::new(&cfg.storage.artifacts_dir),
            layout.artifacts.as_path()
        );
        assert_eq!(
            Path::new(&cfg.agent.worktrees_path),
            layout.worktrees.as_path()
        );
        assert_eq!(Path::new(&cfg.plugins.dir), layout.plugins.as_path());
        assert_eq!(
            Path::new(&cfg.mcp.builtin_plugins_dir),
            layout.builtin_plugins.as_path()
        );
        assert_eq!(cfg.agent.default_connector, "mock");
    }

    #[test]
    fn secrets_are_stable_hex_and_private() {
        let dir = tempfile::tempdir().expect("tempdir");
        let secrets_dir = dir.path().join("secrets");
        std::fs::create_dir_all(&secrets_dir).unwrap();

        let first = load_or_create_secrets(&secrets_dir).expect("create");
        let second = load_or_create_secrets(&secrets_dir).expect("reuse");

        assert_eq!(first.session_secret, second.session_secret);
        assert_eq!(first.master_key, second.master_key);
        assert_eq!(first.pg_password, second.pg_password);
        assert_eq!(first.admin_password, second.admin_password);
        assert_ne!(first.session_secret, first.master_key);
        for value in [
            &first.session_secret,
            &first.master_key,
            &first.pg_password,
            &first.admin_password,
        ] {
            assert_eq!(value.len(), 64);
            assert!(value.chars().all(|c| c.is_ascii_hexdigit()));
        }
        for name in [
            "session_secret",
            "master_key",
            "pg_password",
            "admin_password",
        ] {
            let mode = std::fs::metadata(secrets_dir.join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{name}");
        }
    }

    #[test]
    fn corrupt_secret_file_is_an_error_naming_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("master_key"), "").unwrap();

        let err = load_or_create_secrets(dir.path()).expect_err("empty secret must fail");
        assert!(err.to_string().contains("master_key"), "{err}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("master_key")).unwrap(),
            ""
        );
    }

    #[test]
    fn desktop_config_forces_desktop_values_over_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = layout_in(&dir);
        let secrets = load_or_create_secrets(&layout.secrets_dir).expect("secrets");
        ensure_config_file(&layout).expect("create");
        let mut contents = std::fs::read_to_string(&layout.config_file).unwrap();
        assert!(!contents.contains(&secrets.session_secret));
        assert!(!contents.contains(&secrets.master_key));
        contents = contents.replace("[mcp]\n", "[mcp]\nbase_url = \"http://elsewhere/mcp\"\n");
        assert!(contents.contains("http://elsewhere/mcp"));
        contents.push_str(
            "\n[server]\nport = 1\n\n[auth]\ndesktop_mode = false\ncookie_secure = true\ndesktop_allowed_hosts = [\"evil.example:1\"]\n",
        );
        std::fs::write(&layout.config_file, contents).unwrap();

        let pg_url = "postgres://coppice:pw@127.0.0.1:55432/coppice";
        let cfg = desktop_config(&layout, &secrets, pg_url, 43210).expect("config");

        assert_eq!(cfg.server.port, 43210);
        assert_eq!(cfg.database.url, pg_url);
        assert!(cfg.auth.desktop_mode);
        assert_eq!(
            cfg.auth.desktop_allowed_hosts,
            ["127.0.0.1:43210", "localhost:43210"]
        );
        assert!(!cfg.auth.cookie_secure);
        assert_eq!(cfg.auth.session_secret, secrets.session_secret);
        assert_eq!(
            cfg.auth.bootstrap_admin_email.as_deref(),
            Some("admin@localhost")
        );
        assert_eq!(
            cfg.auth.bootstrap_admin_password.as_deref(),
            Some(secrets.admin_password.as_str())
        );
        assert_eq!(cfg.secrets.master_key, secrets.master_key);
        assert_eq!(cfg.mcp.base_url, None);
    }
}
