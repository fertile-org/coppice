use crate::services::worktree_service::{
    init_empty_git_workspace, plugin_build_dir, WorktreeError,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::path::{Path, PathBuf};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

const COLUMNS: &str = "id, answers, plans, status, plugin_name, workspace_path, publish_target, created_at, updated_at";

const TERMINAL: &[&str] = &["accepted", "discarded", "cancelled"];

#[derive(Debug, thiserror::Error)]
pub enum PluginBuildError {
    #[error("plugin build not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Worktree(#[from] WorktreeError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginBuild {
    pub id: Uuid,
    pub answers: Value,
    pub plans: Value,
    pub status: String,
    pub plugin_name: Option<String>,
    pub workspace_path: String,
    pub publish_target: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Default)]
pub struct PluginBuildUpdate {
    pub answers: Option<Value>,
    pub plan_body: Option<String>,
    pub plan_change_request: Option<String>,
    pub plugin_name: Option<Option<String>>,
    pub publish_target: Option<Option<String>>,
}

pub struct PluginBuildService<'a> {
    pool: &'a PgPool,
    worktrees_root: PathBuf,
}

impl<'a> PluginBuildService<'a> {
    pub fn new(pool: &'a PgPool, worktrees_root: impl Into<PathBuf>) -> Self {
        Self {
            pool,
            worktrees_root: worktrees_root.into(),
        }
    }

    pub async fn create(&self) -> Result<PluginBuild, PluginBuildError> {
        let id = Uuid::new_v4();
        let path = plugin_build_dir(&self.worktrees_root, id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        init_empty_git_workspace(&path).await?;
        let workspace_path = path_to_string(&path)?;

        let inserted = sqlx::query(&format!(
            "INSERT INTO plugin_builds (id, status, workspace_path) VALUES ($1, 'drafting', $2) RETURNING {COLUMNS}"
        ))
        .bind(id)
        .bind(&workspace_path)
        .fetch_one(self.pool)
        .await;

        match inserted {
            Ok(row) => Ok(row_to_build(row)),
            Err(err) => {
                let _ = tokio::fs::remove_dir_all(&path).await;
                Err(err.into())
            }
        }
    }

    pub async fn get(&self, id: Uuid) -> Result<PluginBuild, PluginBuildError> {
        let row = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM plugin_builds WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(PluginBuildError::NotFound)?;
        Ok(row_to_build(row))
    }

    /// Newest first. Discarded builds are omitted unless `include_discarded`.
    pub async fn list(&self, include_discarded: bool) -> Result<Vec<PluginBuild>, PluginBuildError> {
        let sql = if include_discarded {
            format!("SELECT {COLUMNS} FROM plugin_builds ORDER BY created_at DESC")
        } else {
            format!(
                "SELECT {COLUMNS} FROM plugin_builds WHERE status <> 'discarded' ORDER BY created_at DESC"
            )
        };
        let rows = sqlx::query(&sql).fetch_all(self.pool).await?;
        Ok(rows.into_iter().map(row_to_build).collect())
    }

    pub async fn update(
        &self,
        id: Uuid,
        update: PluginBuildUpdate,
    ) -> Result<PluginBuild, PluginBuildError> {
        if update.answers.is_none()
            && update.plan_body.is_none()
            && update.plugin_name.is_none()
            && update.publish_target.is_none()
        {
            return Err(PluginBuildError::Validation(
                "at least one of answers, planRevision, pluginName, or publishTarget is required"
                    .into(),
            ));
        }

        let mut build = self.get(id).await?;
        if is_terminal(&build.status) {
            return Err(PluginBuildError::Conflict(format!(
                "cannot update a {} build",
                build.status
            )));
        }

        if let Some(answers) = update.answers {
            if !answers.is_object() {
                return Err(PluginBuildError::Validation(
                    "answers must be a JSON object".into(),
                ));
            }
            build.answers = answers;
        }

        if let Some(body) = update.plan_body {
            let mut plans = build
                .plans
                .as_array()
                .cloned()
                .unwrap_or_default();
            let revision = (plans.len() as i64) + 1;
            let mut entry = json!({
                "revision": revision,
                "body": body,
                "createdAt": OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .unwrap_or_default(),
            });
            if let Some(change_request) = update.plan_change_request {
                entry["changeRequest"] = Value::String(change_request);
            }
            plans.push(entry);
            build.plans = Value::Array(plans);
        }

        if let Some(name) = update.plugin_name {
            build.plugin_name = name.filter(|s| !s.is_empty());
        }
        if let Some(target) = update.publish_target {
            build.publish_target = target.filter(|s| !s.is_empty());
        }

        let row = sqlx::query(&format!(
            "UPDATE plugin_builds
             SET answers = $2, plans = $3, plugin_name = $4, publish_target = $5, updated_at = now()
             WHERE id = $1
             RETURNING {COLUMNS}"
        ))
        .bind(id)
        .bind(&build.answers)
        .bind(&build.plans)
        .bind(&build.plugin_name)
        .bind(&build.publish_target)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_build(row))
    }

