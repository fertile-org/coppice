use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::config::PluginsConfig;
use crate::crypto::SecretStore;
use crate::mcp::proxy::naming::exposed_name;
use crate::mcp::proxy::{McpServerPool, PluginServerNames, PoolError, PoolServerSpec, ServerKey};
use crate::plugins::capability::McpServerTransport;
use crate::plugins::discover::{discover, Discovered};
use crate::plugins::git_install::{repo_dir_name, validate_git_url, validate_ref};
use crate::plugins::manifest::{PluginLayout, PluginManifest};
use crate::plugins::skills::{PluginSkillSet, SkillCatalog};
use crate::services::plugin_settings_service::{PluginSettingsError, PluginSettingsService};
use crate::services::secret_service::SecretError;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

const DIR_COLUMNS: &str = "id, path, position, is_default";

const INSTALL_COLUMNS: &str =
    "id, plugin_dir_id, kind, git_url, git_ref, plugin_id, plugin_ids, status, error";

/// Matches rows of dir `$1` produced from the clone folder `$2`: the folder
/// itself, in-repo entries (`<root>/…`) and external or error entries (`<root>#…`).
const CLONE_ROWS: &str = "plugin_dir_id = $1 AND (rel_path = $2 OR left(rel_path, length($2) + 1) IN ($2 || '/', $2 || '#'))";

const NO_PLUGIN_IN_REPO: &str = "no plugin, skills, or marketplace found in this repository";

const PLUGIN_COLUMNS: &str = r#"
    p.id, p.plugin_dir_id, p.rel_path, p.name, p.version, p.description, p.source,
    p.git_url, p.git_ref, p.git_commit, p.manifest::text AS manifest, p.status, p.error, p.enabled,
    p.git_root, p.disabled_skills, p.agent_access
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
    pub git_root: Option<String>,
    pub disabled_skills: Vec<String>,
    /// `all` (every agent, including ones created later) or `explicit`.
    pub agent_access: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginAgentAssignment {
    pub mode: String,
    pub agent_ids: Vec<Uuid>,
}

