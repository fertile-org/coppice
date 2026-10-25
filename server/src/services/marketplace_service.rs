use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::plugins::capability::MARKETPLACE_JSON;
use crate::plugins::catalog::CATALOG_DIR;
use crate::plugins::git_install::{clone, validate_git_url, validate_ref};
use crate::plugins::manifest::is_valid_plugin_name;
use crate::plugins::marketplace::{
    entry_categories, entry_source, expand_marketplace, github_url, is_github_shorthand,
    normalize_source, validate_marketplace,
};
use crate::plugins::skills::SkillCatalog;
use crate::services::plugin_service::{
    insert_install, lock_plugin_state, PluginDir, PluginError, PluginInstall, PluginRow,
    PluginService,
};
use serde_json::{Map, Value};
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

/// Where a plugin folder came from. Stamped on the plugin row after placement.
struct Origin<'a> {
    source_id: Uuid,
    entry: &'a str,
    commit: Option<&'a str>,
}

/// Where an entry's files come from once its source is fetched.
enum EntrySource {
    /// A `path` entry: a folder inside the cached marketplace.
    Path { root: PathBuf, source: String },
    /// An object entry: a git repository, optionally one folder inside it.
    Git {
        url: String,
        git_ref: Option<String>,
        subpath: Option<String>,
    },
}

struct ResolvedEntry {
    source_id: Uuid,
    /// The marketplace's commit; the origin of `path` entries.
    commit: Option<String>,
    from: EntrySource,
}

/// Where a placement lands. `update_of` is set when an existing plugin is replaced.
struct Target {
    dir: PluginDir,
    name: String,
    update_of: Option<Uuid>,
}

#[derive(Clone, Copy, Debug)]
enum Placement {
    /// The folder must not exist yet.
    Install,
    /// The folder exists and is swapped for the new copy, keeping its rows.
    Replace,
}

/// Result of installing or updating an entry. Path entries are placed
/// immediately; object entries return a job that must be run.
#[derive(Debug)]
pub enum EntryOutcome {
    Plugin(Box<PluginRow>),
    Job(Box<MarketplaceJob>),
}

