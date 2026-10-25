use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::plugins::capability::MARKETPLACE_JSON;
use crate::plugins::catalog::CATALOG_DIR;
use crate::plugins::git_install::{clone, validate_git_url, validate_ref};
use crate::plugins::manifest::is_valid_plugin_name;
use crate::plugins::marketplace::{
    entry_categories, entry_source, expand_marketplace, normalize_source, validate_marketplace,
};
use crate::plugins::skills::SkillCatalog;
use crate::services::plugin_service::{lock_plugin_state, PluginError, PluginRow, PluginService};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use time::OffsetDateTime;
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
            Ok(_) => match self.stamp_origin(&default_dir.id, entry.name, &entry).await {
                Ok(Some(row)) => self.ensure_unique_name(&row).await.map(|_| Some(row)),
                other => other,
            },
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

    /// The pre-copy check only sees the folder name. The row name comes from
    /// `plugin.json`, so a clash found after the rescan must still roll back.
    async fn ensure_unique_name(&self, row: &PluginRow) -> Result<(), PluginError> {
        let clash: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM plugins WHERE name = $1 AND id <> $2 AND status <> 'missing')",
        )
        .bind(&row.name)
        .bind(row.id)
        .fetch_one(self.pool)
        .await?;
        if clash {
            return Err(PluginError::Conflict(format!(
                "plugin name {} is already used by another plugin",
                row.name
            )));
        }
        Ok(())
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

#[derive(Debug, Clone, PartialEq)]
pub struct MarketplaceSource {
    pub id: Uuid,
    pub name: String,
    /// `builtin` (embedded Coppice catalog) or `git`.
    pub kind: String,
    pub git_url: Option<String>,
    pub git_ref: Option<String>,
    pub commit_sha: Option<String>,
    pub refreshed_at: Option<OffsetDateTime>,
    pub last_error: Option<String>,
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketplaceEntry {
    pub source_id: Uuid,
    pub source_name: String,
    pub name: String,
    pub description: String,
    pub version: String,
    /// `path` for in-repo entries; otherwise the object source kind
    /// (`github`, `url`, `git-subdir`, `npm`, ...).
    pub kind: String,
    pub installable: bool,
    pub reason: Option<String>,
    /// `category` from the source's `marketplace.json`, when set.
    pub category: Option<String>,
    pub installed_plugin_id: Option<Uuid>,
    pub update_available: bool,
}

#[derive(Debug, Default)]
pub struct EntryFilter<'a> {
    /// Case-insensitive match on entry name or description.
    pub query: Option<&'a str>,
    pub source_id: Option<Uuid>,
    /// Exact match on the entry's `category`.
    pub category: Option<&'a str>,
}

const SOURCE_COLUMNS: &str =
    "id, name, kind, git_url, git_ref, commit_sha, refreshed_at, last_error, position";

const BUILTIN_SOURCE_NAME: &str = "coppice";

