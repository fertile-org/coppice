use crate::AppConfig;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const BACKUP_FORMAT_VERSION: u32 = 1;
const MANIFEST_NAME: &str = "manifest.json";
const CONFIG_NAME: &str = "config.toml";
const DATABASE_NAME: &str = "database.sql";

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("backup tool unavailable: {0}")]
    ToolUnavailable(String),
    #[error("invalid backup archive: {0}")]
    InvalidArchive(String),
    #[error("unsupported backup format version {0}")]
    UnsupportedVersion(u32),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("config error: {0}")]
    Config(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format_version: u32,
    pub exported_at: String,
    pub app_version: String,
}

pub struct BackupService;

impl BackupService {
    pub async fn export_to_path(config: &AppConfig, zip_path: &Path) -> Result<(), BackupError> {
        export_to_path_with(config, zip_path, run_pg_dump)
    }

    pub async fn import_from_bytes(
        config: &AppConfig,
        pool: &PgPool,
        bytes: &[u8],
        config_path: Option<&Path>,
        secret_store: &crate::crypto::SecretStore,
    ) -> Result<String, BackupError> {
        let temp_dir = tempfile::tempdir().map_err(BackupError::Io)?;
        let extract_root = temp_dir.path().join("import");
        std::fs::create_dir_all(&extract_root)?;

        extract_zip(bytes, &extract_root)?;
        validate_manifest(&extract_root)?;

        let db_sql = extract_root.join(DATABASE_NAME);
        if !db_sql.is_file() {
            return Err(BackupError::InvalidArchive(format!(
                "missing {DATABASE_NAME}"
            )));
        }

        let restore = plan_config_restore(config_path, &extract_root);

        terminate_other_sessions(pool).await?;
        run_psql_file(&config.database.url, &db_sql)?;
        crate::services::secret_service::SecretService::new(pool, secret_store)
            .flag_undecryptable()
            .await
            .map_err(|err| match err {
                crate::services::secret_service::SecretError::Database(err) => {
                    BackupError::Database(err)
                }
                other => BackupError::Config(other.to_string()),
            })?;

        restore_tree(
            &extract_root.join("data/artifacts"),
            Path::new(&config.storage.artifacts_dir),
        )?;
        restore_tree(
            &extract_root.join("data/worktrees"),
            Path::new(&config.agent.worktrees_path),
        )?;

        if let Some(text) = restore.replacement {
            let Some(path) = config_path else {
                tracing::error!("backup settings could not be saved: no config path");
                return Err(BackupError::Config(
                    "Your data was imported, but the settings couldn't be saved. Your current settings were kept.".into(),
                ));
            };
            if let Err(err) =
                coppice_config::connector_file::write_toml_atomic(path, &text)
            {
                tracing::error!(error = %err, "backup settings could not be saved");
                return Err(BackupError::Config(
                    "Your data was imported, but the settings couldn't be saved. Your current settings were kept.".into(),
                ));
            }
        }

        Ok(restore.note)
    }
}

fn export_to_path_with(
    config: &AppConfig,
    zip_path: &Path,
    dump: fn(&str, &Path) -> Result<(), BackupError>,
) -> Result<(), BackupError> {
    let temp_dir = tempfile::tempdir().map_err(BackupError::Io)?;
    let staging = temp_dir.path();

    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        exported_at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "unknown".into()),
        app_version: env!("CARGO_PKG_VERSION").into(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| BackupError::InvalidArchive(e.to_string()))?;
    std::fs::write(staging.join(MANIFEST_NAME), manifest_json)?;

    let config_toml = sanitize_config_toml(config).map_err(BackupError::Config)?;
    std::fs::write(staging.join(CONFIG_NAME), config_toml)?;

    let db_dump = staging.join(DATABASE_NAME);
    dump(&config.database.url, &db_dump)?;

    copy_tree_into_staging(
        Path::new(&config.storage.artifacts_dir),
        &staging.join("data/artifacts"),
    )?;
    copy_tree_into_staging(
        Path::new(&config.agent.worktrees_path),
        &staging.join("data/worktrees"),
    )?;

    write_zip(staging, zip_path)?;
    Ok(())
}