/// A git-backed install or update, recorded as `running` in `plugin_installs`.
#[derive(Debug)]
pub struct MarketplaceJob {
    pub install: PluginInstall,
    /// Set for updates: the plugin whose row id is kept.
    pub update_of: Option<Uuid>,
    clone_dir: PathBuf,
    source_id: Uuid,
    entry: String,
    subpath: Option<String>,
    dir: PluginDir,
    name: String,
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
        let src = resolve_inside(entry.root, entry.source)?;
        let dir = self.default_dir().await?;
        self.place(
            &src,
            &dir,
            entry.name,
            Placement::Install,
            Origin {
                source_id: entry.source_id,
                entry: entry.name,
                commit: entry.commit,
            },
            catalog,
        )
        .await
    }

    /// Installs an entry of a source's cached marketplace into the default
    /// plugin dir. Object entries (`github`, `url`, `git-subdir`) return a job.
    pub async fn install_entry(
        &self,
        source_id: Uuid,
        cache: &Path,
        name: &str,
        allow_file: bool,
        catalog: &SkillCatalog,
    ) -> Result<EntryOutcome, PluginError> {
        if !is_valid_plugin_name(name) {
            return Err(PluginError::Validation(format!(
                "invalid plugin name `{name}`"
            )));
        }
        let resolved = self
            .resolve_entry(source_id, cache, name, allow_file)
            .await?;
        let target = Target {
            dir: self.default_dir().await?,
            name: name.to_string(),
            update_of: None,
        };
        self.run_entry(resolved, name, target, cache, catalog).await
    }

    /// Re-resolves the entry a plugin was installed from and replaces its
    /// folder, keeping the row id, enabled state, settings and agent access.
    pub async fn update_entry(
        &self,
        plugin_id: Uuid,
        cache: &Path,
        allow_file: bool,
        catalog: &SkillCatalog,
    ) -> Result<EntryOutcome, PluginError> {
        let row = sqlx::query(
            r#"
            SELECT p.rel_path, p.status, p.origin_source_id, p.origin_entry,
                d.id AS dir_id, d.path AS dir_path, d.position, d.is_default
            FROM plugins p JOIN plugin_dirs d ON d.id = p.plugin_dir_id
            WHERE p.id = $1
            "#,
        )
        .bind(plugin_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(PluginError::NotFound)?;
        let (Some(source_id), Some(entry)) = (
            row.get::<Option<Uuid>, _>("origin_source_id"),
            row.get::<Option<String>, _>("origin_entry"),
        ) else {
            return Err(PluginError::Validation(
                "plugin was not installed from a marketplace, or its source was removed".into(),
            ));
        };
        let rel_path: String = row.get("rel_path");
        if row.get::<String, _>("status") == "missing" {
            return Err(PluginError::Validation(
                "plugin folder is missing; rescan or reinstall".into(),
            ));
        }
        if rel_path.contains(['/', '#']) {
            return Err(PluginError::Validation(
                "plugin is not a marketplace folder".into(),
            ));
        }
        let dir = PluginDir {
            id: row.get("dir_id"),
            path: row.get("dir_path"),
            position: row.get("position"),
            is_default: row.get("is_default"),
        };
        let resolved = self
            .resolve_entry(source_id, cache, &entry, allow_file)
            .await?;
        let target = Target {
            dir,
            name: rel_path,
            update_of: Some(plugin_id),
        };
        self.run_entry(resolved, &entry, target, cache, catalog)
            .await
    }

    /// Runs a git job: clones, places the folder, and records the outcome on
    /// the install row. Failures are recorded, not returned, except for errors
    /// writing that record.
    pub async fn run_job(
        &self,
        job: MarketplaceJob,
        allow_file: bool,
        timeout: Duration,
        catalog: &SkillCatalog,
    ) -> Result<PluginInstall, PluginError> {
        let outcome = self.execute_job(&job, allow_file, timeout, catalog).await;
        remove_quietly(&job.clone_dir);
        self.finish_job(
            job.install.id,
            outcome.map(|id| vec![id]).map_err(|err| err.to_string()),
        )
        .await
    }

    async fn execute_job(
        &self,
        job: &MarketplaceJob,
        allow_file: bool,
        timeout: Duration,
        catalog: &SkillCatalog,
    ) -> Result<Uuid, PluginError> {
        let commit = clone(
            &job.install.git_url,
            job.install.git_ref.as_deref(),
            &job.clone_dir,
            allow_file,
            timeout,
        )
        .await
        .map_err(PluginError::Validation)?;
        remove_quietly(&job.clone_dir.join(".git"));
        let src = match &job.subpath {
            None => job.clone_dir.clone(),
            Some(sub) => resolve_inside(&job.clone_dir, sub)?,
        };
        let placement = if job.update_of.is_some() {
            Placement::Replace
        } else {
            Placement::Install
        };
        let row = self
            .place(
                &src,
                &job.dir,
                &job.name,
                placement,
                Origin {
                    source_id: job.source_id,
                    entry: &job.entry,
                    commit: Some(&commit),
                },
                catalog,
            )
            .await?;
        Ok(row.id)
    }

    async fn finish_job(
        &self,
        id: Uuid,
        outcome: Result<Vec<Uuid>, String>,
    ) -> Result<PluginInstall, PluginError> {
        match outcome {
            Ok(ids) => {
                sqlx::query(
                    r#"
                    UPDATE plugin_installs SET status = 'succeeded', plugin_id = $2,
                        plugin_ids = $3, error = NULL, finished_at = now()
                    WHERE id = $1
                    "#,
                )
                .bind(id)
                .bind(ids.first().copied())
                .bind(ids.as_slice())
                .execute(self.pool)
                .await?;
            }
            Err(error) => {
                sqlx::query(
                    "UPDATE plugin_installs SET status = 'failed', error = $2, finished_at = now() WHERE id = $1",
                )
                .bind(id)
                .bind(error)
                .execute(self.pool)
                .await?;
            }
        }
        PluginService::new(self.pool).get_install(id).await
    }

    async fn run_entry(
        &self,
        resolved: ResolvedEntry,
        entry: &str,
        target: Target,
        cache: &Path,
        catalog: &SkillCatalog,
    ) -> Result<EntryOutcome, PluginError> {
        let placement = if target.update_of.is_some() {
            Placement::Replace
        } else {
            Placement::Install
        };
        match resolved.from {
            EntrySource::Path { root, source } => {
                let src = resolve_inside(&root, &source)?;
                let row = self
                    .place(
                        &src,
                        &target.dir,
                        &target.name,
                        placement,
                        Origin {
                            source_id: resolved.source_id,
                            entry,
                            commit: resolved.commit.as_deref(),
                        },
                        catalog,
                    )
                    .await?;
                Ok(EntryOutcome::Plugin(Box::new(row)))
            }
            EntrySource::Git {
                url,
                git_ref,
                subpath,
            } => self
                .start_job(
                    resolved.source_id,
                    entry,
                    (url, git_ref, subpath),
                    target,
                    cache,
                )
                .await
                .map(|job| EntryOutcome::Job(Box::new(job))),
        }
    }

    /// Records a `running` install or update and picks its clone folder. The
    /// clone itself happens in `run_job`.
    async fn start_job(
        &self,
        source_id: Uuid,
        entry: &str,
        (url, git_ref, subpath): (String, Option<String>, Option<String>),
        target: Target,
        cache: &Path,
    ) -> Result<MarketplaceJob, PluginError> {
        let mut tx = self.pool.begin().await?;
        lock_plugin_state(&mut tx).await?;
        match target.update_of {
            None => {
                let live: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM plugins WHERE name = $1 AND status <> 'missing')",
                )
                .bind(&target.name)
                .fetch_one(&mut *tx)
                .await?;
                if live
                    || Path::new(&target.dir.path)
                        .join(&target.name)
                        .symlink_metadata()
                        .is_ok()
                {
                    return Err(PluginError::Conflict(format!(
                        "plugin {} already exists",
                        target.name
                    )));
                }
            }
            Some(plugin_id) => {
                let running: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM plugin_installs WHERE plugin_ids && $1 AND status = 'running')",
                )
                .bind(vec![plugin_id])
                .fetch_one(&mut *tx)
                .await?;
                if running {
                    return Err(PluginError::Conflict(format!(
                        "an update of plugin {} is already running",
                        target.name
                    )));
                }
            }
        }
        let kind = if target.update_of.is_some() {
            "update"
        } else {
            "install"
        };
        let ids: Vec<Uuid> = target.update_of.into_iter().collect();
        let install =
            insert_install(&mut tx, target.dir.id, kind, &url, git_ref.as_deref(), &ids).await?;
        tx.commit().await?;

        let installs = cache.join(INSTALLS_DIR);
        std::fs::create_dir_all(&installs)?;
        Ok(MarketplaceJob {
            clone_dir: installs.join(install.id.to_string()),
            install,
            update_of: target.update_of,
            source_id,
            entry: entry.to_string(),
            subpath,
            dir: target.dir,
            name: target.name,
        })
    }

    /// Copies `src` into `dir` as `name`, rescans, stamps the origin and
    /// refreshes the skill catalog. On failure the folder is removed, and an
    /// update's previous copy is restored.
    async fn place(
        &self,
        src: &Path,
        dir: &PluginDir,
        name: &str,
        placement: Placement,
        origin: Origin<'_>,
        catalog: &SkillCatalog,
    ) -> Result<PluginRow, PluginError> {
        let base = PathBuf::from(&dir.path);
        let dest = base.join(name);
        let staging = base.join(format!(".{name}.staging-{}", Uuid::new_v4()));
        let previous = base.join(format!(".{name}.previous-{}", Uuid::new_v4()));
        copy_entry_tree(src, &staging).inspect_err(|_| remove_quietly(&staging))?;

        let claim = async {
            let mut tx = self.pool.begin().await?;
            lock_plugin_state(&mut tx).await?;
            match placement {
                Placement::Install => {
                    let live: bool = sqlx::query_scalar(
                        "SELECT EXISTS (SELECT 1 FROM plugins WHERE name = $1 AND status <> 'missing')",
                    )
                    .bind(name)
                    .fetch_one(&mut *tx)
                    .await?;
                    if live || dest.symlink_metadata().is_ok() {
                        return Err(PluginError::Conflict(format!("plugin {name} already exists")));
                    }
                    std::fs::rename(&staging, &dest)?;
                }
                Placement::Replace => {
                    if !dest.is_dir() {
                        return Err(PluginError::Validation(
                            "plugin folder is missing; rescan or reinstall".into(),
                        ));
                    }
                    std::fs::rename(&dest, &previous)?;
                    if let Err(err) = std::fs::rename(&staging, &dest) {
                        let _ = std::fs::rename(&previous, &dest);
                        return Err(err.into());
                    }
                }
            }
            tx.commit().await?;
            Ok::<(), PluginError>(())
        }
        .await;
        if let Err(err) = claim {
            remove_quietly(&staging);
            return Err(err);
        }

        let plugins = PluginService::new(self.pool);
        let placed = async {
            plugins.rescan().await?;
            let row = self
                .stamp_origin(&dir.id, name, &origin)
                .await?
                .ok_or_else(|| {
                    PluginError::Validation(format!("{name} is not a plugin or skills folder"))
                })?;
            self.ensure_unique_name(&row).await?;
            Ok::<PluginRow, PluginError>(row)
        }
        .await;
        match placed {
            Ok(row) => {
                remove_quietly(&previous);
                if let Err(err) = plugins.refresh_catalog(catalog).await {
                    tracing::error!(error = %err, "failed to refresh plugin skill catalog");
                }
                Ok(row)
            }
            Err(err) => {
                remove_quietly(&dest);
                if matches!(placement, Placement::Replace) && previous.symlink_metadata().is_ok() {
                    if let Err(restore) = std::fs::rename(&previous, &dest) {
                        tracing::error!(error = %restore, "failed to restore previous plugin copy");
                    }
                }
                if let Err(rescan) = plugins.rescan().await {
                    tracing::error!(error = %rescan, "failed to rescan after marketplace placement");
                }
                Err(err)
            }
        }
    }

    async fn default_dir(&self) -> Result<PluginDir, PluginError> {
        PluginService::new(self.pool)
            .list_dirs()
            .await?
            .into_iter()
            .find(|dir| dir.is_default)
            .ok_or_else(|| PluginError::Validation("default plugin dir is not configured".into()))
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
        origin: &Origin<'_>,
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
        .bind(origin.source_id)
        .bind(origin.entry)
        .bind(origin.commit)
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

    async fn resolve_entry(
        &self,
        source_id: Uuid,
        cache: &Path,
        entry: &str,
        allow_file: bool,
    ) -> Result<ResolvedEntry, PluginError> {
        let source = self.get_source(source_id).await?;
        let (root, _) = source_root(cache, &source);
        if !root.join(MARKETPLACE_JSON).is_file() {
            return Err(PluginError::Validation(
                "marketplace has not been fetched yet".into(),
            ));
        }
        let value = entry_source(&root, entry).ok_or(PluginError::NotFound)?;
        let from = match value {
            Value::String(path) => EntrySource::Path { root, source: path },
            Value::Object(object) => object_source(&object, allow_file)?,
            _ => {
                return Err(PluginError::Validation(
                    "source must be a path or an object".into(),
                ))
            }
        };
        Ok(ResolvedEntry {
            source_id,
            commit: source.commit_sha,
            from,
        })
    }
}

