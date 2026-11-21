use crate::domain::Id;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use sqlx::PgPool;
use super::{best_effort_persist_artifacts, JobCancelled};
use crate::domain::plugin_build::{
    is_plugin_build_job, BUILD_FORMAT_RULES, JOB_TYPE_PLAN_PLUGIN_BUILD,
};
use crate::domain::run::{AgentRun, RunStatus};
use crate::domain::slug::slugify;
use crate::events::publish_run_finished;
use crate::mcp::grant::grant_for_run;
use crate::mcp::token::NewRunToolScope;
use crate::mcp::tools::result::prefer_submitted_result;
use crate::providers::{connector_enforces_read_only, AgentRunInput, ProviderError};
use crate::services::agent_service::AgentService;
use crate::services::context_builder::{
    build_tool_first_context, write_context_document, ContextInput,
};
use crate::services::plugin_build_service::PluginBuildService;
use crate::services::plugin_service::PluginService;
use crate::services::result_contract;
use crate::services::run_service::RunService;
use crate::AppState;
use serde_json::Value;

/// Drive `plan_plugin_build` / `build_plugin` in the build's workspace_path.
pub(super) async fn execute_plugin_build(
    state: &Arc<AppState>,
    pool: &PgPool,
    run_svc: &RunService<'_>,
    run: &AgentRun,
) -> anyhow::Result<()> {
    if !is_plugin_build_job(&run.job_type) {
        anyhow::bail!("not a plugin build job");
    }
    let builds = PluginBuildService::new(pool, state.config.agent.worktrees_path.clone());
    let build_id = builds
        .build_id_for_run(run.id)
        .await
        .context("plugin build run has no plugin_build_id")?;
    let outcome = drive(state, pool, run_svc, run, &builds, build_id).await;
    let succeeded = outcome.is_ok();
    if let Err(err) = builds
        .on_run_terminal(build_id, &run.job_type, succeeded)
        .await
    {
        tracing::warn!(run_id = %run.id, %build_id, error = %err, "failed to sync plugin build status after run");
    }
    outcome
}