/// Install default. A local (stdio) MCP server is assigned to nobody until
/// sandboxing exists. A skills-only plugin with no local server is available
/// to all agents. Anything else starts as an empty explicit list.
pub fn default_agent_access(manifest: &PluginManifest) -> &'static str {
    let runs_locally = manifest
        .mcp_servers
        .iter()
        .any(|server| matches!(server.transport, McpServerTransport::Stdio { .. }));
    if runs_locally {
        "explicit"
    } else if manifest.layout == PluginLayout::SkillsOnly {
        "all"
    } else {
        "explicit"
    }
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
    pub plugin_ids: Vec<Uuid>,
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

    pub async fn ok_plugin_ids(&self) -> Result<HashSet<Uuid>, PluginError> {
        let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM plugins WHERE status = 'ok'")
            .fetch_all(self.pool)
            .await?;
        Ok(ids.into_iter().collect())
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
        if updated == 0 && plugin.status == "external" {
            return Err(PluginError::Conflict(
                "external plugins cannot be enabled".into(),
            ));
        }
        if updated == 0 {
            return Err(PluginError::Conflict(format!(
                "plugin {} has status `{}` and cannot be enabled",
                plugin.name, plugin.status
            )));
        }
        Ok(plugin)
    }

    /// Switches one manifest skill on or off by name.
    pub async fn set_skill_enabled(
        &self,
        plugin_id: Uuid,
        skill: &str,
        enabled: bool,
    ) -> Result<PluginRow, PluginError> {
        let plugin = self.get_plugin(plugin_id).await?;
        let known = plugin
            .manifest
            .as_ref()
            .is_some_and(|m| m.skills.iter().any(|s| s.name == skill));
        if !known {
            return Err(PluginError::NotFound);
        }
        sqlx::query(
            r#"
            UPDATE plugins SET
                disabled_skills = CASE WHEN $3
                    THEN array_remove(disabled_skills, $2)
                    ELSE array_append(array_remove(disabled_skills, $2), $2)
                END,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(plugin_id)
        .bind(skill)
        .bind(enabled)
        .execute(self.pool)
        .await?;
        self.get_plugin(plugin_id).await
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
                    Ok(root) => Some(PluginSkillSet::from_manifest(
                        plugin.id,
                        &root,
                        manifest,
                        &plugin.disabled_skills,
                    )),
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

    /// Names of the enabled `ok` plugins among `plugin_ids` that declare MCP servers,
    /// with their server names. Settings are not read.
    pub async fn mcp_server_names_for(
        &self,
        plugin_ids: &[Uuid],
    ) -> Result<Vec<PluginServerNames>, PluginError> {
        if plugin_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(&format!(
            r#"
            SELECT {PLUGIN_COLUMNS} FROM plugins p
            WHERE p.id = ANY($1) AND p.enabled AND p.status = 'ok'
            "#
        ))
        .bind(plugin_ids)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(row_to_plugin)
            .filter_map(|plugin| {
                let manifest = plugin.manifest?;
                (!manifest.mcp_servers.is_empty()).then(|| PluginServerNames {
                    plugin_id: plugin.id,
                    plugin_name: plugin.name,
                    servers: manifest.mcp_servers.into_iter().map(|e| e.name).collect(),
                })
            })
            .collect())
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
            insert_install(&mut tx, plugin_dir_id, "install", git_url, git_ref, &[]).await?;
        tx.commit().await?;
        Ok((install, dest))
    }

    /// Records a `running` update of the clone a git-installed plugin came from,
    /// at its stored ref, and returns the clone folder to update in place and its
    /// rel path (`git_root`, or the plugin's rel_path for rows predating it).
    pub async fn start_update(
        &self,
        plugin_id: Uuid,
    ) -> Result<(PluginInstall, PathBuf, String), PluginError> {
        let plugin = self.get_plugin(plugin_id).await?;
        if plugin.status == "external" {
            return Err(PluginError::Conflict(
                "external plugins are installed from their own repository".into(),
            ));
        }
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

        let root = plugin.git_root.clone().unwrap_or(plugin.rel_path.clone());

        let mut tx = self.pool.begin().await?;
        lock_plugin_state(&mut tx).await?;
        let rows: Vec<(Uuid, String)> = sqlx::query_as(&format!(
            "SELECT id, status FROM plugins WHERE {CLONE_ROWS} ORDER BY rel_path"
        ))
        .bind(plugin.plugin_dir_id)
        .bind(&root)
        .fetch_all(&mut *tx)
        .await?;
        let all_ids: Vec<Uuid> = rows.iter().map(|(id, _)| *id).collect();
        let running: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM plugin_installs WHERE plugin_ids && $1 AND status = 'running')",
        )
        .bind(&all_ids)
        .fetch_one(&mut *tx)
        .await?;
        if running {
            return Err(PluginError::Conflict(format!(
                "an update of plugin {} is already running",
                plugin.name
            )));
        }
        let ids: Vec<Uuid> = rows
            .into_iter()
            .filter(|(_, status)| status != "missing")
            .map(|(id, _)| id)
            .collect();
        let install = insert_install(
            &mut tx,
            plugin.plugin_dir_id,
            "update",
            &git_url,
            plugin.git_ref.as_deref(),
            &ids,
        )
        .await?;
        let dir_path: String = sqlx::query_scalar("SELECT path FROM plugin_dirs WHERE id = $1")
            .bind(plugin.plugin_dir_id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok((install, join_plugin_path(&dir_path, &root), root))
    }

    /// Records the git outcome. On success rescans, stamps git metadata on every
    /// row the clone folder `dest_rel` produced and refreshes the skill catalog;
    /// a fresh install leaves them all disabled.
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
        let plugin_ids: Vec<Uuid> = sqlx::query_scalar(&format!(
            r#"
            WITH stamped AS (
                UPDATE plugins SET
                    source = 'git', git_url = $3, git_ref = $4, git_commit = $5, git_root = $2,
                    enabled = enabled AND NOT $6, updated_at = now()
                WHERE {CLONE_ROWS} AND status <> 'missing'
                RETURNING id, rel_path
            )
            SELECT id FROM stamped ORDER BY rel_path
            "#
        ))
        .bind(install.plugin_dir_id)
        .bind(dest_rel)
        .bind(&install.git_url)
        .bind(&install.git_ref)
        .bind(&commit)
        .bind(is_install)
        .fetch_all(&mut *tx)
        .await?;
        let Some(&plugin_id) = plugin_ids.first() else {
            tx.rollback().await?;
            if !is_install {
                return self.fail_install(id, NO_PLUGIN_IN_REPO).await;
            }
            let dir_path: String = sqlx::query_scalar("SELECT path FROM plugin_dirs WHERE id = $1")
                .bind(install.plugin_dir_id)
                .fetch_one(self.pool)
                .await?;
            let dest = Path::new(&dir_path).join(dest_rel);
            if let Err(err) = std::fs::remove_dir_all(&dest) {
                tracing::warn!(dest = %dest.display(), error = %err, "failed to remove non-plugin clone");
            }
            return self.fail_install(id, NO_PLUGIN_IN_REPO).await;
        };
        sqlx::query(
            r#"
            UPDATE plugin_installs SET status = 'succeeded', plugin_id = $2,
                plugin_ids = $3, error = NULL, finished_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(plugin_id)
        .bind(&plugin_ids)
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

    /// Plugins this agent can use. `all` counts only while the plugin is enabled
    /// and `ok`, so a disabled "all agents" choice is not dropped by saving an
    /// agent. Explicit rows are returned even when the plugin is unavailable.
    pub async fn agent_plugin_ids(&self, agent_id: Uuid) -> Result<Vec<Uuid>, PluginError> {
        self.ensure_agent(agent_id).await?;
        Ok(sqlx::query_scalar(
            r#"
            SELECT p.id FROM plugins p
            WHERE (p.agent_access = 'all' AND p.enabled AND p.status = 'ok')
               OR (
                    p.agent_access = 'explicit'
                    AND EXISTS (
                        SELECT 1 FROM agent_plugins ap
                        WHERE ap.plugin_id = p.id AND ap.agent_id = $1
                    )
               )
            ORDER BY p.id
            "#,
        )
        .bind(agent_id)
        .fetch_all(self.pool)
        .await?)
    }

    /// Updates the same per-plugin assignment the plugin card edits.
    /// Leaving an "all agents" plugin removes this agent and keeps everyone else.
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

        let current = self.agent_plugin_ids(agent_id).await?;
        let mut tx = self.pool.begin().await?;
        for plugin_id in current.iter().filter(|id| !wanted.contains(id)) {
            detach_agent(&mut tx, *plugin_id, agent_id).await?;
        }
        for plugin_id in wanted.iter().filter(|id| !current.contains(id)) {
            attach_agent(&mut tx, *plugin_id, agent_id).await?;
        }
        tx.commit().await?;
        self.agent_plugin_ids(agent_id).await
    }

    pub async fn plugin_assignment(
        &self,
        plugin_id: Uuid,
    ) -> Result<PluginAgentAssignment, PluginError> {
        let plugin = self.get_plugin(plugin_id).await?;
        let agent_ids = if plugin.agent_access == "all" {
            Vec::new()
        } else {
            explicit_agent_ids(self.pool, plugin_id).await?
        };
        Ok(PluginAgentAssignment {
            mode: plugin.agent_access,
            agent_ids,
        })
    }

    pub async fn explicit_members(
        &self,
        plugin_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, Vec<Uuid>>, PluginError> {
        let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
            r#"
            SELECT plugin_id, agent_id FROM agent_plugins
            WHERE plugin_id = ANY($1)
            ORDER BY agent_id
            "#,
        )
        .bind(plugin_ids)
        .fetch_all(self.pool)
        .await?;
        let mut members: std::collections::HashMap<Uuid, Vec<Uuid>> =
            std::collections::HashMap::new();
        for (plugin_id, agent_id) in rows {
            members.entry(plugin_id).or_default().push(agent_id);
        }
        Ok(members)
    }

    /// Replaces this plugin's assignment. `all` drops the explicit list.
    pub async fn set_plugin_agents(
        &self,
        plugin_id: Uuid,
        mode: &str,
        agent_ids: &[Uuid],
    ) -> Result<PluginAgentAssignment, PluginError> {
        self.get_plugin(plugin_id).await?;
        match mode {
            "all" => {
                let mut tx = self.pool.begin().await?;
                sqlx::query(
                    "UPDATE plugins SET agent_access = 'all', updated_at = now() WHERE id = $1",
                )
                .bind(plugin_id)
                .execute(&mut *tx)
                .await?;
                sqlx::query("DELETE FROM agent_plugins WHERE plugin_id = $1")
                    .bind(plugin_id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                Ok(PluginAgentAssignment {
                    mode: "all".into(),
                    agent_ids: Vec::new(),
                })
            }
            "explicit" => {
                let mut ids = agent_ids.to_vec();
                ids.sort();
                ids.dedup();
                let found: Vec<Uuid> =
                    sqlx::query_scalar("SELECT id FROM agents WHERE id = ANY($1)")
                        .bind(&ids)
                        .fetch_all(self.pool)
                        .await?;
                if found.len() != ids.len() {
                    return Err(PluginError::Validation("unknown agent".into()));
                }
                let mut tx = self.pool.begin().await?;
                sqlx::query(
                    "UPDATE plugins SET agent_access = 'explicit', updated_at = now() WHERE id = $1",
                )
                .bind(plugin_id)
                .execute(&mut *tx)
                .await?;
                sqlx::query("DELETE FROM agent_plugins WHERE plugin_id = $1")
                    .bind(plugin_id)
                    .execute(&mut *tx)
                    .await?;
                if !ids.is_empty() {
                    sqlx::query(
                        "INSERT INTO agent_plugins (agent_id, plugin_id) SELECT UNNEST($1::uuid[]), $2",
                    )
                    .bind(&ids)
                    .bind(plugin_id)
                    .execute(&mut *tx)
                    .await?;
                }
                tx.commit().await?;
                Ok(PluginAgentAssignment {
                    mode: "explicit".into(),
                    agent_ids: ids,
                })
            }
            _ => Err(PluginError::Validation(
                "mode must be all or explicit".into(),
            )),
        }
    }

    /// Assigned plugins that are enabled and `ok` — the per-run token snapshot.
    /// Read when the run starts, so a later assignment waits until the next run.
    pub async fn run_plugin_ids(&self, agent_id: Uuid) -> Result<Vec<Uuid>, PluginError> {
        Ok(sqlx::query_scalar(
            r#"
            SELECT p.id FROM plugins p
            WHERE p.enabled AND p.status = 'ok'
              AND (
                    p.agent_access = 'all'
                    OR (
                        p.agent_access = 'explicit'
                        AND EXISTS (
                            SELECT 1 FROM agent_plugins ap
                            WHERE ap.plugin_id = p.id AND ap.agent_id = $1
                        )
                    )
              )
            ORDER BY p.id
            "#,
        )
        .bind(agent_id)
        .fetch_all(self.pool)
        .await?)
    }

    /// Assigns preset default plugins by name. `all` plugins already include the
    /// new agent. Names that are not enabled and `ok` are skipped.
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
            SELECT $1, id FROM plugins
            WHERE name = ANY($2) AND enabled AND status = 'ok' AND agent_access = 'explicit'
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

async fn explicit_agent_ids(pool: &PgPool, plugin_id: Uuid) -> Result<Vec<Uuid>, PluginError> {
    Ok(sqlx::query_scalar(
        "SELECT agent_id FROM agent_plugins WHERE plugin_id = $1 ORDER BY agent_id",
    )
    .bind(plugin_id)
    .fetch_all(pool)
    .await?)
}

async fn detach_agent(
    tx: &mut Transaction<'_, Postgres>,
    plugin_id: Uuid,
    agent_id: Uuid,
) -> Result<(), PluginError> {
    let mode: Option<String> =
        sqlx::query_scalar("SELECT agent_access FROM plugins WHERE id = $1 FOR UPDATE")
            .bind(plugin_id)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(mode) = mode else {
        return Ok(());
    };
    if mode == "all" {
        sqlx::query(
            "UPDATE plugins SET agent_access = 'explicit', updated_at = now() WHERE id = $1",
        )
        .bind(plugin_id)
        .execute(&mut **tx)
        .await?;
        sqlx::query("DELETE FROM agent_plugins WHERE plugin_id = $1")
            .bind(plugin_id)
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "INSERT INTO agent_plugins (agent_id, plugin_id) SELECT id, $1 FROM agents WHERE id <> $2",
        )
        .bind(plugin_id)
        .bind(agent_id)
        .execute(&mut **tx)
        .await?;
    } else {
        sqlx::query("DELETE FROM agent_plugins WHERE plugin_id = $1 AND agent_id = $2")
            .bind(plugin_id)
            .bind(agent_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn attach_agent(
    tx: &mut Transaction<'_, Postgres>,
    plugin_id: Uuid,
    agent_id: Uuid,
) -> Result<(), PluginError> {
    let mode: String =
        sqlx::query_scalar("SELECT agent_access FROM plugins WHERE id = $1 FOR UPDATE")
            .bind(plugin_id)
            .fetch_one(&mut **tx)
            .await?;
    if mode == "all" {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO agent_plugins (agent_id, plugin_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(agent_id)
    .bind(plugin_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
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
            if manifest.external.is_some() {
                "external"
            } else {
                "ok"
            },
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
    let skill_names: Vec<&str> = match &discovered.result {
        Ok(manifest) => manifest.skills.iter().map(|s| s.name.as_str()).collect(),
        Err(_) => Vec::new(),
    };
    let agent_access = match &discovered.result {
        Ok(manifest) => default_agent_access(manifest),
        Err(_) => "explicit",
    };
    sqlx::query(
        r#"
        INSERT INTO plugins
            (id, plugin_dir_id, rel_path, name, version, description, manifest, status, error, agent_access)
        VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9, $11)
        ON CONFLICT (plugin_dir_id, rel_path) DO UPDATE SET
            name = EXCLUDED.name,
            version = EXCLUDED.version,
            description = EXCLUDED.description,
            manifest = EXCLUDED.manifest,
            status = EXCLUDED.status,
            error = EXCLUDED.error,
            disabled_skills = CASE WHEN EXCLUDED.status = 'ok'
                THEN ARRAY(
                    SELECT s FROM unnest(plugins.disabled_skills) AS s
                    WHERE s = ANY($10::text[])
                )
                ELSE plugins.disabled_skills
            END,
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
    .bind(skill_names)
    .bind(agent_access)
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
    plugin_ids: &[Uuid],
) -> Result<PluginInstall, sqlx::Error> {
    let row = sqlx::query(&format!(
        r#"
        INSERT INTO plugin_installs
            (id, plugin_dir_id, kind, git_url, git_ref, plugin_id, plugin_ids, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'running')
        RETURNING {INSTALL_COLUMNS}
        "#
    ))
    .bind(Uuid::new_v4())
    .bind(plugin_dir_id)
    .bind(kind)
    .bind(git_url)
    .bind(git_ref)
    .bind(plugin_ids.first())
    .bind(plugin_ids)
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
        plugin_ids: row.get("plugin_ids"),
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
        git_root: row.get("git_root"),
        disabled_skills: row.get("disabled_skills"),
        agent_access: row.get("agent_access"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::capability::McpServerEntry;
    use std::collections::BTreeMap;

    fn manifest(layout: PluginLayout, mcp_servers: Vec<McpServerEntry>) -> PluginManifest {
        PluginManifest {
            name: "plugin".into(),
            version: "0.0.0".into(),
            description: String::new(),
            author: None,
            layout,
            skills: Vec::new(),
            mcp_servers,
            unsupported: Vec::new(),
            marketplace: None,
            external: None,
        }
    }

    fn stdio_server() -> McpServerEntry {
        McpServerEntry {
            name: "echo".into(),
            transport: McpServerTransport::Stdio {
                command: "node".into(),
                args: Vec::new(),
                env: BTreeMap::new(),
            },
            error: None,
        }
    }

    fn http_server() -> McpServerEntry {
        McpServerEntry {
            name: "remote".into(),
            transport: McpServerTransport::Http {
                url: "http://127.0.0.1:9".into(),
                headers: BTreeMap::new(),
            },
            error: None,
        }
    }

    #[test]
    fn skills_only_plugins_default_to_all_agents() {
        assert_eq!(
            default_agent_access(&manifest(PluginLayout::SkillsOnly, Vec::new())),
            "all"
        );
    }

    #[test]
    fn local_mcp_plugins_default_to_no_agents() {
        assert_eq!(
            default_agent_access(&manifest(PluginLayout::Plugin, vec![stdio_server()])),
            "explicit"
        );
        assert_eq!(
            default_agent_access(&manifest(PluginLayout::SkillsOnly, vec![stdio_server()])),
            "explicit"
        );
    }

    #[test]
    fn remote_mcp_plugins_default_to_no_agents() {
        assert_eq!(
            default_agent_access(&manifest(PluginLayout::Plugin, vec![http_server()])),
            "explicit"
        );
        assert_eq!(
            default_agent_access(&manifest(PluginLayout::Plugin, Vec::new())),
            "explicit"
        );
    }
}