/// Parses an object `source` (`github`, `url`, `git-subdir`). `npm` and unknown
/// kinds are refused here, so they never reach the git job.
fn object_source(
    object: &Map<String, Value>,
    allow_file: bool,
) -> Result<EntrySource, PluginError> {
    let field = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let required = |key: &str| {
        field(key).ok_or_else(|| PluginError::Validation(format!("source is missing `{key}`")))
    };
    let kind = field("source").unwrap_or_default();
    let (url, subpath) = match kind {
        "github" => {
            let repo = required("repo")?;
            if !is_github_shorthand(repo) {
                return Err(PluginError::Validation(
                    "github source repo must be owner/name".into(),
                ));
            }
            (github_url(repo), None)
        }
        "url" => (required("url")?.to_string(), None),
        "git-subdir" => {
            let url = required("url")?;
            let url = if is_github_shorthand(url) {
                github_url(url)
            } else {
                url.to_string()
            };
            let path = required("path")?;
            let subpath = normalize_source(path)
                .filter(|rel| !rel.is_empty())
                .ok_or_else(|| {
                    PluginError::Validation(
                        "git-subdir path must stay inside the repository".into(),
                    )
                })?;
            (url, Some(subpath))
        }
        "npm" => {
            return Err(PluginError::Validation(
                "npm sources are not supported".into(),
            ))
        }
        other => {
            return Err(PluginError::Validation(format!(
                "unknown source kind `{other}`"
            )))
        }
    };
    validate_git_url(&url, allow_file).map_err(PluginError::Validation)?;
    let git_ref = field("ref").map(str::to_string);
    if let Some(git_ref) = &git_ref {
        validate_ref(git_ref).map_err(PluginError::Validation)?;
    }
    Ok(EntrySource::Git {
        url,
        git_ref,
        subpath,
    })
}

