use std::path::{Path, PathBuf};

use crate::config::PluginsConfig;
use crate::crypto::SecretStore;
use crate::mcp::proxy::naming::exposed_name;
use crate::mcp::proxy::{McpServerPool, PoolError, PoolServerSpec, ServerKey};
use crate::plugins::capability::McpServerTransport;
use crate::plugins::discover::{discover, Discovered};
use crate::plugins::git_install::{repo_dir_name, validate_git_url, validate_ref};
use crate::plugins::manifest::PluginManifest;
use crate::plugins::skills::{PluginSkillSet, SkillCatalog};
use crate::services::plugin_settings_service::{PluginSettingsError, PluginSettingsService};
use crate::services::secret_service::SecretError;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

const DIR_COLUMNS: &str = "id, path, position, is_default";

const INSTALL_COLUMNS: &str = "id, plugin_dir_id, kind, git_url, git_ref, plugin_id, status, error";

const PLUGIN_COLUMNS: &str = r#"
    p.id, p.plugin_dir_id, p.rel_path, p.name, p.version, p.description, p.source,
    p.git_url, p.git_ref, p.git_commit, p.manifest::text AS manifest, p.status, p.error, p.enabled
"#;

pub struct PluginService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin not found")]
    NotFound,
    #[error("agent not found")]
    AgentNotFound,
    #[error("plugin install not found")]
    InstallNotFound,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Settings(#[from] PluginSettingsError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginDir {
    pub id: Uuid,
    pub path: String,
    pub position: i32,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginRow {
    pub id: Uuid,
    pub plugin_dir_id: Uuid,
    pub rel_path: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: String,
    pub git_url: Option<String>,
    pub git_ref: Option<String>,
    pub git_commit: Option<String>,
    pub manifest: Option<PluginManifest>,
    pub status: String,
    pub error: Option<String>,
    pub enabled: bool,
}

const UNDECRYPTABLE_SETTINGS: &str = "plugin settings could not be decrypted";

/// Outcome of testing one MCP server; carries names and redacted errors only.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerTestResult {
    pub name: String,
    pub kind: String,
    pub outcome: ServerTestOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerTestOutcome {
    Ok(Vec<TestedTool>),
    Error(String),
    Unsupported { error: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestedTool {
    pub name: String,
    pub exposed_name: String,
    pub description: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginInstall {
    pub id: Uuid,
    pub plugin_dir_id: Uuid,
    pub kind: String,
    pub git_url: String,
    pub git_ref: Option<String>,
    pub plugin_id: Option<Uuid>,
    pub status: String,
    pub error: Option<String>,
}

impl<'a> PluginService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Creates the configured default dir on disk and records it. When the
    /// configured path changes, the existing default row is repointed and its
    /// plugins are forgotten, so a different plugin at the same rel_path in the
    /// new dir cannot inherit `enabled` or git metadata. The caller rescans and
    /// refreshes the skill catalog afterwards.
    pub async fn ensure_default_dir(&self, path: &str) -> Result<PluginDir, PluginError> {
        std::fs::create_dir_all(path)?;
        let canonical = canonical_string(Path::new(path))?;

        let mut tx = self.pool.begin().await?;
        lock_plugin_state(&mut tx).await?;
        let current = sqlx::query(&format!(
            "SELECT {DIR_COLUMNS} FROM plugin_dirs WHERE is_default"
        ))
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| row_to_dir(&row));
        let other = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM plugin_dirs WHERE path = $1 AND NOT is_default",
        )
        .bind(&canonical)
        .fetch_optional(&mut *tx)
        .await?;
        if other.is_some() {
            return Err(PluginError::Conflict(format!(
                "default plugin dir {canonical} is already registered as a plugin dir"
            )));
        }

        let dir = match current {
            Some(dir) if dir.path == canonical => dir,
            Some(dir) => {
                sqlx::query("DELETE FROM plugins WHERE plugin_dir_id = $1")
                    .bind(dir.id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE plugin_dirs SET path = $2 WHERE id = $1")
                    .bind(dir.id)
                    .bind(&canonical)
                    .execute(&mut *tx)
                    .await?;
                PluginDir {
                    path: canonical,
                    ..dir
                }
            }
            None => {
                sqlx::query("UPDATE plugin_dirs SET position = position + 1")
                    .execute(&mut *tx)
                    .await?;
                let id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO plugin_dirs (id, path, position, is_default) VALUES ($1, $2, 0, true)",
                )
                .bind(id)
                .bind(&canonical)
                .execute(&mut *tx)
                .await?;
                PluginDir {
                    id,
                    path: canonical,
                    position: 0,
                    is_default: true,
                }
            }
        };
        tx.commit().await?;
        Ok(dir)
    }

    pub async fn list_dirs(&self) -> Result<Vec<PluginDir>, PluginError> {
        let rows = sqlx::query(&format!(
            "SELECT {DIR_COLUMNS} FROM plugin_dirs ORDER BY position, created_at"
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.iter().map(row_to_dir).collect())
    }

    pub async fn add_dir(&self, path: &str) -> Result<PluginDir, PluginError> {
        let path = Path::new(path.trim());
        if !path.is_absolute() {
            return Err(PluginError::Validation(
                "plugin dir path must be absolute".into(),
            ));
        }
        if !path.is_dir() {
            return Err(PluginError::Validation(format!(
                "plugin dir {} does not exist or is not a directory",
                path.display()
            )));
        }
        let canonical = canonical_string(path)?;

        let mut tx = self.pool.begin().await?;
        lock_plugin_state(&mut tx).await?;
        let id = Uuid::new_v4();
        let position: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO plugin_dirs (id, path, position)
            SELECT $1, $2, COALESCE(MAX(position) + 1, 0) FROM plugin_dirs
            RETURNING position
            "#,
        )
        .bind(id)
        .bind(&canonical)
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| match &err {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                PluginError::Conflict(format!("plugin dir {canonical} is already registered"))
            }
            _ => PluginError::Db(err),
        })?;
        tx.commit().await?;

        self.rescan().await?;
        Ok(PluginDir {
            id,
            path: canonical,
            position,
            is_default: false,
        })
    }

    pub async fn move_dir(&self, id: Uuid, position: i32) -> Result<Vec<PluginDir>, PluginError> {
        if position < 0 {
            return Err(PluginError::Validation(
                "position must be zero or greater".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        lock_plugin_state(&mut tx).await?;
        let mut ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM plugin_dirs ORDER BY position, created_at")
                .fetch_all(&mut *tx)
                .await?;
        let from = ids
            .iter()
            .position(|dir_id| *dir_id == id)
            .ok_or(PluginError::NotFound)?;
        ids.remove(from);
        let to = usize::try_from(position).unwrap_or(0).min(ids.len());
        ids.insert(to, id);
        renumber(&mut tx, &ids).await?;
        tx.commit().await?;

        self.rescan().await?;
        self.list_dirs().await
    }

    pub async fn remove_dir(&self, id: Uuid) -> Result<(), PluginError> {
        let mut tx = self.pool.begin().await?;
        lock_plugin_state(&mut tx).await?;
        let is_default: bool =
            sqlx::query_scalar("SELECT is_default FROM plugin_dirs WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(PluginError::NotFound)?;
        if is_default {
            return Err(PluginError::Conflict(
                "the default plugin dir cannot be removed".into(),
            ));
        }
        sqlx::query("DELETE FROM plugin_dirs WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM plugin_dirs ORDER BY position, created_at")
                .fetch_all(&mut *tx)
                .await?;
        renumber(&mut tx, &ids).await?;
        tx.commit().await?;

        self.rescan().await?;
        Ok(())
    }

    /// Reconciles `plugins` with what is on disk: upserts discovered plugins
    /// (keeping `enabled` and git metadata), marks vanished ones `missing`,
    /// and shadows same-name `ok` plugins in later dirs.
    pub async fn rescan(&self) -> Result<Vec<PluginRow>, PluginError> {
        let mut tx = self.pool.begin().await?;
        lock_plugin_state(&mut tx).await?;
        let dirs: Vec<PluginDir> = sqlx::query(&format!(
            "SELECT {DIR_COLUMNS} FROM plugin_dirs ORDER BY position, created_at"
        ))
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(row_to_dir)
        .collect();

        for dir in &dirs {
            let found = scan_dir(Path::new(&dir.path));
            for discovered in &found {
                upsert_discovered(&mut tx, dir.id, discovered).await?;
            }
            let seen: Vec<String> = found.into_iter().map(|d| d.rel_path).collect();
            sqlx::query(
                r#"
                UPDATE plugins SET status = 'missing', error = NULL, updated_at = now()
                WHERE plugin_dir_id = $1 AND status <> 'missing' AND NOT (rel_path = ANY($2))
                "#,
            )
            .bind(dir.id)
            .bind(&seen)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r#"
            UPDATE plugins SET status = 'shadowed', updated_at = now()
            FROM (
                SELECT p.id, ROW_NUMBER() OVER (
                    PARTITION BY p.name ORDER BY d.position, p.rel_path
                ) AS rank
                FROM plugins p JOIN plugin_dirs d ON d.id = p.plugin_dir_id
                WHERE p.status = 'ok'
            ) ranked
            WHERE plugins.id = ranked.id AND ranked.rank > 1
            "#,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        self.list_plugins().await
    }

    pub async fn list_plugins(&self) -> Result<Vec<PluginRow>, PluginError> {
        let rows = sqlx::query(&format!(
            r#"
            SELECT {PLUGIN_COLUMNS} FROM plugins p
            JOIN plugin_dirs d ON d.id = p.plugin_dir_id
            ORDER BY d.position, p.rel_path
            "#
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.iter().map(row_to_plugin).collect())
    }

    pub async fn get_plugin(&self, id: Uuid) -> Result<PluginRow, PluginError> {
        let row = sqlx::query(&format!(
            "SELECT {PLUGIN_COLUMNS} FROM plugins p WHERE p.id = $1"
        ))
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(PluginError::NotFound)?;
        Ok(row_to_plugin(&row))
    }

    /// Disabling is always allowed; enabling requires status `ok`.
    pub async fn set_enabled(&self, id: Uuid, enabled: bool) -> Result<PluginRow, PluginError> {
        let updated = sqlx::query(
            r#"
            UPDATE plugins SET enabled = $2, updated_at = now()
            WHERE id = $1 AND (NOT $2 OR status = 'ok')
            "#,
        )
        .bind(id)
        .bind(enabled)
        .execute(self.pool)
        .await?
        .rows_affected();
        let plugin = self.get_plugin(id).await?;
        if updated == 0 {
            return Err(PluginError::Conflict(format!(
                "plugin {} has status `{}` and cannot be enabled",
                plugin.name, plugin.status
            )));
        }
        Ok(plugin)
    }

    pub async fn plugin_path(&self, id: Uuid) -> Result<PathBuf, PluginError> {
        let (dir, rel_path): (String, String) = sqlx::query_as(
            "SELECT d.path, p.rel_path FROM plugins p JOIN plugin_dirs d ON d.id = p.plugin_dir_id WHERE p.id = $1",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(PluginError::NotFound)?;
        Ok(join_plugin_path(&dir, &rel_path))
    }

    /// Replaces the catalog's plugin skills with those of enabled `ok` plugins.
    pub async fn refresh_catalog(&self, catalog: &SkillCatalog) -> Result<(), PluginError> {
        let _refresh = catalog.lock_refresh().await;
        let rows = sqlx::query(&format!(
            r#"
            SELECT {PLUGIN_COLUMNS}, d.path AS dir_path FROM plugins p
            JOIN plugin_dirs d ON d.id = p.plugin_dir_id
            WHERE p.enabled AND p.status = 'ok'
            "#
        ))
        .fetch_all(self.pool)
        .await?;
        let sets = rows
            .iter()
            .filter_map(|row| {
                let plugin = row_to_plugin(row);
                let manifest = plugin.manifest.as_ref()?;
                let root = join_plugin_path(&row.get::<String, _>("dir_path"), &plugin.rel_path);
                match std::fs::canonicalize(&root) {
                    Ok(root) => Some(PluginSkillSet::from_manifest(plugin.id, &root, manifest)),
                    Err(err) => {
                        tracing::warn!(plugin = %plugin.name, error = %err, "plugin root unavailable; skills not served");
                        None
                    }
                }
            })
            .collect();
        catalog.set_plugin_skills(sets);
        Ok(())
    }

    /// One spec per MCP server of each plugin in `plugin_ids`, with decrypted
    /// settings. `enabled_only` keeps only enabled `ok` plugins and skips (with a
    /// warning) plugins whose settings cannot be decrypted; otherwise that is an error.
    pub async fn server_specs_for(
        &self,
        plugin_ids: &[Uuid],
        store: &SecretStore,
        enabled_only: bool,
    ) -> Result<Vec<PoolServerSpec>, PluginError> {
        if plugin_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(&format!(
            r#"
            SELECT {PLUGIN_COLUMNS}, d.path AS dir_path FROM plugins p
            JOIN plugin_dirs d ON d.id = p.plugin_dir_id
            WHERE p.id = ANY($1) AND (NOT $2 OR (p.enabled AND p.status = 'ok'))
            ORDER BY d.position, p.rel_path
            "#
        ))
        .bind(plugin_ids)
        .bind(enabled_only)
        .fetch_all(self.pool)
        .await?;
        let settings = PluginSettingsService::new(self.pool, store);
        let mut specs = Vec::new();
        for row in &rows {
            let plugin = row_to_plugin(row);
            let Some(manifest) = plugin.manifest.as_ref() else {
                continue;
            };
            if manifest.mcp_servers.is_empty() {
                continue;
            }
            let root = join_plugin_path(&row.get::<String, _>("dir_path"), &plugin.rel_path);
            let values = match settings.decrypted(plugin.id).await {
                Ok(values) => values,
                Err(err) if enabled_only => {
                    tracing::warn!(plugin = %plugin.name, error = %err, "plugin settings unreadable; MCP servers skipped");
                    continue;
                }
                Err(err) => return Err(err.into()),
            };
            specs.extend(manifest.mcp_servers.iter().map(|entry| PoolServerSpec {
                key: ServerKey {
                    plugin_id: plugin.id,
                    server: entry.name.clone(),
                },
                plugin_name: plugin.name.clone(),
                plugin_root: root.clone(),
                entry: entry.clone(),
                settings: values.clone(),
            }));
        }
        Ok(specs)
    }

    /// Specs for every MCP server of one plugin, whether or not it is enabled.
    pub async fn server_specs(
        &self,
        plugin_id: Uuid,
        store: &SecretStore,
    ) -> Result<Vec<PoolServerSpec>, PluginError> {
        self.server_specs_for(&[plugin_id], store, false).await
    }

    /// Restarts every MCP server of an `ok` plugin (enabled or not) and lists
    /// its tools; results follow manifest order.
    pub async fn test_servers(
        &self,
        plugin_id: Uuid,
        store: &SecretStore,
        mcp: &McpServerPool,
    ) -> Result<Vec<ServerTestResult>, PluginError> {
        let plugin = self.get_plugin(plugin_id).await?;
        if plugin.status != "ok" {
            return Err(PluginError::Conflict("plugin is not ok".into()));
        }
        let entries = plugin.manifest.map(|m| m.mcp_servers).unwrap_or_default();
        let specs = match self.server_specs(plugin_id, store).await {
            Ok(specs) => specs,
            Err(PluginError::Settings(PluginSettingsError::Secret(err)))
                if !matches!(err, SecretError::Database(_)) =>
            {
                tracing::warn!(plugin = %plugin.name, error = %err, "plugin settings unreadable; MCP test skipped");
                return Ok(entries
                    .iter()
                    .map(|entry| ServerTestResult {
                        name: entry.name.clone(),
                        kind: entry.api_kind().to_string(),
                        outcome: ServerTestOutcome::Error(UNDECRYPTABLE_SETTINGS.into()),
                    })
                    .collect());
            }
            Err(err) => return Err(err),
        };
        Ok(futures_util::future::join_all(specs.iter().map(|spec| test_server(mcp, spec))).await)
    }

    /// Validates a git install and records it as `running`; the caller clones
    /// into the returned destination and then calls `finish_install`.
    pub async fn start_install(
        &self,
        git_url: &str,
        git_ref: Option<&str>,
        plugin_dir_id: Uuid,
        cfg: &PluginsConfig,
    ) -> Result<(PluginInstall, PathBuf), PluginError> {
        let git_url = git_url.trim();
        validate_git_url(git_url, cfg.allow_file_git_urls).map_err(PluginError::Validation)?;
        let git_ref = git_ref.map(str::trim).filter(|r| !r.is_empty());
        if let Some(git_ref) = git_ref {
            validate_ref(git_ref).map_err(PluginError::Validation)?;
        }
        let name = repo_dir_name(git_url).map_err(PluginError::Validation)?;

        let mut tx = self.pool.begin().await?;
        lock_plugin_state(&mut tx).await?;
        let dir_path: String = sqlx::query_scalar("SELECT path FROM plugin_dirs WHERE id = $1")
            .bind(plugin_dir_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| {
                PluginError::Validation(format!("plugin dir {plugin_dir_id} does not exist"))
            })?;
        let dest = Path::new(&dir_path).join(&name);
        let running_urls: Vec<String> = sqlx::query_scalar(
            "SELECT git_url FROM plugin_installs WHERE plugin_dir_id = $1 AND kind = 'install' AND status = 'running'",
        )
        .bind(plugin_dir_id)
        .fetch_all(&mut *tx)
        .await?;
        let in_flight = running_urls
            .iter()
            .any(|url| repo_dir_name(url).as_deref() == Ok(name.as_str()));
        if in_flight || dest.symlink_metadata().is_ok() {
            return Err(PluginError::Conflict(format!(
                "{} already exists in the plugin dir",
                dest.display()
            )));
        }
        let install =
            insert_install(&mut tx, plugin_dir_id, "install", git_url, git_ref, None).await?;
        tx.commit().await?;
        Ok((install, dest))
    }

    /// Records a `running` update of a git-installed plugin at its stored ref
    /// and returns the plugin root to update in place and its stored rel_path.
    pub async fn start_update(
        &self,
        plugin_id: Uuid,
    ) -> Result<(PluginInstall, PathBuf, String), PluginError> {
        let plugin = self.get_plugin(plugin_id).await?;
        let git_url = match (plugin.source.as_str(), &plugin.git_url) {
            ("git", Some(url)) => url.clone(),
            _ => {
                return Err(PluginError::Validation(format!(
                    "plugin {} was not installed from git",
                    plugin.name
                )))
            }
        };
        if plugin.status == "missing" {
            return Err(PluginError::Validation(
                "plugin folder is missing; rescan or reinstall".into(),
            ));
        }

        let mut tx = self.pool.begin().await?;
        lock_plugin_state(&mut tx).await?;
        let running: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM plugin_installs WHERE plugin_id = $1 AND status = 'running')",
        )
        .bind(plugin_id)
        .fetch_one(&mut *tx)
        .await?;
        if running {
            return Err(PluginError::Conflict(format!(
                "an update of plugin {} is already running",
                plugin.name
            )));
        }
        let install = insert_install(
            &mut tx,
            plugin.plugin_dir_id,
            "update",
            &git_url,
            plugin.git_ref.as_deref(),
            Some(plugin_id),
        )
        .await?;
        tx.commit().await?;
        Ok((install, self.plugin_path(plugin_id).await?, plugin.rel_path))
    }

    /// Records the git outcome. On success rescans, stamps git metadata on the
    /// plugin at `(plugin_dir_id, dest_rel)` and refreshes the skill catalog;
    /// a fresh install is always left disabled.
    pub async fn finish_install(
        &self,
        id: Uuid,
        result: Result<String, String>,
        dest_rel: &str,
        catalog: &SkillCatalog,
    ) -> Result<PluginInstall, PluginError> {
        let install = self.get_install(id).await?;
        let commit = match result {
            Ok(commit) => commit,
            Err(err) => return self.fail_install(id, &err).await,
        };
        self.rescan().await?;

        let is_install = install.kind == "install";
        let mut tx = self.pool.begin().await?;
        let plugin_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            UPDATE plugins SET
                source = 'git', git_url = $3, git_ref = $4, git_commit = $5,
                enabled = enabled AND NOT $6, updated_at = now()
            WHERE plugin_dir_id = $1 AND rel_path = $2 AND status <> 'missing'
            RETURNING id
            "#,
        )
        .bind(install.plugin_dir_id)
        .bind(dest_rel)
        .bind(&install.git_url)
        .bind(&install.git_ref)
        .bind(&commit)
        .bind(is_install)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(plugin_id) = plugin_id else {
            tx.rollback().await?;
            if !is_install {
                return self
                    .fail_install(id, "updated repository is not a plugin")
                    .await;
            }
            let dir_path: String = sqlx::query_scalar("SELECT path FROM plugin_dirs WHERE id = $1")
                .bind(install.plugin_dir_id)
                .fetch_one(self.pool)
                .await?;
            let dest = Path::new(&dir_path).join(dest_rel);
            if let Err(err) = std::fs::remove_dir_all(&dest) {
                tracing::warn!(dest = %dest.display(), error = %err, "failed to remove non-plugin clone");
            }
            return self
                .fail_install(id, "cloned repository is not a plugin")
                .await;
        };
        sqlx::query(
            r#"
            UPDATE plugin_installs SET status = 'succeeded', plugin_id = $2, error = NULL,
                finished_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(plugin_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        if let Err(err) = self.refresh_catalog(catalog).await {
            tracing::error!(error = %err, "failed to refresh plugin skill catalog");
        }
        self.get_install(id).await
    }

    /// Entry point for background git jobs. If recording the outcome fails, the
    /// install is still failed (best effort) and a fresh clone removed, so a
    /// stuck `running` row cannot block a reinstall.
    pub async fn complete_git_job(
        &self,
        id: Uuid,
        result: Result<String, String>,
        dest_rel: &str,
        catalog: &SkillCatalog,
    ) {
        let Err(err) = self.finish_install(id, result, dest_rel, catalog).await else {
            return;
        };
        tracing::error!(install = %id, error = %err, "failed to finish plugin install");
        if let Err(abandon_err) = self
            .abandon_install(id, &format!("internal error: {err}"), dest_rel)
            .await
        {
            tracing::error!(install = %id, error = %abandon_err, "failed to mark plugin install failed");
        }
    }

    async fn abandon_install(
        &self,
        id: Uuid,
        error: &str,
        dest_rel: &str,
    ) -> Result<(), PluginError> {
        let abandoned: Option<(String, String)> = sqlx::query_as(
            r#"
            UPDATE plugin_installs i SET status = 'failed', error = $2, finished_at = now()
            FROM plugin_dirs d
            WHERE i.id = $1 AND i.status = 'running' AND d.id = i.plugin_dir_id
            RETURNING i.kind, d.path
            "#,
        )
        .bind(id)
        .bind(error)
        .fetch_optional(self.pool)
        .await?;
        let Some((kind, dir_path)) = abandoned else {
            return Ok(());
        };
        if kind == "install" {
            let dest = Path::new(&dir_path).join(dest_rel);
            if let Err(err) = std::fs::remove_dir_all(&dest) {
                tracing::warn!(dest = %dest.display(), error = %err, "failed to remove abandoned clone");
            }
            self.rescan().await?;
        }
        Ok(())
    }

    pub async fn get_install(&self, id: Uuid) -> Result<PluginInstall, PluginError> {
        let row = sqlx::query(&format!(
            "SELECT {INSTALL_COLUMNS} FROM plugin_installs WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(PluginError::InstallNotFound)?;
        Ok(row_to_install(&row))
    }

    /// Installs cannot outlive the process that ran them.
    pub async fn fail_stale_installs(&self) -> Result<u64, PluginError> {
        Ok(sqlx::query(
            r#"
            UPDATE plugin_installs SET status = 'failed', error = 'server restarted',
                finished_at = now()
            WHERE status = 'running'
            "#,
        )
        .execute(self.pool)
        .await?
        .rows_affected())
    }

    async fn fail_install(&self, id: Uuid, error: &str) -> Result<PluginInstall, PluginError> {
        sqlx::query(
            "UPDATE plugin_installs SET status = 'failed', error = $2, finished_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(error)
        .execute(self.pool)
        .await?;
        self.get_install(id).await
    }

    /// Every assigned plugin, whatever its current status.
    pub async fn agent_plugin_ids(&self, agent_id: Uuid) -> Result<Vec<Uuid>, PluginError> {
        self.ensure_agent(agent_id).await?;
        Ok(sqlx::query_scalar(
            "SELECT plugin_id FROM agent_plugins WHERE agent_id = $1 ORDER BY plugin_id",
        )
        .bind(agent_id)
        .fetch_all(self.pool)
        .await?)
    }

    pub async fn set_agent_plugins(
        &self,
        agent_id: Uuid,
        plugin_ids: &[Uuid],
    ) -> Result<Vec<Uuid>, PluginError> {
        self.ensure_agent(agent_id).await?;
        let mut wanted = plugin_ids.to_vec();
        wanted.sort();
        wanted.dedup();
        let usable: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM plugins WHERE id = ANY($1) AND enabled AND status = 'ok'",
        )
        .bind(&wanted)
        .fetch_all(self.pool)
        .await?;
        if let Some(bad) = wanted.iter().find(|id| !usable.contains(id)) {
            return Err(PluginError::Validation(format!(
                "plugin {bad} does not exist or is not enabled"
            )));
        }

        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM agent_plugins WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO agent_plugins (agent_id, plugin_id) SELECT $1, UNNEST($2::uuid[])",
        )
        .bind(agent_id)
        .bind(&wanted)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.agent_plugin_ids(agent_id).await
    }

    /// Assigned plugins that are enabled and `ok` — the per-run token snapshot.
    pub async fn run_plugin_ids(&self, agent_id: Uuid) -> Result<Vec<Uuid>, PluginError> {
        Ok(sqlx::query_scalar(
            r#"
            SELECT p.id FROM agent_plugins ap JOIN plugins p ON p.id = ap.plugin_id
            WHERE ap.agent_id = $1 AND p.enabled AND p.status = 'ok'
            ORDER BY p.id
            "#,
        )
        .bind(agent_id)
        .fetch_all(self.pool)
        .await?)
    }

    /// Assigns preset default plugins by name; names not enabled/`ok` are skipped.
    pub async fn apply_preset_defaults(
        &self,
        agent_id: Uuid,
        names: &[String],
    ) -> Result<(), PluginError> {
        if names.is_empty() {
            return Ok(());
        }
        sqlx::query(
            r#"
            INSERT INTO agent_plugins (agent_id, plugin_id)
            SELECT $1, id FROM plugins WHERE name = ANY($2) AND enabled AND status = 'ok'
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(agent_id)
        .bind(names)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    async fn ensure_agent(&self, agent_id: Uuid) -> Result<(), PluginError> {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM agents WHERE id = $1)")
            .bind(agent_id)
            .fetch_one(self.pool)
            .await?;
        if exists {
            Ok(())
        } else {
            Err(PluginError::AgentNotFound)
        }
    }
}

async fn test_server(mcp: &McpServerPool, spec: &PoolServerSpec) -> ServerTestResult {
    let outcome = if let McpServerTransport::Unsupported { .. } = spec.entry.transport {
        ServerTestOutcome::Unsupported {
            error: spec.entry.error.clone(),
        }
    } else {
        match mcp.test(spec).await {
            Ok(tools) => ServerTestOutcome::Ok(
                tools
                    .into_iter()
                    .map(|tool| TestedTool {
                        exposed_name: exposed_name(&spec.plugin_name, &tool.name),
                        name: tool.name,
                        description: tool.description,
                        read_only: tool.read_only,
                    })
                    .collect(),
            ),
            Err(PoolError::Config(message) | PoolError::Unhealthy(message)) => {
                ServerTestOutcome::Error(message)
            }
            Err(err) => ServerTestOutcome::Error(
                mcp.last_error(&spec.key).unwrap_or_else(|| err.to_string()),
            ),
        }
    };
    ServerTestResult {
        name: spec.entry.name.clone(),
        kind: spec.entry.api_kind().to_string(),
        outcome,
    }
}

fn join_plugin_path(dir: &str, rel_path: &str) -> PathBuf {
    let dir = PathBuf::from(dir);
    if rel_path.is_empty() {
        dir
    } else {
        dir.join(rel_path)
    }
}

async fn lock_plugin_state(tx: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('coppice_plugin_rescan'))")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn renumber(tx: &mut Transaction<'_, Postgres>, ids: &[Uuid]) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE plugin_dirs SET position = ordered.idx - 1
        FROM UNNEST($1::uuid[]) WITH ORDINALITY AS ordered(id, idx)
        WHERE plugin_dirs.id = ordered.id
        "#,
    )
    .bind(ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A dir that is gone or unreadable yields no plugins, so its rows become `missing`.
fn scan_dir(dir: &Path) -> Vec<Discovered> {
    if !dir.is_dir() {
        return Vec::new();
    }
    discover(dir).unwrap_or_else(|err| {
        tracing::warn!(dir = %dir.display(), error = %err, "failed to scan plugin dir");
        Vec::new()
    })
}

async fn upsert_discovered(
    tx: &mut Transaction<'_, Postgres>,
    dir_id: Uuid,
    discovered: &Discovered,
) -> Result<(), PluginError> {
    let (version, description, manifest, status, error) = match &discovered.result {
        Ok(manifest) => (
            manifest.version.clone(),
            manifest.description.clone(),
            serde_json::to_string(manifest).expect("manifest serializes"),
            "ok",
            None,
        ),
        Err(err) => (
            "0.0.0".to_string(),
            String::new(),
            "{}".to_string(),
            "invalid",
            Some(err.clone()),
        ),
    };
    sqlx::query(
        r#"
        INSERT INTO plugins
            (id, plugin_dir_id, rel_path, name, version, description, manifest, status, error)
        VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9)
        ON CONFLICT (plugin_dir_id, rel_path) DO UPDATE SET
            name = EXCLUDED.name,
            version = EXCLUDED.version,
            description = EXCLUDED.description,
            manifest = EXCLUDED.manifest,
            status = EXCLUDED.status,
            error = EXCLUDED.error,
            updated_at = now()
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(dir_id)
    .bind(&discovered.rel_path)
    .bind(&discovered.name)
    .bind(version)
    .bind(description)
    .bind(manifest)
    .bind(status)
    .bind(error)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_install(
    tx: &mut Transaction<'_, Postgres>,
    plugin_dir_id: Uuid,
    kind: &str,
    git_url: &str,
    git_ref: Option<&str>,
    plugin_id: Option<Uuid>,
) -> Result<PluginInstall, sqlx::Error> {
    let row = sqlx::query(&format!(
        r#"
        INSERT INTO plugin_installs (id, plugin_dir_id, kind, git_url, git_ref, plugin_id, status)
        VALUES ($1, $2, $3, $4, $5, $6, 'running')
        RETURNING {INSTALL_COLUMNS}
        "#
    ))
    .bind(Uuid::new_v4())
    .bind(plugin_dir_id)
    .bind(kind)
    .bind(git_url)
    .bind(git_ref)
    .bind(plugin_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row_to_install(&row))
}

fn row_to_install(row: &PgRow) -> PluginInstall {
    PluginInstall {
        id: row.get("id"),
        plugin_dir_id: row.get("plugin_dir_id"),
        kind: row.get("kind"),
        git_url: row.get("git_url"),
        git_ref: row.get("git_ref"),
        plugin_id: row.get("plugin_id"),
        status: row.get("status"),
        error: row.get("error"),
    }
}

fn canonical_string(path: &Path) -> Result<String, PluginError> {
    let canonical = std::fs::canonicalize(path)?;
    canonical
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| PluginError::Validation("plugin dir path must be valid UTF-8".into()))
}

fn row_to_dir(row: &PgRow) -> PluginDir {
    PluginDir {
        id: row.get("id"),
        path: row.get("path"),
        position: row.get("position"),
        is_default: row.get("is_default"),
    }
}

fn row_to_plugin(row: &PgRow) -> PluginRow {
    let manifest: String = row.get("manifest");
    PluginRow {
        id: row.get("id"),
        plugin_dir_id: row.get("plugin_dir_id"),
        rel_path: row.get("rel_path"),
        name: row.get("name"),
        version: row.get("version"),
        description: row.get("description"),
        source: row.get("source"),
        git_url: row.get("git_url"),
        git_ref: row.get("git_ref"),
        git_commit: row.get("git_commit"),
        manifest: serde_json::from_str(&manifest).ok(),
        status: row.get("status"),
        error: row.get("error"),
        enabled: row.get("enabled"),
    }
}
