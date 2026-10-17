use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
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
    /// Password for the non-superuser runtime role. Separate from `pg_password`,
    /// which stays the bootstrap superuser.
    pub pg_app_password: String,
    /// `pg_app_password` was missing or unreadable. Startup must set this
    /// password on the role before connecting. The file is not written here.
    pub reset_pg_app_password: bool,
    pub admin_password: String,
}

/// One file per secret in `dir` (made `0700`); generated on first run (mode
/// `0600`) and reused after.
///
/// `pg_app_password` is not written when it is missing or unreadable. Startup
/// sets that password on the role, then writes the file. Regenerating it here
/// would look like a normal password and leave the role unreachable.
pub fn load_or_create_secrets(dir: &Path) -> io::Result<DesktopSecrets> {
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let (pg_app_password, reset_pg_app_password) = load_app_password(&dir.join("pg_app_password"));
    Ok(DesktopSecrets {
        session_secret: load_or_create_secret(&dir.join("session_secret"))?,
        master_key: load_or_create_secret(&dir.join("master_key"))?,
        pg_password: load_or_create_secret(&dir.join("pg_password"))?,
        pg_app_password,
        reset_pg_app_password,
        admin_password: load_or_create_admin_password(&dir.join("admin_password"))?,
    })
}

fn load_or_create_secret(path: &Path) -> io::Result<String> {
    // Only a crash during the non-atomic writes of earlier versions leaves an
    // empty file, before anything could have been encrypted with it.
    if std::fs::metadata(path).is_ok_and(|meta| meta.len() == 0) {
        std::fs::remove_file(path)?;
    }
    let value = generate_secret_value();
    if write_new_file(path, &value, 0o600)? {
        Ok(value)
    } else {
        read_secret(path)
    }
}

/// A readable file is reused. Anything else (missing, empty, corrupt, or
/// unreadable) stays on disk and is replaced after the role password is set.
fn load_app_password(path: &Path) -> (String, bool) {
    if let Some(value) = read_valid_secret(path) {
        (value, false)
    } else {
        (generate_secret_value(), true)
    }
}

fn read_valid_secret(path: &Path) -> Option<String> {
    let value = std::fs::read_to_string(path).ok()?.trim().to_string();
    if value.len() == SECRET_BYTES * 2 && value.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(value)
    } else {
        None
    }
}

fn generate_secret_value() -> String {
    let mut bytes = [0u8; SECRET_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// First run generates a hex password. Replace keys may later store the
/// password the admin chose, which is not hex.
fn load_or_create_admin_password(path: &Path) -> io::Result<String> {
    if std::fs::metadata(path).is_ok_and(|meta| meta.len() == 0) {
        std::fs::remove_file(path)?;
    }
    if path.is_file() {
        return read_admin_password(path);
    }
    load_or_create_secret(path)
}

fn read_admin_password(path: &Path) -> io::Result<String> {
    let value = std::fs::read_to_string(path)?.trim().to_string();
    if value.is_empty() || value.chars().any(|c| c.is_control()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("secret file {} is corrupt", path.display()),
        ));
    }
    Ok(value)
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

/// Replace `path` with `contents`, keeping a crash from leaving a partial file.
pub fn write_config_preserving(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = path.with_extension("toml.writing");
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Writes the desktop `config.toml` if missing; returns true when created.
/// An existing file is never touched so user edits survive upgrades.
pub fn ensure_config_file(layout: &DataLayout) -> io::Result<bool> {
    if layout.config_file.exists() {
        return Ok(false);
    }
    write_new_file(&layout.config_file, &generated_config(layout)?, 0o644)
}

/// Creates `path` with `contents` unless it exists; returns true when created.
/// The data is written and synced to a temp file in the same directory, then
/// hard-linked into place, so `path` only ever appears complete after a crash.
fn write_new_file(path: &Path, contents: &str, mode: u32) -> io::Result<bool> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other(format!("{} has no parent", path.display())))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".coppice-")
        .permissions(std::fs::Permissions::from_mode(mode))
        .tempfile_in(dir)?;
    temp.write_all(contents.as_bytes())?;
    temp.as_file().sync_all()?;
    match std::fs::hard_link(temp.path(), path) {
        Ok(()) => {
            std::fs::File::open(dir)?.sync_all()?;
            Ok(true)
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(err) => Err(err),
    }
}

