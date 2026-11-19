use crate::domain::agent::Agent;
use crate::domain::plugin_build::{
    JOB_TYPE_BUILD_PLUGIN, JOB_TYPE_PLAN_PLUGIN_BUILD, NO_AGENT_READY_TO_BUILD,
};
use crate::domain::run::AgentRun;
use crate::providers::{AgentRunResult, PlanQuestion};
use crate::services::agent_service::AgentService;
use crate::services::planning_service::planner_for_ticket;
use crate::services::run_service::{RunError, RunService};
use crate::services::workflow_service::is_implementer;
use crate::services::worktree_service::{
    init_empty_git_workspace, plugin_build_dir, WorktreeError,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::path::{Path, PathBuf};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

const COLUMNS: &str = "id, answers, plans, status, plugin_name, workspace_path, publish_target, plan_questions, created_at, updated_at";

const TERMINAL: &[&str] = &["accepted", "discarded", "cancelled"];

const ONE_ACTIVE_RUN_INDEX: &str = "agent_runs_one_active_plugin_build_idx";

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

impl From<RunError> for PluginBuildError {
    fn from(err: RunError) -> Self {
        match err {
            RunError::Validation(msg) => PluginBuildError::Validation(msg),
            RunError::ActiveRunExists => PluginBuildError::Conflict(
                "a plan or build run is already active for this build".into(),
            ),
            RunError::NotFound => PluginBuildError::NotFound,
            RunError::Database(e) => PluginBuildError::Db(e),
        }
    }
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
    pub plan_questions: Option<Value>,
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
            let mut plans = build.plans.as_array().cloned().unwrap_or_default();
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

    /// Queue a `plan_plugin_build` run. Requires saved answers. `connector_ready`
    /// returns whether the chosen agent's connector can run.
    ///
    /// Stop/cancel of the *run* must not call build `cancel`/`discard` — the
    /// build stays at `planning` / `plan_ready` so the admin can start again.
    pub async fn queue_plan_run(
        &self,
        id: Uuid,
        change_request: Option<String>,
        connector_ready: impl Fn(&str) -> bool,
    ) -> Result<(PluginBuild, AgentRun), PluginBuildError> {
        let mut build = self.get(id).await?;
        if is_terminal(&build.status) {
            return Err(PluginBuildError::Conflict(format!(
                "cannot plan a {} build",
                build.status
            )));
        }
        if !answers_ready(&build.answers) {
            return Err(PluginBuildError::Validation(
                "save the five answers before starting a plan run".into(),
            ));
        }
        if matches!(build.status.as_str(), "building" | "ready_to_try" | "accepted") {
            return Err(PluginBuildError::Conflict(format!(
                "cannot plan while status is {}",
                build.status
            )));
        }

        let agents = AgentService::new(self.pool)
            .list_agents()
            .await
            .map_err(|err| PluginBuildError::Validation(err.to_string()))?;
        let Some(planner) = planner_for_ticket(&agents) else {
            return Err(PluginBuildError::Conflict(NO_AGENT_READY_TO_BUILD.into()));
        };
        if !connector_ready(&planner.connector) {
            return Err(PluginBuildError::Conflict(NO_AGENT_READY_TO_BUILD.into()));
        }

        if let Some(note) = change_request.filter(|s| !s.trim().is_empty()) {
            // Ask-for-changes: keep the note for the next plan revision context.
            let mut answers = build.answers.as_object().cloned().unwrap_or_default();
            answers.insert("_changeRequest".into(), Value::String(note));
            build.answers = Value::Object(answers);
        }

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE plugin_builds SET answers = $2, status = 'planning', plan_questions = NULL, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(&build.answers)
        .execute(&mut *tx)
        .await?;

        let run = match RunService::create_plugin_build_run(
            &mut tx,
            id,
            planner.id,
            JOB_TYPE_PLAN_PLUGIN_BUILD,
        )
        .await
        {
            Ok(run) => run,
            Err(RunError::Database(err)) if is_unique_violation(&err, ONE_ACTIVE_RUN_INDEX) => {
                return Err(PluginBuildError::Conflict(
                    "a plan or build run is already active for this build".into(),
                ));
            }
            Err(err) => return Err(err.into()),
        };
        tx.commit().await?;
        Ok((self.get(id).await?, run))
    }

    /// Queue a `build_plugin` run from `plan_ready` with at least one plan revision.
    ///
    /// Stopping the agent run must not call build `cancel`/`discard`. On run
    /// cancel/fail the worker reverts status to `plan_ready` so Try again works.
    pub async fn queue_build_run(
        &self,
        id: Uuid,
        connector_ready: impl Fn(&str) -> bool,
    ) -> Result<(PluginBuild, AgentRun), PluginBuildError> {
        let build = self.get(id).await?;
        if build.status != "plan_ready" {
            return Err(PluginBuildError::Conflict(format!(
                "build runs require status plan_ready (got {})",
                build.status
            )));
        }
        if !has_plan_revision(&build.plans) {
            return Err(PluginBuildError::Validation(
                "approve a plan before starting a build run".into(),
            ));
        }

        let agents = AgentService::new(self.pool)
            .list_agents()
            .await
            .map_err(|err| PluginBuildError::Validation(err.to_string()))?;
        let Some(builder) = builder_for_plugin(&agents) else {
            return Err(PluginBuildError::Conflict(NO_AGENT_READY_TO_BUILD.into()));
        };
        if !connector_ready(&builder.connector) {
            return Err(PluginBuildError::Conflict(NO_AGENT_READY_TO_BUILD.into()));
        }

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE plugin_builds SET status = 'building', updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        let run = match RunService::create_plugin_build_run(
            &mut tx,
            id,
            builder.id,
            JOB_TYPE_BUILD_PLUGIN,
        )
        .await
        {
            Ok(run) => run,
            Err(RunError::Database(err)) if is_unique_violation(&err, ONE_ACTIVE_RUN_INDEX) => {
                return Err(PluginBuildError::Conflict(
                    "a plan or build run is already active for this build".into(),
                ));
            }
            Err(err) => return Err(err.into()),
        };
        tx.commit().await?;
        Ok((self.get(id).await?, run))
    }

    /// Apply a successful plan result: questions stay in `planning`; a plan
    /// appends a revision and moves to `plan_ready`.
    pub async fn apply_plan_result(
        &self,
        build_id: Uuid,
        result: &AgentRunResult,
    ) -> Result<PluginBuild, PluginBuildError> {
        let AgentRunResult::Done {
            summary,
            plan_questions,
            advanced,
            ..
        } = result
        else {
            return Err(PluginBuildError::Validation(
                "plugin plan runs must finish with status done".into(),
            ));
        };

        if !plan_questions.is_empty() {
            let questions = serde_json::to_value(plan_questions)
                .map_err(|err| PluginBuildError::Validation(err.to_string()))?;
            let row = sqlx::query(&format!(
                "UPDATE plugin_builds
                 SET plan_questions = $2, status = 'planning', updated_at = now()
                 WHERE id = $1
                 RETURNING {COLUMNS}"
            ))
            .bind(build_id)
            .bind(questions)
            .fetch_one(self.pool)
            .await?;
            return Ok(row_to_build(row));
        }

        let mut build = self.get(build_id).await?;
        let mut plans = build.plans.as_array().cloned().unwrap_or_default();
        let revision = (plans.len() as i64) + 1;
        let mut entry = json!({
            "revision": revision,
            "body": summary,
            "createdAt": OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_default(),
        });
        if let Some(change) = build
            .answers
            .get("_changeRequest")
            .and_then(|v| v.as_str())
        {
            entry["changeRequest"] = Value::String(change.to_string());
        }
        if let Some(advanced) = advanced {
            entry["advanced"] = advanced.clone();
        }
        plans.push(entry);
        build.plans = Value::Array(plans);

        let plugin_name = advanced
            .as_ref()
            .and_then(|v| v.get("pluginName"))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .filter(|s| !s.is_empty())
            .or(build.plugin_name.clone());

        // Clear the pending ask-for-changes note once consumed.
        if let Some(obj) = build.answers.as_object_mut() {
            obj.remove("_changeRequest");
        }

        let row = sqlx::query(&format!(
            "UPDATE plugin_builds
             SET plans = $2, answers = $3, plugin_name = $4, plan_questions = NULL,
                 status = 'plan_ready', updated_at = now()
             WHERE id = $1
             RETURNING {COLUMNS}"
        ))
        .bind(build_id)
        .bind(&build.plans)
        .bind(&build.answers)
        .bind(&plugin_name)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_build(row))
    }

    /// Build success: keep status `building` for later PB-7/8 steps.
    pub async fn apply_build_result(
        &self,
        build_id: Uuid,
        result: &AgentRunResult,
    ) -> Result<PluginBuild, PluginBuildError> {
        let AgentRunResult::Done { summary, .. } = result else {
            return Err(PluginBuildError::Validation(
                "plugin build runs must finish with status done".into(),
            ));
        };
        if summary.trim().is_empty() {
            return Err(PluginBuildError::Validation(
                "plugin build runs need a non-empty summary".into(),
            ));
        }
        // Status stays `building` until format check / install (later tickets).
        self.get(build_id).await
    }

    /// On plan/build run cancel or failure: revert build runs to `plan_ready`
    /// so Try again works; plan runs stay `planning`. Never calls cancel/discard.
    pub async fn on_run_terminal(
        &self,
        build_id: Uuid,
        job_type: &str,
        succeeded: bool,
    ) -> Result<(), PluginBuildError> {
        if succeeded {
            return Ok(());
        }
        let status = if job_type == JOB_TYPE_BUILD_PLUGIN {
            "plan_ready"
        } else if job_type == JOB_TYPE_PLAN_PLUGIN_BUILD {
            "planning"
        } else {
            return Ok(());
        };
        sqlx::query(
            "UPDATE plugin_builds SET status = $2, updated_at = now()
             WHERE id = $1 AND status IN ('planning', 'building')",
        )
        .bind(build_id)
        .bind(status)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Load `plugin_build_id` for a run (workers; not on `AgentRun`).
    pub async fn build_id_for_run(&self, run_id: Uuid) -> Result<Uuid, PluginBuildError> {
        sqlx::query_scalar("SELECT plugin_build_id FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_optional(self.pool)
            .await?
            .flatten()
            .ok_or(PluginBuildError::NotFound)
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

/// Oldest enabled implementer whose role contains `engineer`; prefer Backend Engineer.
pub fn builder_for_plugin(agents: &[Agent]) -> Option<&Agent> {
    let engineers: Vec<&Agent> = agents
        .iter()
        .filter(|agent| agent.enabled && is_implementer(&agent.role))
        .filter(|agent| agent.role.to_lowercase().contains("engineer"))
        .collect();
    engineers
        .iter()
        .copied()
        .filter(|agent| {
            agent.preset_source.as_deref() == Some("backend_engineer")
                || agent.role.to_lowercase().contains("backend")
        })
        .min_by_key(|agent| agent.created_at)
        .or_else(|| {
            engineers
                .iter()
                .copied()
                .min_by_key(|agent| agent.created_at)
        })
}

fn answers_ready(answers: &Value) -> bool {
    let Some(obj) = answers.as_object() else {
        return false;
    };
    // Any non-meta key with a non-empty string/array counts as saved answers.
    obj.iter().any(|(key, value)| {
        !key.starts_with('_')
            && match value {
                Value::String(s) => !s.trim().is_empty(),
                Value::Array(items) => !items.is_empty(),
                Value::Object(map) => !map.is_empty(),
                Value::Bool(_) | Value::Number(_) => true,
                Value::Null => false,
            }
    })
}

fn has_plan_revision(plans: &Value) -> bool {
    plans
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item.get("body").is_some()))
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

fn is_unique_violation(err: &sqlx::Error, index: &str) -> bool {
    match err {
        sqlx::Error::Database(db) => db
            .constraint()
            .or_else(|| db.message().contains(index).then_some(index))
            .is_some_and(|name| name == index || name.contains(index)),
        _ => false,
    }
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
        plan_questions: row.get("plan_questions"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

/// Context helpers used by the worker when writing `.agent/context.md`.
pub fn format_plan_questions(questions: &[PlanQuestion]) -> String {
    if questions.is_empty() {
        return String::new();
    }
    let mut out = String::from("## Open plan questions\n\n");
    for (i, q) in questions.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, q.question));
        if !q.choices.is_empty() {
            for choice in &q.choices {
                out.push_str(&format!("   - {choice}\n"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::worktree_service::plugin_build_dir;
    use time::OffsetDateTime;

    #[test]
    fn plugin_build_dir_nests_under_plugin_builds() {
        let root = PathBuf::from("/tmp/worktrees");
        let id = Uuid::from_u128(1);
        assert_eq!(
            plugin_build_dir(&root, id),
            root.join("plugin-builds").join(id.to_string())
        );
    }

    fn agent(id: u128, role: &str, key: &str, enabled: bool, created_secs: i64) -> Agent {
        Agent {
            id: Uuid::from_u128(id),
            name: role.into(),
            role: role.into(),
            responsibilities: vec![],
            system_prompt: String::new(),
            connector: "mock".into(),
            model_provider: None,
            model: None,
            thinking_level: None,
            enabled,
            preset_source: Some(key.into()),
            created_at: OffsetDateTime::from_unix_timestamp(created_secs).unwrap(),
            updated_at: OffsetDateTime::from_unix_timestamp(created_secs).unwrap(),
        }
    }

    #[test]
    fn builder_prefers_oldest_backend_engineer() {
        let fe = agent(1, "Frontend Engineer", "frontend_engineer", true, 1);
        let be_old = agent(2, "Backend Engineer", "backend_engineer", true, 5);
        let be_new = agent(3, "Backend Engineer", "backend_engineer", true, 10);
        let agents = [fe, be_new, be_old.clone()];
        let chosen = builder_for_plugin(&agents);
        assert_eq!(chosen.map(|a| a.id), Some(be_old.id));
    }

    #[test]
    fn builder_falls_back_to_any_engineer() {
        let fe = agent(1, "Frontend Engineer", "frontend_engineer", true, 1);
        assert_eq!(
            builder_for_plugin(&[fe.clone()]).map(|a| a.id),
            Some(fe.id)
        );
    }

    #[test]
    fn builder_none_without_engineer() {
        let pm = agent(1, "PM", "pm", true, 1);
        assert!(builder_for_plugin(&[pm]).is_none());
    }

    #[test]
    fn answers_ready_ignores_meta_keys() {
        assert!(!answers_ready(&json!({})));
        assert!(!answers_ready(&json!({ "_changeRequest": "x" })));
        assert!(answers_ready(&json!({ "purpose": "changelog" })));
    }
}