/// `config.toml` for a backup. Secret fields are removed and credentials are
/// stripped from URL values. One function so export cannot forget a field.
pub fn sanitize_config_toml(config: &AppConfig) -> Result<String, String> {
    let raw = toml::to_string_pretty(config).map_err(|err| err.to_string())?;
    let mut doc = raw
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| err.to_string())?;
    strip_credential_urls(doc.as_table_mut());
    remove_secret_keys(doc.as_table_mut());
    Ok(doc.to_string())
}

fn is_secret_key(key: &str) -> bool {
    matches!(
        key,
        "session_secret"
            | "bootstrap_password"
            | "bootstrap_admin_password"
            | "master_key"
            | "password"
            | "secret"
            | "api_key"
            | "token"
            | "access_token"
            | "refresh_token"
    )
}

/// Install-local values. Import keeps whatever this machine already has.
fn is_preserved_path(path: &str) -> bool {
    matches!(
        path,
        "database"
            | "database.url"
            | "secrets"
            | "secrets.master_key"
            | "auth.session_secret"
            | "auth.bootstrap_password"
            | "auth.bootstrap_admin_password"
            | "auth.bootstrap_admin_email"
            | "auth.desktop_mode"
            | "auth.desktop_allowed_hosts"
            | "server"
            | "server.port"
            | "storage.artifacts_dir"
            | "agent.worktrees_path"
            | "agent.default_connector"
            | "agent.default_provider"
            | "plugins.dir"
            | "mcp.base_url"
            | "mcp.builtin_plugins_dir"
            | "logs.dir"
            | "web"
            | "web.port"
            | "web.static_dir"
            | "web.api_url"
    ) || path.rsplit('.').next().is_some_and(is_secret_key)
}

fn remove_secret_keys(table: &mut toml_edit::Table) {
    let keys: Vec<String> = table.iter().map(|(key, _)| key.to_string()).collect();
    for key in keys {
        if is_secret_key(&key) {
            table.remove(&key);
            continue;
        }
        if let Some(child) = table.get_mut(&key).and_then(|item| item.as_table_mut()) {
            remove_secret_keys(child);
            if child.is_empty() {
                table.remove(&key);
            }
        }
    }
}

fn strip_credential_urls(table: &mut toml_edit::Table) {
    let keys: Vec<String> = table.iter().map(|(key, _)| key.to_string()).collect();
    for key in keys {
        if key == "url" || key.ends_with("_url") {
            if let Some(raw) = table.get(&key).and_then(|item| item.as_str()) {
                let stripped = strip_url_userinfo(raw);
                if stripped != raw {
                    table.insert(&key, toml_edit::value(stripped));
                }
            }
        }
        if let Some(child) = table.get_mut(&key).and_then(|item| item.as_table_mut()) {
            strip_credential_urls(child);
        }
    }
}

/// `scheme://user:password@host/path` → `scheme://host/path`.
fn strip_url_userinfo(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let rest = &url[scheme_end + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let Some(at) = authority.rfind('@') else {
        return url.to_string();
    };
    let host = &authority[at + 1..];
    format!("{}://{host}{}", &url[..scheme_end], &rest[authority_end..])
}

struct ConfigRestore {
    note: String,
    replacement: Option<String>,
}

fn plan_config_restore(config_path: Option<&Path>, extract_root: &Path) -> ConfigRestore {
    let Some(config_path) = config_path else {
        return ConfigRestore {
            note: "Import finished. Your current settings were kept.".into(),
            replacement: None,
        };
    };
    let imported_path = extract_root.join(CONFIG_NAME);
    let imported = match std::fs::read_to_string(&imported_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return ConfigRestore {
                note: "Import finished. The backup had no settings, so your current settings were kept."
                    .into(),
                replacement: None,
            };
        }
        Err(err) => {
            return ConfigRestore {
                note: invalid_config_note(&err.to_string()),
                replacement: None,
            };
        }
    };
    if let Err(err) = coppice_config::settings_file::validate_config_text(&imported) {
        return ConfigRestore {
            note: invalid_config_note(&err.message),
            replacement: None,
        };
    }
    let local = if config_path.is_file() {
        match std::fs::read_to_string(config_path) {
            Ok(text) => text,
            Err(err) => {
                tracing::error!(error = %err, "current settings could not be read");
                return ConfigRestore {
                    note: "Couldn't read your current settings, so nothing was changed.".into(),
                    replacement: None,
                };
            }
        }
    } else {
        String::new()
    };
    if !local.trim().is_empty() && local.parse::<toml_edit::DocumentMut>().is_err() {
        return ConfigRestore {
            note: "Couldn't read your current settings, so nothing was changed.".into(),
            replacement: None,
        };
    }
    let merged = match merge_backup_settings(&local, &imported) {
        Ok(text) => text,
        Err(err) => {
            return ConfigRestore {
                note: format!(
                    "Couldn't apply the backup's settings, so your current settings were kept. {}",
                    sentence(&err)
                ),
                replacement: None,
            };
        }
    };
    if let Err(err) = coppice_config::settings_file::validate_config_text(&merged) {
        return ConfigRestore {
            note: format!(
                "Couldn't apply the backup's settings, so your current settings were kept. {}",
                sentence(&err.message)
            ),
            replacement: None,
        };
    }
    ConfigRestore {
        note: "Import finished. The backup's settings were applied, and this install kept its passwords, keys, and folder paths. If something looks wrong, restart Coppice.".into(),
        replacement: Some(merged),
    }
}