/// Opening lines of a newly written desktop `config.toml`. Word for word.
pub const DESKTOP_CONFIG_HEADER: &str = "\
# Coppice desktop configuration. Edits here survive upgrades.\n\
# Secrets (session secret, encryption key, passwords) live in secrets/, not here.\n\
# The server port, database URL and auth mode are set by the desktop app at startup.\n\
# Turn connectors on or off in Connectors in the app.\n";

fn generated_config(layout: &DataLayout) -> io::Result<String> {
    // Written by hand so each section keeps its comment; toml::to_string drops
    // comments. toml::Value quotes and escapes paths (spaces, backslashes).
    // No `[agent.connectors.*]` tables and no `default_connector`: a signed-in
    // CLI is on until someone turns it off in Connectors.
    let quoted = |s: &str| toml::Value::String(s.to_owned()).to_string();
    let path = |p: &Path| quoted(&p.to_string_lossy());
    Ok(format!(
        "{DESKTOP_CONFIG_HEADER}\
         \n\
         # How agents run.\n\
         [agent]\n\
         # Coppice gives each ticket its own git worktree in this folder.\n\
         worktrees_path = {worktrees}\n\
         \n\
         # Logs and other files from agent runs and chats.\n\
         [storage]\n\
         artifacts_dir = {artifacts}\n\
         # Days a finished run's console and terminal log are kept. Archiving a ticket removes them.\n\
         run_artifacts_retention_days = 30\n\
         \n\
         # Your default plugin folder. Coppice always looks here for plugins.\n\
         [plugins]\n\
         # Add more folders on the Plugins page. This one can't be removed there.\n\
         dir = {plugins}\n\
         \n\
         # Skills that ship with Coppice.\n\
         [mcp]\n\
         # Coppice rewrites coppice/skills here on every start. Edits there are lost.\n\
         builtin_plugins_dir = {builtin_plugins}\n\
         \n\
         # How many days of logs to keep, including today: 3, 7, 14, or 30. Default 14.\n\
         # If logs pass 256 MiB, Coppice deletes the oldest first, even before retention_days is up.\n\
         [logs]\n\
         retention_days = 14\n\
         # Folder for logs. The desktop app uses its own folder and ignores this.\n\
         # dir = \"./data/logs\"\n\
         \n\
         # Keyboard shortcuts. Your keys replace the defaults, and an empty value turns one off.\n\
         # Mod is Cmd on macOS and Ctrl on Linux and WSL.\n\
         # A shortcut Coppice can't use is skipped, and that command keeps its default.\n\
         # [keybindings]\n\
         # \"palette.open\" = \"Mod+K\"\n",
        worktrees = path(&layout.worktrees),
        artifacts = path(&layout.artifacts),
        plugins = path(&layout.plugins),
        builtin_plugins = path(&layout.builtin_plugins),
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
        assert!(
            contents.starts_with(DESKTOP_CONFIG_HEADER),
            "starter file header:\n{contents}"
        );
        let after_header = contents.strip_prefix(DESKTOP_CONFIG_HEADER).unwrap();
        assert!(
            after_header.starts_with("\n# How agents run.\n"),
            "header is followed by the agent section:\n{after_header}"
        );
        assert!(
            !contents.contains("[agent.connectors"),
            "starter file has no connector tables:\n{contents}"
        );
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
        assert_eq!(cfg.logs.retention_days, 14);
        assert!(
            contents.contains(
                "# If logs pass 256 MiB, Coppice deletes the oldest first, even before retention_days is up."
            ),
            "starter file explains the size cap:\n{contents}"
        );
        assert!(
            !contents.contains("default_connector"),
            "starter file omits default_connector"
        );
        assert!(
            contents.contains("# [keybindings]"),
            "starter file shows the shortcuts section:\n{contents}"
        );
        assert!(
            contents.contains(
                "# Keyboard shortcuts. Your keys replace the defaults, and an empty value turns one off."
            ),
            "starter file comment:\n{contents}"
        );
        assert!(
            contents.contains("# Mod is Cmd on macOS and Ctrl on Linux and WSL."),
            "starter file keeps the Mod line:\n{contents}"
        );
        assert!(
            contents.contains(
                "# A shortcut Coppice can't use is skipped, and that command keeps its default."
            ),
            "starter file explains a skipped shortcut:\n{contents}"
        );
        assert!(contents.contains("\"palette.open\" = \"Mod+K\""));
        assert_eq!(
            toml::Value::String("/tmp/coppice".into()).to_string(),
            "\"/tmp/coppice\"",
            "plain paths are quoted so the deb e2e can match the template"
        );
        assert!(
            !contents.to_ascii_lowercase().contains("mock"),
            "starter file does not mention mock: {contents}"
        );
        coppice_config::settings_file::validate_config_text(&contents)
            .expect("settings validation");
    }

    #[test]
    fn config_with_default_connector_still_validates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = layout_in(&dir);
        let text = generated_config(&layout).unwrap().replace(
            "[agent]\n",
            "[agent]\ndefault_connector = \"claude-code\"\n",
        );
        assert!(text.contains("default_connector = \"claude-code\""));
        coppice_config::settings_file::validate_config_text(&text).expect("validates");
        std::fs::write(&layout.config_file, &text).unwrap();
        AppConfig::load_file_only(&layout.config_file).expect("loads");
    }

    #[test]
    fn secrets_are_stable_hex_and_private() {
        let dir = tempfile::tempdir().expect("tempdir");
        let secrets_dir = dir.path().join("secrets");
        std::fs::create_dir_all(&secrets_dir).unwrap();
        let app_password = "12".repeat(32);
        let app_path = secrets_dir.join("pg_app_password");
        std::fs::write(&app_path, &app_password).unwrap();
        std::fs::set_permissions(&app_path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let first = load_or_create_secrets(&secrets_dir).expect("create");
        let second = load_or_create_secrets(&secrets_dir).expect("reuse");
        assert!(!first.reset_pg_app_password);
        assert_eq!(first.pg_app_password, app_password);

        assert_eq!(first.session_secret, second.session_secret);
        assert_eq!(first.master_key, second.master_key);
        assert_eq!(first.pg_password, second.pg_password);
        assert_eq!(first.pg_app_password, second.pg_app_password);
        assert_ne!(first.pg_password, first.pg_app_password);
        assert_eq!(first.admin_password, second.admin_password);
        assert_ne!(first.session_secret, first.master_key);
        for value in [
            &first.session_secret,
            &first.master_key,
            &first.pg_password,
            &first.pg_app_password,
            &first.admin_password,
        ] {
            assert_eq!(value.len(), 64);
            assert!(value.chars().all(|c| c.is_ascii_hexdigit()));
        }
        for name in [
            "session_secret",
            "master_key",
            "pg_password",
            "pg_app_password",
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
    fn missing_app_password_file_is_left_for_startup_to_reset() {
        let dir = tempfile::tempdir().expect("tempdir");
        let secrets = load_or_create_secrets(dir.path()).expect("load");

        assert!(secrets.reset_pg_app_password);
        assert_eq!(secrets.pg_app_password.len(), 64);
        assert!(secrets
            .pg_app_password
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
        assert!(
            !dir.path().join("pg_app_password").exists(),
            "the file is written only after the role password is set"
        );
        assert!(dir.path().join("pg_password").is_file());
    }

    #[test]
    fn corrupt_app_password_file_stays_on_disk_until_startup_resets_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("pg_app_password");
        std::fs::write(&path, "not-hex\n").unwrap();

        let secrets = load_or_create_secrets(dir.path()).expect("load");

        assert!(secrets.reset_pg_app_password);
        assert_ne!(secrets.pg_app_password, "not-hex");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "not-hex\n");
    }

    #[test]
    fn empty_app_password_file_stays_on_disk_until_startup_resets_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("pg_app_password");
        std::fs::write(&path, "").unwrap();

        let secrets = load_or_create_secrets(dir.path()).expect("load");

        assert!(secrets.reset_pg_app_password);
        assert!(path.is_file());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn unreadable_app_password_file_stays_on_disk_until_startup_resets_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("pg_app_password");
        std::fs::write(&path, "ee".repeat(32)).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let secrets = load_or_create_secrets(dir.path()).expect("load");

        assert!(secrets.reset_pg_app_password);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o000);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn readable_app_password_is_reused_and_not_reset() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("pg_app_password");
        let value = "ab".repeat(32);
        std::fs::write(&path, format!("{value}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let first = load_or_create_secrets(dir.path()).expect("load");
        let second = load_or_create_secrets(dir.path()).expect("reload");

        assert!(!first.reset_pg_app_password);
        assert!(!second.reset_pg_app_password);
        assert_eq!(first.pg_app_password, value);
        assert_eq!(second.pg_app_password, value);
        assert_eq!(std::fs::read_to_string(&path).unwrap().trim(), value);
    }

    #[test]
    fn corrupt_secret_file_is_an_error_naming_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("master_key"), "not-hex\n").unwrap();

        let err = load_or_create_secrets(dir.path()).expect_err("corrupt secret must fail");
        assert!(err.to_string().contains("master_key"), "{err}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("master_key")).unwrap(),
            "not-hex\n"
        );
    }

    fn file_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn zero_length_secret_left_by_a_crash_is_regenerated() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = load_or_create_secrets(dir.path()).expect("create");
        std::fs::write(dir.path().join("master_key"), "").unwrap();

        let second = load_or_create_secrets(dir.path()).expect("regenerate");
        assert_eq!(second.master_key.len(), 64);
        assert_ne!(second.master_key, first.master_key);
        assert_eq!(second.session_secret, first.session_secret);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("master_key")).unwrap(),
            second.master_key
        );
        let mode = std::fs::metadata(dir.path().join("master_key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn secrets_dir_is_private_and_holds_no_temp_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let secrets_dir = dir.path().join("secrets");
        std::fs::create_dir_all(&secrets_dir).unwrap();
        std::fs::set_permissions(&secrets_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        load_or_create_secrets(&secrets_dir).expect("create");
        let mode = std::fs::metadata(&secrets_dir)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
        assert_eq!(
            file_names(&secrets_dir),
            [
                "admin_password",
                "master_key",
                "pg_password",
                "session_secret"
            ]
        );
    }

    #[test]
    fn write_new_file_never_replaces_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");

        assert!(write_new_file(&path, "complete\n", 0o644).expect("create"));
        assert!(!write_new_file(&path, "other\n", 0o644).expect("exists"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "complete\n");
        assert_eq!(file_names(dir.path()), ["config.toml"]);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o644);
    }

    #[test]
    fn ensure_config_file_leaves_only_the_complete_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = layout_in(&dir);

        assert!(ensure_config_file(&layout).expect("create"));
        let contents = std::fs::read_to_string(&layout.config_file).unwrap();
        assert_eq!(contents, generated_config(&layout).unwrap());
        let config_dir = layout.config_file.parent().unwrap();
        assert!(
            file_names(config_dir)
                .iter()
                .all(|name| !name.starts_with('.')),
            "{:?}",
            file_names(config_dir)
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
