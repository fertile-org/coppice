use std::path::{Path, PathBuf};

use crate::plugins::discover::{discover, Discovered};
use crate::plugins::manifest::PluginManifest;
use crate::plugins::skills::{PluginSkillSet, SkillCatalog};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

const DIR_COLUMNS: &str = "id, path, position, is_default";

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
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
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

impl<'a> PluginService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Creates the configured default dir on disk and records it. When the
    /// configured path changes, the existing default row is repointed.
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