fn invalid_config_note(reason: &str) -> String {
    format!(
        "Import finished, but the backup's settings couldn't be read, so your current settings were kept. {}",
        sentence(reason)
    )
}

fn sentence(text: &str) -> String {
    let text = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    if text.is_empty() {
        return String::new();
    }
    if text.ends_with('.') || text.ends_with('!') || text.ends_with('?') {
        text.to_string()
    } else {
        format!("{text}.")
    }
}

fn merge_backup_settings(local: &str, imported: &str) -> Result<String, String> {
    let mut dest = if local.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        local
            .parse::<toml_edit::DocumentMut>()
            .map_err(|err| err.to_string())?
    };
    let src = imported
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| err.to_string())?;
    merge_table(dest.as_table_mut(), src.as_table(), "");
    Ok(dest.to_string())
}

fn merge_table(dest: &mut toml_edit::Table, src: &toml_edit::Table, prefix: &str) {
    for (key, item) in src.iter() {
        let path = if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{prefix}.{key}")
        };
        if is_preserved_path(&path) {
            continue;
        }
        if let Some(src_child) = item.as_table() {
            let created = dest.get(key).is_none();
            let empty = {
                let entry = dest
                    .entry(key)
                    .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
                if let Some(dest_child) = entry.as_table_mut() {
                    if created {
                        dest_child.set_implicit(src_child.is_implicit());
                    }
                    merge_table(dest_child, src_child, &path);
                    dest_child.is_empty()
                } else {
                    false
                }
            };
            if empty {
                dest.remove(key);
            }
            continue;
        }
        dest.insert(key, item.clone());
    }
}

fn validate_manifest(extract_root: &Path) -> Result<(), BackupError> {
    let manifest_path = extract_root.join(MANIFEST_NAME);
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|_| BackupError::InvalidArchive("missing manifest.json".into()))?;
    let manifest: BackupManifest = serde_json::from_str(&raw)
        .map_err(|e| BackupError::InvalidArchive(e.to_string()))?;
    if manifest.format_version != BACKUP_FORMAT_VERSION {
        return Err(BackupError::UnsupportedVersion(manifest.format_version));
    }
    Ok(())
}

/// Set by the desktop app to its bundled Postgres `bin` / `lib` directories.
pub const PG_BIN_DIR_ENV: &str = "COPPICE_PG_BIN_DIR";
pub const PG_LIB_DIR_ENV: &str = "COPPICE_PG_LIB_DIR";

#[derive(Debug)]
struct PgTool {
    program: PathBuf,
    lib_dir: Option<PathBuf>,
}