impl<'a> MarketplaceService<'a> {
    /// Ensures the built-in Coppice source row exists, writes the embedded
    /// catalog into `cache`, and checks that `cache` is outside every plugin dir.
    pub async fn seed_builtin(&self, cache: &Path) -> Result<(), PluginError> {
        let dirs = PluginService::new(self.pool).list_dirs().await?;
        let dir_paths: Vec<&Path> = dirs.iter().map(|d| Path::new(d.path.as_str())).collect();
        crate::plugins::catalog::check_cache_outside_plugin_dirs(cache, &dir_paths)?;
        crate::plugins::catalog::materialize_catalog(cache)?;
        sqlx::query(
            r#"
            INSERT INTO marketplace_sources (id, name, kind, position)
            VALUES ($1, $2, 'builtin', 0)
            ON CONFLICT (name) DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(BUILTIN_SOURCE_NAME)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_sources(&self) -> Result<Vec<MarketplaceSource>, PluginError> {
        let rows = sqlx::query(&format!(
            "SELECT {SOURCE_COLUMNS} FROM marketplace_sources ORDER BY position, name"
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.iter().map(row_to_source).collect())
    }

    pub async fn get_source(&self, id: Uuid) -> Result<MarketplaceSource, PluginError> {
        let row = sqlx::query(&format!(
            "SELECT {SOURCE_COLUMNS} FROM marketplace_sources WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(PluginError::NotFound)?;
        Ok(row_to_source(&row))
    }

    /// Registers a git marketplace. Nothing is cloned here; call
    /// [`Self::refresh_source`] to fetch it.
    pub async fn add_git_source(
        &self,
        name: &str,
        git_url: &str,
        git_ref: Option<&str>,
        allow_file: bool,
    ) -> Result<MarketplaceSource, PluginError> {
        let name = name.trim();
        if !is_valid_plugin_name(name) || name == BUILTIN_SOURCE_NAME {
            return Err(PluginError::Validation(format!(
                "invalid marketplace name `{name}`"
            )));
        }
        validate_git_url(git_url, allow_file).map_err(PluginError::Validation)?;
        if let Some(git_ref) = git_ref {
            validate_ref(git_ref).map_err(PluginError::Validation)?;
        }
        let taken: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM marketplace_sources WHERE name = $1)")
                .bind(name)
                .fetch_one(self.pool)
                .await?;
        if taken {
            return Err(PluginError::Conflict(format!(
                "marketplace {name} already exists"
            )));
        }
        let row = sqlx::query(&format!(
            r#"
            INSERT INTO marketplace_sources (id, name, kind, git_url, git_ref, position)
            VALUES ($1, $2, 'git', $3, $4,
                (SELECT COALESCE(MAX(position), 0) + 1 FROM marketplace_sources))
            RETURNING {SOURCE_COLUMNS}
            "#
        ))
        .bind(Uuid::new_v4())
        .bind(name)
        .bind(git_url)
        .bind(git_ref)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_source(&row))
    }

    /// Removes a git source and its cache folder. Plugins installed from it
    /// stay installed; their origin is cleared by the foreign key.
    pub async fn remove_source(&self, id: Uuid, cache: &Path) -> Result<(), PluginError> {
        let source = self.get_source(id).await?;
        if source.kind == "builtin" {
            return Err(PluginError::Conflict(
                "the built-in Coppice catalog cannot be removed".into(),
            ));
        }
        sqlx::query("DELETE FROM marketplace_sources WHERE id = $1")
            .bind(id)
            .execute(self.pool)
            .await?;
        remove_quietly(&cache.join(id.to_string()));
        Ok(())
    }

    /// Fetches a git source into `<cache>/<id>` and records the commit. The
    /// previous clone stays in place when the fetch or validation fails.
    /// Runs without holding the plugin state lock.
    pub async fn refresh_source(
        &self,
        id: Uuid,
        cache: &Path,
        allow_file: bool,
        timeout: Duration,
    ) -> Result<MarketplaceSource, PluginError> {
        let source = self.get_source(id).await?;
        if source.kind == "builtin" {
            crate::plugins::catalog::materialize_catalog(cache)?;
            return self.mark_refreshed(id, None).await;
        }
        let url = source
            .git_url
            .clone()
            .ok_or_else(|| PluginError::Validation("marketplace has no git URL".into()))?;
        let staging = cache.join(format!(".{id}.refresh-{}", Uuid::new_v4()));
        let fetched = clone(
            &url,
            source.git_ref.as_deref(),
            &staging,
            allow_file,
            timeout,
        )
        .await
        .and_then(|commit| {
            validate_marketplace(&staging)
                .map(|()| commit)
                .map_err(|reason| format!("invalid marketplace: {reason}"))
        });
        let commit = match fetched {
            Ok(commit) => commit,
            Err(reason) => {
                remove_quietly(&staging);
                self.record_error(id, &reason).await?;
                return Err(PluginError::Validation(reason));
            }
        };
        let root = cache.join(id.to_string());
        remove_quietly(&root);
        if let Err(err) = std::fs::rename(&staging, &root) {
            remove_quietly(&staging);
            return Err(err.into());
        }
        self.mark_refreshed(id, Some(&commit)).await
    }

    async fn mark_refreshed(
        &self,
        id: Uuid,
        commit: Option<&str>,
    ) -> Result<MarketplaceSource, PluginError> {
        let row = sqlx::query(&format!(
            r#"
            UPDATE marketplace_sources
            SET commit_sha = $2, refreshed_at = now(), last_error = NULL
            WHERE id = $1
            RETURNING {SOURCE_COLUMNS}
            "#
        ))
        .bind(id)
        .bind(commit)
        .fetch_optional(self.pool)
        .await?
        .ok_or(PluginError::NotFound)?;
        Ok(row_to_source(&row))
    }

    async fn record_error(&self, id: Uuid, reason: &str) -> Result<(), PluginError> {
        sqlx::query("UPDATE marketplace_sources SET last_error = $2 WHERE id = $1")
            .bind(id)
            .bind(reason)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Installs one `path` entry of a source's cached marketplace. Object
    /// sources (`github`, `url`, `git-subdir`, `npm`) are not installed yet.
    pub async fn install_entry(
        &self,
        id: Uuid,
        cache: &Path,
        name: &str,
        catalog: &SkillCatalog,
    ) -> Result<PluginRow, PluginError> {
        let source = self.get_source(id).await?;
        let (root, _) = source_root(cache, &source);
        if !root.join(MARKETPLACE_JSON).is_file() {
            return Err(PluginError::Validation(
                "marketplace has not been fetched yet".into(),
            ));
        }
        let source_value = entry_source(&root, name).ok_or(PluginError::NotFound)?;
        let path = match &source_value {
            serde_json::Value::String(path) => path.clone(),
            serde_json::Value::Object(object) => {
                let kind = object
                    .get("source")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown");
                return Err(PluginError::Validation(format!(
                    "installs from {kind} sources are not supported yet"
                )));
            }
            _ => {
                return Err(PluginError::Validation(
                    "source must be a path or an object".into(),
                ))
            }
        };
        self.install_path_entry(
            PathEntry {
                source_id: source.id,
                root: &root,
                source: &path,
                name,
                commit: source.commit_sha.as_deref(),
            },
            catalog,
        )
        .await
    }

    /// Lists entries of every source whose cache folder exists. A source with
    /// no cache (never refreshed, or failed before any fetch) contributes none.
    pub async fn list_entries(
        &self,
        cache: &Path,
        filter: &EntryFilter<'_>,
    ) -> Result<Vec<MarketplaceEntry>, PluginError> {
        let sources = self.list_sources().await?;
        let installed = self.installed_origins().await?;
        let query = filter.query.map(str::to_lowercase);
        let mut out = Vec::new();
        for source in &sources {
            if filter.source_id.is_some_and(|id| id != source.id) {
                continue;
            }
            let (root, rel) = source_root(cache, source);
            if !root.join(MARKETPLACE_JSON).is_file() {
                continue;
            }
            let categories = entry_categories(&root);
            for found in expand_marketplace(&root, &rel) {
                let category = categories.get(&found.name).cloned();
                if filter
                    .category
                    .is_some_and(|c| category.as_deref() != Some(c))
                {
                    continue;
                }
                let (kind, installable, reason, version, description) = match &found.result {
                    Ok(manifest) => match &manifest.external {
                        None => (
                            "path".to_string(),
                            true,
                            None,
                            manifest.version.clone(),
                            manifest.description.clone(),
                        ),
                        Some(external) => {
                            let kind = external
                                .kind
                                .split(':')
                                .next()
                                .unwrap_or_default()
                                .to_string();
                            let reason = if kind == "npm" {
                                "npm sources are not supported".to_string()
                            } else {
                                format!("installs from {kind} sources are not supported yet")
                            };
                            (
                                kind,
                                false,
                                Some(reason),
                                manifest.version.clone(),
                                manifest.description.clone(),
                            )
                        }
                    },
                    Err(reason) => (
                        "unknown".to_string(),
                        false,
                        Some(reason.clone()),
                        String::new(),
                        String::new(),
                    ),
                };
                if let Some(q) = &query {
                    let hit = found.name.to_lowercase().contains(q)
                        || description.to_lowercase().contains(q);
                    if !hit {
                        continue;
                    }
                }
                let origin = installed.get(&(source.id, found.name.clone()));
                let update_available = origin.is_some_and(|o| match source.kind.as_str() {
                    "builtin" => o.version != version,
                    _ => o.commit.as_deref() != source.commit_sha.as_deref(),
                });
                out.push(MarketplaceEntry {
                    source_id: source.id,
                    source_name: source.name.clone(),
                    name: found.name.clone(),
                    description,
                    version,
                    kind,
                    installable,
                    reason,
                    category,
                    installed_plugin_id: origin.map(|o| o.id),
                    update_available,
                });
            }
        }
        Ok(out)
    }

    async fn installed_origins(
        &self,
    ) -> Result<std::collections::HashMap<(Uuid, String), InstalledOrigin>, PluginError> {
        let rows = sqlx::query(
            r#"
            SELECT id, origin_source_id, origin_entry, origin_commit, version FROM plugins
            WHERE origin_source_id IS NOT NULL AND origin_entry IS NOT NULL AND status <> 'missing'
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|row| {
                let source: Uuid = row.get("origin_source_id");
                let entry: String = row.get("origin_entry");
                (
                    (source, entry),
                    InstalledOrigin {
                        id: row.get("id"),
                        commit: row.get("origin_commit"),
                        version: row.get("version"),
                    },
                )
            })
            .collect())
    }
}

struct InstalledOrigin {
    id: Uuid,
    commit: Option<String>,
    version: String,
}

/// The folder a source's entries are read from, and the `folder_rel` prefix
/// used for their discovered paths.
fn source_root(cache: &Path, source: &MarketplaceSource) -> (PathBuf, String) {
    if source.kind == "builtin" {
        (cache.join(CATALOG_DIR), CATALOG_DIR.to_string())
    } else {
        (cache.join(source.id.to_string()), source.id.to_string())
    }
}

fn row_to_source(row: &PgRow) -> MarketplaceSource {
    MarketplaceSource {
        id: row.get("id"),
        name: row.get("name"),
        kind: row.get("kind"),
        git_url: row.get("git_url"),
        git_ref: row.get("git_ref"),
        commit_sha: row.get("commit_sha"),
        refreshed_at: row.get("refreshed_at"),
        last_error: row.get("last_error"),
        position: row.get("position"),
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
