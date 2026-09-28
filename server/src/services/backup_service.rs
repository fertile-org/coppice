use crate::AppConfig;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::io::{Read, Write};
use std::path::Path;
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

        let config_toml = toml::to_string_pretty(config)
            .map_err(|e| BackupError::Config(e.to_string()))?;
        std::fs::write(staging.join(CONFIG_NAME), config_toml)?;

        let db_dump = staging.join(DATABASE_NAME);
        run_pg_dump(&config.database.url, &db_dump)?;

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

    pub async fn import_from_bytes(
        config: &AppConfig,
        pool: &PgPool,
        bytes: &[u8],
    ) -> Result<(), BackupError> {
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

        terminate_other_sessions(pool).await?;
        run_psql_file(&config.database.url, &db_sql)?;

        restore_tree(
            &extract_root.join("data/artifacts"),
            Path::new(&config.storage.artifacts_dir),
        )?;
        restore_tree(
            &extract_root.join("data/worktrees"),
            Path::new(&config.agent.worktrees_path),
        )?;

        Ok(())
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

fn run_pg_dump(database_url: &str, out_path: &Path) -> Result<(), BackupError> {
    let output = std::process::Command::new("pg_dump")
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
    let child = std::process::Command::new("psql")
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
}