/// The bundled binary when [`PG_BIN_DIR_ENV`] is set, else `name` on `PATH`.
fn resolve_pg_tool(name: &str, env: impl Fn(&str) -> Option<OsString>) -> PgTool {
    let dir = |key| {
        env(key)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    match dir(PG_BIN_DIR_ENV) {
        Some(bin) => PgTool {
            program: bin.join(name),
            lib_dir: dir(PG_LIB_DIR_ENV),
        },
        None => PgTool {
            program: PathBuf::from(name),
            lib_dir: None,
        },
    }
}

/// Bundled Linux libraries carry no RUNPATH, so the bundled tools need
/// `LD_LIBRARY_PATH`; set on this command only so other children are unaffected.
/// A bare name is resolved through the host environment so a short desktop PATH
/// still finds the user's `pg_dump` / `psql`.
fn pg_command(name: &str) -> std::process::Command {
    let tool = resolve_pg_tool(name, |key| std::env::var_os(key));
    let bundled = tool.lib_dir.is_some() || tool.program.components().count() > 1;
    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut cmd = if bundled {
        std::process::Command::new(&tool.program)
    } else {
        crate::host_env::current()
            .command(name, &[] as &[&str])
            .unwrap_or_else(|_| std::process::Command::new(name))
    };
    #[cfg(target_os = "linux")]
    if let Some(lib) = &tool.lib_dir {
        cmd.env("LD_LIBRARY_PATH", lib);
    }
    cmd
}

fn run_pg_dump(database_url: &str, out_path: &Path) -> Result<(), BackupError> {
    let output = pg_command("pg_dump")
        .args([
            "--dbname",
            database_url,
            "--clean",
            "--if-exists",
            "--no-owner",
            "--no-acl",
            "--file",
        ])
        .arg(out_path)
        .output()
        .map_err(|e| BackupError::ToolUnavailable(format!("pg_dump: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BackupError::ToolUnavailable(format!(
            "pg_dump failed: {stderr}"
        )));
    }
    Ok(())
}

async fn terminate_other_sessions(pool: &PgPool) -> Result<(), BackupError> {
    sqlx::query(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
         WHERE datname = current_database() AND pid <> pg_backend_pid()",
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn run_psql_file(database_url: &str, sql_path: &Path) -> Result<(), BackupError> {
    let child = pg_command("psql")
        .args([
            "--dbname",
            database_url,
            "-v",
            "ON_ERROR_STOP=1",
            "-f",
        ])
        .arg(sql_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| BackupError::ToolUnavailable(format!("psql: {e}")))?;

    let output = child
        .wait_with_output()
        .map_err(|e| BackupError::ToolUnavailable(format!("psql wait: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BackupError::ToolUnavailable(format!(
            "psql restore failed: {stderr}"
        )));
    }
    Ok(())
}

fn copy_tree_into_staging(src: &Path, dest: &Path) -> Result<(), BackupError> {
    if !src.exists() {
        return Ok(());
    }
    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let rel = entry.path().strip_prefix(src).map_err(|e| {
            BackupError::InvalidArchive(format!("path prefix: {e}"))
        })?;
        let target = dest.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn restore_tree(src: &Path, dest: &Path) -> Result<(), BackupError> {
    if !src.exists() {
        return Ok(());
    }
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(BackupError::Io)?;
    }
    copy_tree_into_staging(src, dest)?;
    Ok(())
}

fn write_zip(staging: &Path, zip_path: &Path) -> Result<(), BackupError> {
    let file = std::fs::File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for entry in WalkDir::new(staging).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(staging).map_err(|e| {
            BackupError::InvalidArchive(format!("zip staging path: {e}"))
        })?;
        let name = rel.to_string_lossy().replace('\\', "/");
        zip.start_file(name, options)?;
        let mut f = std::fs::File::open(path)?;
        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer)?;
        zip.write_all(&buffer)?;
    }
    zip.finish()?;
    Ok(())
}

fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), BackupError> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let enclosed = file.enclosed_name().ok_or_else(|| {
            BackupError::InvalidArchive("zip path traversal".into())
        })?;
        let out_path = dest.join(enclosed);
        if file.name().ends_with('/') {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut outfile = std::fs::File::create(&out_path)?;
        std::io::copy(&mut file, &mut outfile)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_roundtrip_json() {
        let manifest = BackupManifest {
            format_version: BACKUP_FORMAT_VERSION,
            exported_at: "2026-01-01T00:00:00Z".into(),
            app_version: "0.1.0".into(),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: BackupManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.format_version, BACKUP_FORMAT_VERSION);
    }

    fn env_of<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn pg_tool_uses_path_lookup_without_bundle_env() {
        let tool = resolve_pg_tool("pg_dump", env_of(&[]));
        assert_eq!(tool.program, PathBuf::from("pg_dump"));
        assert_eq!(tool.lib_dir, None);

        let tool = resolve_pg_tool("psql", env_of(&[(PG_BIN_DIR_ENV, "")]));
        assert_eq!(tool.program, PathBuf::from("psql"));
    }

    #[test]
    fn pg_tool_uses_bundled_binary_and_libs_when_set() {
        let env = [
            (PG_BIN_DIR_ENV, "/r/postgres/bin"),
            (PG_LIB_DIR_ENV, "/r/postgres/lib"),
        ];
        let tool = resolve_pg_tool("pg_dump", env_of(&env));
        assert_eq!(tool.program, PathBuf::from("/r/postgres/bin/pg_dump"));
        assert_eq!(tool.lib_dir, Some(PathBuf::from("/r/postgres/lib")));

        let tool = resolve_pg_tool("psql", env_of(&env[..1]));
        assert_eq!(tool.program, PathBuf::from("/r/postgres/bin/psql"));
        assert_eq!(tool.lib_dir, None);
    }

    fn marker_dump(_url: &str, path: &Path) -> Result<(), BackupError> {
        std::fs::write(path, b"database-dump-marker")?;
        Ok(())
    }

    fn zip_text(bytes: &[u8]) -> String {
        let reader = std::io::Cursor::new(bytes);
        let mut archive = ZipArchive::new(reader).expect("zip");
        let mut all = String::new();
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).expect("entry");
            all.push_str(file.name());
            all.push('\n');
            let mut body = String::new();
            std::io::Read::read_to_string(&mut file, &mut body).expect("entry text");
            all.push_str(&body);
            all.push('\n');
        }
        all
    }

    #[test]
    fn backup_zip_omits_secret_values() {
        let mut config = AppConfig::load_defaults().expect("defaults");
        config.database.url =
            "postgres://coppice:db-pass-SENTINEL-eee@127.0.0.1:5432/coppice".into();
        config.auth.session_secret = "session-secret-SENTINEL-aaa".into();
        config.auth.bootstrap_password = "bootstrap-pass-SENTINEL-bbb".into();
        config.auth.bootstrap_admin_password = Some("admin-pass-SENTINEL-ccc".into());
        config.secrets.master_key = "master-key-SENTINEL-ddd".into();
        config.mcp.base_url = Some("http://user:base-url-SENTINEL-fff@example.test/mcp".into());
        config.git.author_name = "Kept Author".into();
        config.agent.worker_count = 7;
        config.storage.artifacts_dir = "/tmp/coppice-backup-test-missing-artifacts".into();
        config.agent.worktrees_path = "/tmp/coppice-backup-test-missing-worktrees".into();

        let dir = tempfile::tempdir().expect("tempdir");
        let zip_path = dir.path().join("backup.zip");
        export_to_path_with(&config, &zip_path, marker_dump).expect("export");
        let bytes = std::fs::read(&zip_path).expect("zip bytes");
        let text = zip_text(&bytes);

        for secret in [
            "session-secret-SENTINEL-aaa",
            "bootstrap-pass-SENTINEL-bbb",
            "admin-pass-SENTINEL-ccc",
            "master-key-SENTINEL-ddd",
            "db-pass-SENTINEL-eee",
            "base-url-SENTINEL-fff",
            "session_secret",
            "bootstrap_password",
            "bootstrap_admin_password",
            "master_key",
        ] {
            assert!(!text.contains(secret), "{secret} leaked into the zip:\n{text}");
        }
        assert!(text.contains("Kept Author"), "{text}");
        assert!(text.contains("worker_count = 7"), "{text}");
        assert!(
            text.contains("postgres://127.0.0.1:5432/coppice"),
            "{text}"
        );
        assert!(text.contains("http://example.test/mcp"), "{text}");
        assert!(text.contains("database-dump-marker"), "{text}");
    }

    #[test]
    fn strip_url_userinfo_keeps_host_and_path() {
        assert_eq!(
            strip_url_userinfo("postgres://coppice:s3cret@127.0.0.1:5432/coppice"),
            "postgres://127.0.0.1:5432/coppice"
        );
        assert_eq!(
            strip_url_userinfo("postgres://coppice@localhost/coppice"),
            "postgres://localhost/coppice"
        );
        assert_eq!(
            strip_url_userinfo("postgres://localhost:5432/coppice"),
            "postgres://localhost:5432/coppice"
        );
    }

    #[test]
    fn import_keeps_local_secrets_and_paths_and_applies_other_settings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("config.toml");
        let local = "\
# keep me
[auth]
session_secret = \"local-session-SECRET\"
bootstrap_password = \"local-bootstrap-SECRET\"
bootstrap_admin_password = \"local-admin-SECRET\"
desktop_mode = true

