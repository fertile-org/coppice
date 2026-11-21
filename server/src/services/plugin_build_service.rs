use crate::crypto::SecretStore;
use crate::domain::agent::Agent;
use crate::domain::plugin_build::{
    JOB_TYPE_BUILD_PLUGIN, JOB_TYPE_PLAN_PLUGIN_BUILD, NO_AGENT_READY_TO_BUILD,
};
use crate::domain::run::AgentRun;
use crate::mcp::proxy::McpServerPool;
use crate::plugins::format_check::check_plugin;
use crate::plugins::manifest::parse_plugin;
use crate::plugins::skills::SkillCatalog;
use crate::providers::{AgentRunResult, PlanQuestion};
use crate::services::agent_service::AgentService;
use crate::services::planning_service::planner_for_ticket;
use crate::services::plugin_service::{
    PluginError, PluginService, ServerTestOutcome, ServerTestResult,
};
use crate::services::run_service::{RunError, RunService};
use crate::services::workflow_service::is_implementer;
use crate::services::worktree_service::{
    init_empty_git_workspace, plugin_build_dir, WorktreeError,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

const COLUMNS: &str = "id, answers, plans, status, plugin_name, workspace_path, publish_target, plan_questions, check_errors, format_retries, plugin_id, test_result, created_at, updated_at";

const MAX_FORMAT_RETRIES: i32 = 2;

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
    #[error(transparent)]
    Plugin(#[from] PluginError),
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
    /// Plain-language format-check errors (JSON string array), when the last check failed.
    pub check_errors: Option<Value>,
    pub format_retries: i32,
    pub plugin_id: Option<Uuid>,
    /// Auto-test payload (`{ "servers": […] }`), set when install+test finishes.
    pub test_result: Option<Value>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    /// Active plan/build run for this build (`queued` / `running`), if any.
    pub active_run_id: Option<Uuid>,
    /// Most recent run for this build (any status).
    pub last_run_id: Option<Uuid>,
    pub last_run_status: Option<String>,
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

    /// Attach active/latest agent-run ids for Stop + Installed-row reopen.
    pub async fn with_run_meta(
        &self,
        mut build: PluginBuild,
    ) -> Result<PluginBuild, PluginBuildError> {
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM agent_runs
             WHERE plugin_build_id = $1 AND status IN ('queued', 'running')
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(build.id)
        .fetch_optional(self.pool)
        .await?;
        let last_row = sqlx::query(
            "SELECT id, status FROM agent_runs
             WHERE plugin_build_id = $1
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(build.id)
        .fetch_optional(self.pool)
        .await?;
        build.active_run_id = active;
        if let Some(row) = last_row {
            build.last_run_id = Some(row.get("id"));
            build.last_run_status = Some(row.get("status"));
        }
        Ok(build)
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

    /// Discard and, when an installed copy exists, remove it and rescan.
    pub async fn discard_and_uninstall(
        &self,
        id: Uuid,
        plugins: &PluginService<'_>,
        plugins_dir: &str,
        plugin_mcp: &McpServerPool,
        skills: &SkillCatalog,
    ) -> Result<PluginBuild, PluginBuildError> {
        let build = self.get(id).await?;
        if is_terminal(&build.status) {
            return Err(PluginBuildError::Conflict(format!(
                "build is already {}",
                build.status
            )));
        }
        if let Some(plugin_id) = build.plugin_id {
            plugin_mcp.stop_plugin(plugin_id).await;
            skills.remove_plugin(plugin_id).await;
            if let Ok(path) = plugins.plugin_path(plugin_id).await {
                remove_workspace(&path).await?;
            } else if let Some(name) = build.plugin_name.as_deref() {
                remove_workspace(&Path::new(plugins_dir).join(name)).await?;
            }
        } else if let Some(name) = build.plugin_name.as_deref() {
            remove_workspace(&Path::new(plugins_dir).join(name)).await?;
        }
        remove_workspace(Path::new(&build.workspace_path)).await?;
        let row = sqlx::query(&format!(
            "UPDATE plugin_builds
             SET status = 'discarded', plugin_id = NULL, test_result = NULL, updated_at = now()
             WHERE id = $1
             RETURNING {COLUMNS}"
        ))
        .bind(id)
        .fetch_one(self.pool)
        .await?;
        if let Err(err) = plugins.rescan().await {
            tracing::warn!(error = %err, %id, "plugin rescan after builder discard");
        }
        if let Err(err) = plugins.refresh_catalog(skills).await {
            tracing::warn!(error = %err, %id, "skill catalog refresh after builder discard");
        }
        Ok(row_to_build(row))
    }

    /// Delete the workspace after Accept (PB-11). Keeps the row; status → accepted.
    pub async fn delete_workspace_on_accept(
        &self,
        id: Uuid,
    ) -> Result<PluginBuild, PluginBuildError> {
        let build = self.get(id).await?;
        if build.status != "ready_to_try" {
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

    /// Accept: delete workspace, optionally enable the installed plugin.
    /// Publish (if requested) runs in the API layer after this returns.
    pub async fn accept(
        &self,
        id: Uuid,
        enable: bool,
        plugins: &PluginService<'_>,
        plugin_mcp: &McpServerPool,
        skills: &SkillCatalog,
    ) -> Result<PluginBuild, PluginBuildError> {
        let build = self.delete_workspace_on_accept(id).await?;
        if enable {
            let Some(plugin_id) = build.plugin_id else {
                return Err(PluginBuildError::Validation(
                    "build has no installed plugin to enable".into(),
                ));
            };
            plugins.set_enabled(plugin_id, true).await?;
            if let Err(err) = plugins.refresh_catalog(skills).await {
                tracing::warn!(error = %err, %id, "skill catalog refresh after builder accept");
            }
            // Enabling starts MCP on demand elsewhere; keep pool consistent.
            let _ = plugin_mcp;
        }
        self.get(id).await
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
        let check_failed = has_check_errors(&build);
        // Ask-for-changes allowed from ready_to_try and check-failed building.
        if matches!(build.status.as_str(), "accepted")
            || (build.status == "building" && !check_failed)
        {
            return Err(PluginBuildError::Conflict(format!(
                "cannot plan while status is {}",
                build.status
            )));
        }
        let from_try = build.status == "ready_to_try";

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
        // Re-enter planning; keep plugin_id so the next finalize can replace the install.
        sqlx::query(
            "UPDATE plugin_builds
             SET answers = $2, status = 'planning', plan_questions = NULL,
                 check_errors = NULL, format_retries = 0,
                 test_result = CASE WHEN $3 THEN NULL ELSE test_result END,
                 updated_at = now()
             WHERE id = $1",
        )
        .bind(id)
        .bind(&build.answers)
        .bind(from_try)
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

    /// Queue a `build_plugin` run from `plan_ready`, or Try again from `building`
    /// after format-check failure (`check_errors` set).
    ///
    /// Stopping the agent run must not call build `cancel`/`discard`. On run
    /// cancel/fail the worker reverts status to `plan_ready` so Try again works.
    pub async fn queue_build_run(
        &self,
        id: Uuid,
        connector_ready: impl Fn(&str) -> bool,
    ) -> Result<(PluginBuild, AgentRun), PluginBuildError> {
        let build = self.get(id).await?;
        let retry_after_check = build.status == "building" && has_check_errors(&build);
        if build.status != "plan_ready" && !retry_after_check {
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
            "UPDATE plugin_builds
             SET status = 'building', check_errors = NULL, format_retries = 0, updated_at = now()
             WHERE id = $1",
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

    /// Build agent success: validate the result. Status stays `building` until
    /// [`Self::finalize_after_build`] runs the checker / install / auto-test.
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
        self.get(build_id).await
    }

    /// After a successful build run finishes (active-run index free): format-check
    /// the workspace, optionally queue a retry, or install disabled + auto-test.
    #[allow(clippy::too_many_arguments)] // install path needs plugins + secret + MCP + skills + readiness
    pub async fn finalize_after_build(
        &self,
        build_id: Uuid,
        plugins: &PluginService<'_>,
        plugins_dir: &str,
        marketplaces_dir: &Path,
        secret_store: &SecretStore,
        plugin_mcp: &McpServerPool,
        skills: &SkillCatalog,
        connector_ready: impl Fn(&str) -> bool,
    ) -> Result<PluginBuild, PluginBuildError> {
        let build = self.get(build_id).await?;
        if build.status != "building" {
            return Ok(build);
        }

        // Missing rows are gone from disk; reusing their name must not fail the check
        // (Compose smoke re-runs leave Accept'ed plugins as status=missing after rm+rescan).
        let installed: HashSet<String> = plugins
            .list_plugins()
            .await?
            .into_iter()
            .filter(|p| p.status != "missing" && build.plugin_id != Some(p.id))
            .map(|p| p.name)
            .collect();

        let errors = check_plugin(Path::new(&build.workspace_path), &installed);
        if !errors.is_empty() {
            return self
                .on_format_check_failed(build_id, &errors, connector_ready)
                .await;
        }

        self.install_disabled_and_test(
            build_id,
            plugins,
            plugins_dir,
            marketplaces_dir,
            secret_store,
            plugin_mcp,
            skills,
        )
        .await
    }

    async fn on_format_check_failed(
        &self,
        build_id: Uuid,
        errors: &[String],
        connector_ready: impl Fn(&str) -> bool,
    ) -> Result<PluginBuild, PluginBuildError> {
        let build = self.get(build_id).await?;
        let check_errors = Value::Array(errors.iter().cloned().map(Value::String).collect());
        let should_retry = build.format_retries < MAX_FORMAT_RETRIES;
        let next_retries = if should_retry {
            build.format_retries + 1
        } else {
            build.format_retries
        };
        sqlx::query(
            "UPDATE plugin_builds
             SET check_errors = $2, format_retries = $3, updated_at = now()
             WHERE id = $1",
        )
        .bind(build_id)
        .bind(&check_errors)
        .bind(next_retries)
        .execute(self.pool)
        .await?;

        if should_retry {
            // Automatic retry after current run finished (unique active-run index free).
            self.queue_format_retry(build_id, connector_ready).await?;
        }
        self.get(build_id).await
    }

    /// Queue a follow-up build run while keeping `check_errors` / `format_retries`.
    async fn queue_format_retry(
        &self,
        id: Uuid,
        connector_ready: impl Fn(&str) -> bool,
    ) -> Result<AgentRun, PluginBuildError> {
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
        Ok(run)
    }

    #[allow(clippy::too_many_arguments)] // same install/test deps as finalize_after_build
    async fn install_disabled_and_test(
        &self,
        build_id: Uuid,
        plugins: &PluginService<'_>,
        plugins_dir: &str,
        marketplaces_dir: &Path,
        secret_store: &SecretStore,
        plugin_mcp: &McpServerPool,
        skills: &SkillCatalog,
    ) -> Result<PluginBuild, PluginBuildError> {
        let build = self.get(build_id).await?;
        let workspace = Path::new(&build.workspace_path);
        let manifest = parse_plugin(workspace).map_err(PluginBuildError::Validation)?;
        let plugin_name = manifest.name.clone();

        plugins
            .ensure_default_dir(plugins_dir, marketplaces_dir)
            .await?;
        let dest = Path::new(plugins_dir).join(&plugin_name);

        if dest.exists() {
            let ok_to_replace = match build.plugin_id {
                Some(id) => plugins
                    .get_plugin(id)
                    .await
                    .ok()
                    .is_some_and(|row| row.name == plugin_name),
                None => false,
            };
            if !ok_to_replace {
                // Orphan folder from a prior attempt for this name, or clash.
                if plugins.list_plugins().await?.iter().any(|p| {
                    p.name == plugin_name
                        && p.status != "missing"
                        && build.plugin_id != Some(p.id)
                }) {
                    return self
                        .store_install_failure(
                            build_id,
                            &format!(
                                "Coppice couldn't load it: a plugin called {plugin_name} already exists."
                            ),
                        )
                        .await;
                }
            }
            if dest.is_dir() {
                tokio::fs::remove_dir_all(&dest).await?;
            } else {
                tokio::fs::remove_file(&dest).await?;
            }
        }

        copy_plugin_tree(workspace, &dest)?;

        let rows = plugins.rescan().await?;
        if let Err(err) = plugins.refresh_catalog(skills).await {
            tracing::warn!(error = %err, %build_id, "plugin skill catalog refresh after builder install");
        }

        let Some(row) = rows.into_iter().find(|p| p.name == plugin_name) else {
            return self
                .store_install_failure(
                    build_id,
                    "Coppice couldn't load it: rescan did not find the installed plugin.",
                )
                .await;
        };
        if row.status != "ok" {
            return self
                .store_install_failure(
                    build_id,
                    &format!(
                        "Coppice couldn't load it: {}.",
                        row.error.as_deref().unwrap_or(&row.status)
                    ),
                )
                .await;
        }
        if row.enabled {
            return self
                .store_install_failure(
                    build_id,
                    "Coppice couldn't load it: plugin was enabled unexpectedly.",
                )
                .await;
        }

        let test_results = plugins
            .test_servers(row.id, secret_store, plugin_mcp)
            .await?;
        if !row.enabled {
            plugin_mcp.stop_plugin(row.id).await;
        }
        let test_result = json!({ "servers": test_results_json(&test_results) });

        let updated = sqlx::query(&format!(
            "UPDATE plugin_builds
             SET check_errors = NULL, plugin_name = $2, plugin_id = $3, test_result = $4,
                 status = 'ready_to_try', updated_at = now()
             WHERE id = $1
             RETURNING {COLUMNS}"
        ))
        .bind(build_id)
        .bind(&plugin_name)
        .bind(row.id)
        .bind(&test_result)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_build(updated))
    }

    async fn store_install_failure(
        &self,
        build_id: Uuid,
        reason: &str,
    ) -> Result<PluginBuild, PluginBuildError> {
        let check_errors = json!([reason]);
        let row = sqlx::query(&format!(
            "UPDATE plugin_builds
             SET check_errors = $2, updated_at = now()
             WHERE id = $1
             RETURNING {COLUMNS}"
        ))
        .bind(build_id)
        .bind(&check_errors)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_build(row))
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
        check_errors: row.get("check_errors"),
        format_retries: row.get("format_retries"),
        plugin_id: row.get("plugin_id"),
        test_result: row.get("test_result"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        active_run_id: None,
        last_run_id: None,
        last_run_status: None,
    }
}

fn has_check_errors(build: &PluginBuild) -> bool {
    match &build.check_errors {
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::String(s)) => !s.is_empty(),
        Some(other) => !other.is_null(),
        None => false,
    }
}

fn test_results_json(results: &[ServerTestResult]) -> Vec<Value> {
    results
        .iter()
        .map(|r| {
            let (status, error, tools) = match &r.outcome {
                ServerTestOutcome::Ok(tools) => (
                    "ok",
                    None,
                    tools
                        .iter()
                        .map(|t| {
                            json!({
                                "name": t.name,
                                "exposedName": t.exposed_name,
                                "description": t.description,
                                "readOnly": t.read_only,
                            })
                        })
                        .collect::<Vec<_>>(),
                ),
                ServerTestOutcome::Error(error) => ("error", Some(error.clone()), Vec::new()),
                ServerTestOutcome::Unsupported { error } => {
                    ("unsupported", error.clone(), Vec::new())
                }
            };
            json!({
                "name": r.name,
                "kind": r.kind,
                "status": status,
                "error": error,
                "tools": tools,
            })
        })
        .collect()
}

/// Recursive copy that skips `.git` / `.agent` and does not follow symlinks.
fn copy_plugin_tree(src: &Path, dest: &Path) -> Result<(), PluginBuildError> {
    std::fs::create_dir_all(dest)?;
    for entry in walkdir::WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != ".git" && name != ".agent"
        })
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(src) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(rel);
        if entry.file_type().is_symlink() {
            // Skip outbound / any symlinks — checker already rejects escapes.
            continue;
        }
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, &target)?;
        }
    }
    Ok(())
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