/// Canonical path of `source` under `root`. Refuses `..`, absolute paths and
/// anything that resolves outside `root`, including through symlinks.
fn resolve_inside(root: &Path, source: &str) -> Result<PathBuf, PluginError> {
    let rel = normalize_source(source)
        .ok_or_else(|| PluginError::Validation("source must stay inside the marketplace".into()))?;
    let root = std::fs::canonicalize(root)?;
    let target = std::fs::canonicalize(root.join(&rel))
        .map_err(|_| PluginError::Validation("source path does not exist".into()))?;
    if !target.starts_with(&root) || !target.is_dir() {
        return Err(PluginError::Validation(
            "source must be a folder inside the marketplace".into(),
        ));
    }
    Ok(target)
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
    /// Only computed for `path` entries; object entries need a remote check.
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

/// Folder under the marketplace cache where object-entry clones are made.
const INSTALLS_DIR: &str = ".installs";

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
                            let (installable, reason) = match kind.as_str() {
                                "github" | "url" | "git-subdir" => (true, None),
                                "npm" => (false, Some("npm sources are not supported".to_string())),
                                _ => (false, Some("unknown source kind".to_string())),
                            };
                            (
                                kind,
                                installable,
                                reason,
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
                let update_available = origin.is_some_and(|o| {
                    kind == "path"
                        && match source.kind.as_str() {
                            "builtin" => o.version != version,
                            _ => o.commit.as_deref() != source.commit_sha.as_deref(),
                        }
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

    #[test]
    fn object_sources_parse_and_refuse_npm() {
        let parse = |value: Value| object_source(value.as_object().unwrap(), false);
        match parse(serde_json::json!({ "source": "github", "repo": "acme/tools", "ref": "v1" })) {
            Ok(EntrySource::Git {
                url,
                git_ref,
                subpath,
            }) => {
                assert_eq!(url, "https://github.com/acme/tools.git");
                assert_eq!(git_ref.as_deref(), Some("v1"));
                assert!(subpath.is_none());
            }
            _ => panic!("github source should parse"),
        }
        match parse(
            serde_json::json!({ "source": "git-subdir", "url": "acme/mono", "path": "tools/x" }),
        ) {
            Ok(EntrySource::Git { subpath, .. }) => assert_eq!(subpath.as_deref(), Some("tools/x")),
            _ => panic!("git-subdir should parse"),
        }
        assert!(parse(
            serde_json::json!({ "source": "git-subdir", "url": "acme/mono", "path": "../x" })
        )
        .is_err());
        assert!(parse(serde_json::json!({ "source": "github", "repo": "../evil" })).is_err());
        assert!(matches!(
            parse(serde_json::json!({ "source": "npm", "package": "@acme/pkg" })),
            Err(PluginError::Validation(_))
        ));
        assert!(parse(serde_json::json!({ "source": "url", "url": "--upload-pack=x" })).is_err());
    }
}