[secrets]
master_key = \"local-master-SECRET\"

[database]
url = \"postgres://local-user:local-db-SECRET@localhost:5432/localdb\"

[server]
port = 5000

[storage]
artifacts_dir = \"/local/artifacts\"
max_upload_bytes = 10

[agent]
worktrees_path = \"/local/worktrees\"
worker_count = 2

[logs]
dir = \"/local/logs\"
retention_days = 14

[git]
author_name = \"Local Author\"
";
        std::fs::write(&config_path, local).unwrap();
        let extract = dir.path().join("extract");
        std::fs::create_dir_all(&extract).unwrap();
        std::fs::write(
            extract.join(CONFIG_NAME),
            r#"
[auth]
session_secret = "zip-session-SECRET"
bootstrap_password = "zip-bootstrap-SECRET"
bootstrap_admin_password = "zip-admin-SECRET"
cookie_secure = true
desktop_mode = false

[secrets]
master_key = "zip-master-SECRET"

[database]
url = "postgres://zip-user:zip-db-SECRET@other.example:5432/zipdb"

[server]
port = 9

[storage]
artifacts_dir = "/zip/artifacts"
max_upload_bytes = 42

[agent]
worktrees_path = "/zip/worktrees"
worker_count = 9

[logs]
dir = "/zip/logs"
retention_days = 3

[git]
author_name = "Zip Author"

[knowledge.retrieval]
top_k = 4

[keybindings]
"palette.open" = "Mod+P"
"#,
        )
        .unwrap();

        let planned = plan_config_restore(Some(&config_path), &extract);
        let merged = planned.replacement.expect("settings applied");
        assert!(
            planned.note.contains("passwords, keys, and folder paths"),
            "{}",
            planned.note
        );
        assert!(merged.contains("# keep me"), "{merged}");
        assert!(merged.contains("local-session-SECRET"), "{merged}");
        assert!(merged.contains("local-bootstrap-SECRET"), "{merged}");
        assert!(merged.contains("local-admin-SECRET"), "{merged}");
        assert!(merged.contains("local-master-SECRET"), "{merged}");
        assert!(merged.contains("local-db-SECRET"), "{merged}");
        assert!(merged.contains("worktrees_path = \"/local/worktrees\""), "{merged}");
        assert!(merged.contains("artifacts_dir = \"/local/artifacts\""), "{merged}");
        assert!(merged.contains("dir = \"/local/logs\""), "{merged}");
        assert!(merged.contains("port = 5000"), "{merged}");
        assert!(merged.contains("desktop_mode = true"), "{merged}");
        assert!(merged.contains("worker_count = 9"), "{merged}");
        assert!(merged.contains("max_upload_bytes = 42"), "{merged}");
        assert!(merged.contains("retention_days = 3"), "{merged}");
        assert!(merged.contains("Zip Author"), "{merged}");
        assert!(merged.contains("cookie_secure = true"), "{merged}");
        assert!(merged.contains("top_k = 4"), "{merged}");
        assert!(merged.contains("palette.open"), "{merged}");
        for leaked in [
            "zip-session-SECRET",
            "zip-bootstrap-SECRET",
            "zip-admin-SECRET",
            "zip-master-SECRET",
            "zip-db-SECRET",
            "/zip/artifacts",
            "/zip/worktrees",
            "/zip/logs",
        ] {
            assert!(!merged.contains(leaked), "{leaked} was imported:\n{merged}");
        }
        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), local);

        std::fs::write(
            extract.join(CONFIG_NAME),
            "[logs]\nretention_days = 99\n",
        )
        .unwrap();
        let invalid = plan_config_restore(Some(&config_path), &extract);
        assert!(invalid.replacement.is_none());
        assert!(
            invalid.note.contains("backup's settings couldn't be read")
                && invalid.note.contains("current settings were kept"),
            "{}",
            invalid.note
        );
        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), local);

        std::fs::remove_file(extract.join(CONFIG_NAME)).unwrap();
        let missing = plan_config_restore(Some(&config_path), &extract);
        assert!(missing.note.contains("had no settings"), "{}", missing.note);
        assert_eq!(std::fs::read_to_string(&config_path).unwrap(), local);
    }
}
