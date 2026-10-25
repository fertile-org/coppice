use std::path::{Path, PathBuf};

use crate::plugins::manifest::is_valid_plugin_name;
use crate::plugins::marketplace::normalize_source;
use crate::plugins::skills::SkillCatalog;
use crate::services::plugin_service::{lock_plugin_state, PluginError, PluginRow, PluginService};
use sqlx::PgPool;
use uuid::Uuid;

pub struct MarketplaceService<'a> {
    pool: &'a PgPool,
}

/// One `path`-sourced entry of a marketplace folder, already on disk.
pub struct PathEntry<'a> {
    pub source_id: Uuid,
    /// The marketplace folder that holds `.claude-plugin/marketplace.json`.
    pub root: &'a Path,
    /// The entry's `source` value, relative to `root`.
    pub source: &'a str,
    /// The entry's `name`; becomes the folder name in the default plugin dir.
    pub name: &'a str,
    pub commit: Option<&'a str>,
}

impl<'a> MarketplaceService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Copies a path entry into the default plugin dir as `<name>`, rescans,
    /// and stamps the row with its marketplace origin. The row id is assigned
    /// by the rescan and is stable across later updates of the same folder.
    pub async fn install_path_entry(
        &self,
        entry: PathEntry<'_>,
        catalog: &SkillCatalog,
    ) -> Result<PluginRow, PluginError> {
        if !is_valid_plugin_name(entry.name) {
            return Err(PluginError::Validation(format!(
                "invalid plugin name `{}`",
                entry.name
            )));
        }
        let rel = normalize_source(entry.source).ok_or_else(|| {
            PluginError::Validation("source must stay inside the marketplace".into())
        })?;
        let root = std::fs::canonicalize(entry.root)?;
        let src = std::fs::canonicalize(root.join(&rel))
            .map_err(|_| PluginError::Validation("source path does not exist".into()))?;
        if !src.starts_with(&root) || !src.is_dir() {
            return Err(PluginError::Validation(
                "source must be a folder inside the marketplace".into(),
            ));
        }

        let plugins = PluginService::new(self.pool);
        let default_dir = plugins
            .list_dirs()
            .await?
            .into_iter()
            .find(|dir| dir.is_default)
            .ok_or_else(|| {
                PluginError::Validation("default plugin dir is not configured".into())
            })?;
        let dir = PathBuf::from(&default_dir.path);
        let dest = dir.join(entry.name);
        let staging = dir.join(format!(".{}.installing-{}", entry.name, Uuid::new_v4()));

        copy_entry_tree(&src, &staging).inspect_err(|_| remove_quietly(&staging))?;

        let mut tx = self.pool.begin().await?;
        lock_plugin_state(&mut tx).await?;
        let live_elsewhere: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM plugins WHERE name = $1 AND status <> 'missing')",
        )
        .bind(entry.name)
        .fetch_one(&mut *tx)
        .await?;
        if live_elsewhere || dest.symlink_metadata().is_ok() {
            remove_quietly(&staging);
            return Err(PluginError::Conflict(format!(
                "plugin {} already exists",
                entry.name
            )));
        }
        if let Err(err) = std::fs::rename(&staging, &dest) {
            remove_quietly(&staging);
            return Err(err.into());
        }
        tx.commit().await?;

        let stamped = match plugins.rescan().await {
            Ok(_) => self.stamp_origin(&default_dir.id, entry.name, &entry).await,
            Err(err) => Err(err),
        };
        let row = match stamped {
            Ok(Some(row)) => row,
            Ok(None) => {
                remove_quietly(&dest);
                plugins.rescan().await?;
                return Err(PluginError::Validation(format!(
                    "{} is not a plugin or skills folder",
                    entry.name
                )));
            }
            Err(err) => {
                remove_quietly(&dest);
                plugins.rescan().await?;
                return Err(err);
            }
        };
        if let Err(err) = plugins.refresh_catalog(catalog).await {
            tracing::error!(error = %err, "failed to refresh plugin skill catalog");
        }
        Ok(row)
    }

    async fn stamp_origin(
        &self,
        dir_id: &Uuid,
        name: &str,
        entry: &PathEntry<'_>,
    ) -> Result<Option<PluginRow>, PluginError> {
        let updated = sqlx::query(
            r#"
            UPDATE plugins SET origin_source_id = $3, origin_entry = $4, origin_commit = $5,
                updated_at = now()
            WHERE plugin_dir_id = $1 AND rel_path = $2
            "#,
        )
        .bind(dir_id)
        .bind(name)
        .bind(entry.source_id)
        .bind(entry.name)
        .bind(entry.commit)
        .execute(self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Ok(None);
        }
        let row = PluginService::new(self.pool)
            .list_plugins()
            .await?
            .into_iter()
            .find(|plugin| plugin.plugin_dir_id == *dir_id && plugin.rel_path == name);
        Ok(row)
    }
}

/// Copies a plugin folder, refusing any symlink. Links are not followed and
/// not recreated, so nothing outside `src` can end up readable through `dst`.
fn copy_entry_tree(src: &Path, dst: &Path) -> Result<(), PluginError> {
    std::fs::create_dir(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(PluginError::Validation(format!(
                "{} is a symlink; marketplace entries may not contain symlinks",
                entry.file_name().to_string_lossy()
            )));
        } else if kind.is_dir() {
            copy_entry_tree(&from, &to)?;
        } else if kind.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn remove_quietly(path: &Path) {
    let result = match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(_) => return,
    };
    if let Err(err) = result {
        tracing::warn!(path = %path.display(), error = %err, "failed to remove marketplace install leftovers");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_nested_files_and_refuses_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("src");
        std::fs::create_dir_all(src.join("skills/a")).unwrap();
        std::fs::write(src.join("skills/a/SKILL.md"), "x").unwrap();
        let dst = root.path().join("dst");
        copy_entry_tree(&src, &dst).expect("plain tree copies");
        assert!(dst.join("skills/a/SKILL.md").is_file());

        #[cfg(unix)]
        {
            let outside = root.path().join("secret");
            std::fs::write(&outside, "s").unwrap();
            std::os::unix::fs::symlink(&outside, src.join("link")).unwrap();
            let err = copy_entry_tree(&src, &root.path().join("dst2"))
                .expect_err("symlink must be refused");
            assert!(matches!(err, PluginError::Validation(_)), "{err:?}");
            assert!(!root.path().join("dst2/link").exists());
        }
    }
}