    pub async fn cancel(&self, id: Uuid) -> Result<PluginBuild, PluginBuildError> {
        self.set_terminal(id, "cancelled", false).await
    }

    pub async fn discard(&self, id: Uuid) -> Result<PluginBuild, PluginBuildError> {
        self.set_terminal(id, "discarded", true).await
    }

    /// Delete the workspace after Accept (PB-11). Keeps the row; status → accepted.
    pub async fn delete_workspace_on_accept(
        &self,
        id: Uuid,
    ) -> Result<PluginBuild, PluginBuildError> {
        let build = self.get(id).await?;
        if build.status == "discarded" || build.status == "cancelled" {
            return Err(PluginBuildError::Conflict(format!(
                "cannot accept a {} build",
                build.status
            )));
        }
        remove_workspace(Path::new(&build.workspace_path)).await?;
        let row = sqlx::query(&format!(
            "UPDATE plugin_builds SET status = 'accepted', updated_at = now() WHERE id = $1 RETURNING {COLUMNS}"
        ))
        .bind(id)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_build(row))
    }

    async fn set_terminal(
        &self,
        id: Uuid,
        status: &str,
        delete_workspace: bool,
    ) -> Result<PluginBuild, PluginBuildError> {
        let build = self.get(id).await?;
        if is_terminal(&build.status) {
            return Err(PluginBuildError::Conflict(format!(
                "build is already {}",
                build.status
            )));
        }
        if delete_workspace {
            remove_workspace(Path::new(&build.workspace_path)).await?;
        }
        let row = sqlx::query(&format!(
            "UPDATE plugin_builds SET status = $2, updated_at = now() WHERE id = $1 RETURNING {COLUMNS}"
        ))
        .bind(id)
        .bind(status)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_build(row))
    }
}

fn is_terminal(status: &str) -> bool {
    TERMINAL.contains(&status)
}

fn path_to_string(path: &Path) -> Result<String, PluginBuildError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| PluginBuildError::Validation("workspace path is not valid UTF-8".into()))
}

async fn remove_workspace(path: &Path) -> Result<(), PluginBuildError> {
    if path.exists() {
        tokio::fs::remove_dir_all(path).await?;
    }
    Ok(())
}

fn row_to_build(row: sqlx::postgres::PgRow) -> PluginBuild {
    PluginBuild {
        id: row.get("id"),
        answers: row.get("answers"),
        plans: row.get("plans"),
        status: row.get("status"),
        plugin_name: row.get("plugin_name"),
        workspace_path: row.get("workspace_path"),
        publish_target: row.get("publish_target"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::worktree_service::plugin_build_dir;

    #[test]
    fn plugin_build_dir_nests_under_plugin_builds() {
        let root = PathBuf::from("/tmp/worktrees");
        let id = Uuid::from_u128(1);
        assert_eq!(
            plugin_build_dir(&root, id),
            root.join("plugin-builds").join(id.to_string())
        );
    }
}