async fn drive(
    state: &Arc<AppState>,
    pool: &PgPool,
    run_svc: &RunService<'_>,
    run: &AgentRun,
    builds: &PluginBuildService<'_>,
    build_id: Id,
) -> anyhow::Result<()> {
    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    let build = builds.get(build_id).await.context("load plugin build")?;
    let agent = AgentService::new(pool)
        .get(run.agent_id)
        .await
        .context("load agent")?;
    let connector_name = agent.connector.as_str();
    let connector = state
        .connectors
        .registry_for(connector_name, crate::connectors_runtime::READINESS_WAIT)
        .await
        .get(connector_name)
        .with_context(|| {
            crate::connectors_runtime::connector_unavailable_message(connector_name)
        })?;
    let agent_key = agent
        .preset_source
        .clone()
        .unwrap_or_else(|| slugify(&agent.name));

    let cwd = Path::new(&build.workspace_path);
    if !cwd.is_dir() {
        anyhow::bail!("plugin build workspace missing: {}", build.workspace_path);
    }

    let planning = run.job_type == JOB_TYPE_PLAN_PLUGIN_BUILD;
    let context_input = ContextInput {
        ticket_title: if planning {
            "Plan plugin build"
        } else {
            "Build plugin"
        },
        ticket_description: "",
        ticket_status: &build.status,
        ticket_substatus: None,
        agent_name: &agent.name,
        agent_key: &agent_key,
        agent_role: &agent.role,
        agent_responsibilities: &agent.responsibilities,
        agent_system_prompt: &agent.system_prompt,
        repo_name: None,
        repo_remote_url: None,
        repo_default_branch: None,
        worktree_path: Some(&build.workspace_path),
        latest_comments: None,
        project_rules: None,
        resume_context: None,
        context_profile: run.context_profile,
        human_request: None,
        ticket_id: None,
        assignee_agent_key: None,
        thread_excerpt: Some(""),
        has_attachments: false,
        planning_run: planning,
    };
    let mut markdown = build_tool_first_context(&context_input, &[], None);
    markdown.push_str(&format!(
        "\n# Task\n\n**Job:** {}\n**Build ID:** `{}`\n**Workspace:** `{}`\n\n",
        run.job_type, build.id, build.workspace_path
    ));
    markdown.push_str("## Answers\n\n```json\n");
    markdown.push_str(&serde_json::to_string_pretty(&build.answers).unwrap_or_default());
    markdown.push_str("\n```\n\n");
    if let Some(latest) = latest_plan_body(&build.plans) {
        markdown.push_str("## Latest plan\n\n");
        markdown.push_str(latest);
        markdown.push_str("\n\n");
    }
    if let Some(change) = build
        .answers
        .get("_changeRequest")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        markdown.push_str("## Change request\n\n");
        markdown.push_str(change);
        markdown.push_str("\n\n");
    }
    if planning {
        markdown.push_str(
            "\n# Plan\n\n\
             Write a plain-language plan for this plugin (name, what it will do, what it needs, how you'll know it works, where it goes).\n\
             Put the plan in `summary`. Optionally include structured Advanced data in `advanced` \
             (pluginName, version, description, files, skills, servers, settings, permissions, manifestPreview).\n\
             If you need clarifying input first, return `planQuestions` (1–3) and leave summary empty.\n\
             Finish with `result_submit` only — no other Coppice tools are available.\n",
        );
    } else {
        markdown.push('\n');
        markdown.push_str(BUILD_FORMAT_RULES);
        if let Some(errors) = build.check_errors.as_ref().and_then(|v| v.as_array()) {
            if !errors.is_empty() {
                markdown.push_str(
                    "\n## Format check errors (fix these before finishing)\n\n",
                );
                for err in errors {
                    if let Some(line) = err.as_str() {
                        markdown.push_str(&format!("- {line}\n"));
                    }
                }
                markdown.push('\n');
            }
        }
        markdown.push_str(
            "\nWrite the plugin files only under this workspace. Finish with `result_submit` \
             (`done` + non-empty summary). No other Coppice tools are available.\n",
        );
    }
    write_context_document(cwd, &markdown).context("write plugin build context")?;
    let context_path = cwd.join(".agent").join("context.md");

    let stream = state.run_streams.register(run.id);
    crate::services::run_service::spawn_reported_model_writer(pool.clone(), run.id, &stream);
    crate::services::usage_service::spawn_usage_writer(pool.clone(), run.id, &stream);
    let cancel_rx = stream.cancelled_rx();
    run_svc
        .mark_running(run.id)
        .await
        .context("mark plugin build run running")?;

    // No installed plugins for builder scopes (`plugin_ids` empty).
    let grant = grant_for_run(
        state,
        pool,
        NewRunToolScope {
            run_id: run.id,
            agent_id: run.agent_id,
            ticket_id: None,
            chat_session_id: None,
            board_id: None,
            profile: run.context_profile,
            job_type: run.job_type.clone(),
            compaction_ticket_ids: Vec::new(),
            plugin_ids: Vec::new(),
        },
    )
    .await
    .context("mint plugin build tool token")?;

    tracing::info!(
        run_id = %run.id,
        %build_id,
        job_type = %run.job_type,
        connector = connector_name,
        "plugin build run started"
    );

    let provider_result = connector
        .run(AgentRunInput {
            agent_id: run.agent_id.to_string(),
            agent_key: agent_key.clone(),
            agent_role: agent.role.clone(),
            job_type: run.job_type.clone(),
            ticket_id: None,
            ticket_status: None,
            context_profile: run.context_profile,
            context_path: context_path.to_string_lossy().into_owned(),
            run_id: Some(run.id.to_string()),
            chat_session_id: None,
            artifacts_dir: Some(state.config.storage.artifacts_dir.clone()),
            stream: Some(stream.clone()),
            cancel_rx: Some(cancel_rx),
            model_provider: agent.model_provider.clone(),
            model: agent.model.clone(),
            thinking_level: agent.thinking_level,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
            read_only_tools: connector_enforces_read_only(connector_name),
            mcp: Some(grant.access.clone()),
        })
        .await;
    crate::services::run_service::record_reported_model(pool, run.id, &stream).await;
    crate::services::usage_service::record_usage(pool, run.id, &stream).await;
    grant.revoke().await;
    let provider_result = prefer_submitted_result(pool, run.id, provider_result).await;

    let result = match provider_result {
        Ok(result) => result,
        Err(ProviderError::Cancelled) => {
            best_effort_persist_artifacts(
                state,
                &stream,
                run.id,
                connector_name,
                None,
                "after cancel",
            );
            state.run_streams.remove(run.id);
            return Err(JobCancelled.into());
        }
        Err(err) => {
            best_effort_persist_artifacts(
                state,
                &stream,
                run.id,
                connector_name,
                None,
                "after provider error",
            );
            state.run_streams.remove(run.id);
            return Err(anyhow::Error::new(err).context(format!(
                "connector `{connector_name}` plugin build run failed"
            )));
        }
    };
    best_effort_persist_artifacts(
        state,
        &stream,
        run.id,
        connector_name,
        None,
        "after plugin build",
    );
    state.run_streams.remove(run.id);
    if run_svc.is_cancelled(run.id).await? {
        return Err(JobCancelled.into());
    }

    result_contract::validate_for_profile(&result, run.context_profile, &run.job_type)
        .map_err(|err| anyhow::anyhow!(err))?;

    if planning {
        builds
            .apply_plan_result(build_id, &result)
            .await
            .context("apply plugin plan result")?;
    } else {
        builds
            .apply_build_result(build_id, &result)
            .await
            .context("apply plugin build result")?;
    }

    let finished = run_svc
        .finish_run(
            run.id,
            RunStatus::Succeeded,
            Some(build.workspace_path.clone()),
            None,
        )
        .await
        .context("finish plugin build run")?;

    publish_run_finished(
        state,
        pool,
        finished.id,
        finished.ticket_id,
        finished.agent_id,
        finished.status,
        finished.error_message,
    )
    .await;

    // Format check / install / auto-test after the active-run index is free.
    if !planning {
        let plugins = PluginService::new(pool);
        let ready = |connector: &str| {
            if connector == coppice_connectors::MOCK {
                return true;
            }
            matches!(
                state
                    .connector_probes
                    .get(connector)
                    .map(|cached| cached.readiness),
                Some(coppice_connectors::sign_in::Readiness::Ready)
            )
        };
        if let Err(err) = builds
            .finalize_after_build(
                build_id,
                &plugins,
                &state.config.plugins.dir,
                Path::new(&state.config.plugins.marketplaces_dir),
                &state.secret_store,
                &state.plugin_mcp,
                &state.skills,
                ready,
            )
            .await
        {
            tracing::warn!(
                run_id = %run.id,
                %build_id,
                error = %err,
                "plugin build finalize (check/install/test) failed"
            );
        }
    }

    tracing::info!(
        run_id = %run.id,
        %build_id,
        job_type = %run.job_type,
        "plugin build run finished"
    );
    Ok(())
}

fn latest_plan_body(plans: &Value) -> Option<&str> {
    plans
        .as_array()?
        .last()?
        .get("body")?
        .as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn latest_plan_body_reads_last_revision() {
        let plans = json!([
            {"revision": 1, "body": "first"},
            {"revision": 2, "body": "second"}
        ]);
        assert_eq!(latest_plan_body(&plans), Some("second"));
    }
}
