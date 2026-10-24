use super::models::{ModelInfo, ModelListCache};
use super::{
    mcp_unavailable, run_dir, worktree_dir_from_context, AgentProvider, AgentRunInput,
    AgentRunResult, ProviderError,
};
use crate::services::ticket_attachments;
use crate::sessions::opencode_client::OpenCodeClient;
use crate::sessions::opencode_events::coppice_run_prompt;
use crate::sessions::opencode_run_server::{opencode_run_config, OpenCodeRunServers};
use async_trait::async_trait;
use coppice_config::OpenCodeProviderConfig;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct OpenCodeProvider {
    servers: Arc<OpenCodeRunServers>,
    config: OpenCodeProviderConfig,
    /// Models `opencode acp` offers, keyed by command.
    acp_models: ModelListCache,
    acp_models_gate: tokio::sync::Mutex<()>,
}

impl OpenCodeProvider {
    pub fn new(servers: Arc<OpenCodeRunServers>, config: OpenCodeProviderConfig) -> Self {
        Self {
            servers,
            config,
            acp_models: ModelListCache::new(),
            acp_models_gate: tokio::sync::Mutex::new(()),
        }
    }

    /// The `provider/model` to run with `opencode run`, when ACP does not
    /// offer it. `None` keeps the run on ACP, including when the list cannot
    /// be read: ACP then reports the model problem itself.
    async fn model_outside_acp(&self, input: &AgentRunInput) -> Option<String> {
        let model = super::opencode_acp::model_value(
            input.model_provider.as_deref(),
            input.model.as_deref(),
        )?;
        let command = &self.config.command;
        let offered = {
            let _gate = self.acp_models_gate.lock().await;
            match self.acp_models.get(command, false) {
                Some(hit) => hit,
                None => match super::opencode_acp::offered_models(command).await {
                    Ok(ids) if !ids.is_empty() => {
                        let models: Vec<ModelInfo> = ids
                            .into_iter()
                            .map(|id| ModelInfo {
                                name: id.clone(),
                                id,
                            })
                            .collect();
                        self.acp_models.store(command, models.clone());
                        models
                    }
                    Ok(_) => return None,
                    Err(err) => {
                        tracing::warn!(error = %err, "could not list the models opencode acp offers");
                        return None;
                    }
                },
            }
        };
        (!offered.iter().any(|m| m.id == model)).then_some(model)
    }
}

#[async_trait]
impl AgentProvider for OpenCodeProvider {
    fn id(&self) -> &str {
        "opencode"
    }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        // Runs without a gateway token (drafts, probes) have no run dir; their
        // config lives in a temp dir held until the run ends.
        let (config_path, _config_dir) = match &input.mcp {
            Some(_) => (run_config_path(&input)?, None),
            None => {
                let dir = tempfile::tempdir()?;
                (dir.path().join("opencode.json"), Some(dir))
            }
        };
        let worktree = worktree_dir_from_context(&input.context_path)?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let staged = ticket_attachments::load_for_run(
            input.artifacts_dir.as_deref(),
            input.ticket_id.as_deref(),
        );
        let version = super::cli_version::detect(&self.config.command)
            .await
            .map_err(|err| {
                ProviderError::InvalidFixture(super::cli_version::version_unavailable(
                    &self.config.command,
                    &err,
                ))
            })?;
        let acp = super::opencode_acp::uses_acp(&version);
        let cli_model = if acp {
            self.model_outside_acp(&input).await
        } else {
            None
        };
        // Over ACP the gateway is passed in session/new, not this file.
        let gateway_in_config = !acp || cli_model.is_some();
        std::fs::write(
            &config_path,
            ticket_attachments::with_attachment_read(
                &opencode_run_config(
                    if gateway_in_config {
                        input.mcp.as_ref()
                    } else {
                        None
                    },
                    input.read_only_tools,
                ),
                ticket_attachments::external_read_dir(&staged).as_deref(),
            ),
        )?;
        let run_timeout = Duration::from_secs(self.config.run_timeout_secs);
        if let Some(model) = cli_model {
            let prompt = ticket_attachments::launch_prompt(coppice_run_prompt(), &staged);
            return super::opencode_cli::run(
                input,
                super::opencode_cli::RunSpec {
                    program: &self.config.command,
                    version: &version,
                    config_path: &config_path,
                    worktree: &worktree,
                    prompt: &prompt,
                    run_timeout,
                    model: &model,
                },
            )
            .await;
        }
        if acp {
            let prompt = ticket_attachments::launch_prompt(coppice_run_prompt(), &staged);
            return super::opencode_acp::run(
                input,
                super::opencode_acp::RunSpec {
                    program: &self.config.command,
                    config_path: &config_path,
                    worktree: &worktree,
                    prompt: &prompt,
                    run_timeout,
                },
            )
            .await;
        }

        let key = input
            .run_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let mut env = input
            .mcp
            .as_ref()
            .map(|access| access.env().to_vec())
            .unwrap_or_default();
        if input.read_only_tools {
            // Without this, a checkout's opencode.json that allows edit and
            // bash overrides the per-run deny. Verified on OpenCode 1.19.0.
            env.push(("OPENCODE_DISABLE_PROJECT_CONFIG", "1".into()));
        }
        let lease = self
            .servers
            .start(&key, &config_path, env, input.ticket_id.as_deref())
            .await
            .map_err(|err| ProviderError::Io(std::io::Error::other(format!("{err:#}"))))?;

        let client = OpenCodeClient::with_run_timeout(lease.base_url(), run_timeout);
        let result = client
            .run_session(
                &worktree,
                input.model_provider.as_deref(),
                input.model.as_deref(),
                input.resume_session_id.as_deref(),
                &ticket_attachments::launch_prompt(coppice_run_prompt(), &staged),
                input.stream,
                input.cancel_rx,
                input.session_created_tx,
            )
            .await;
        lease.stop().await;
        result
    }
}

fn run_config_path(input: &AgentRunInput) -> Result<PathBuf, ProviderError> {
    let dir = run_dir(input, "opencode").map_err(|err| match err {
        ProviderError::InvalidInput(_) => {
            mcp_unavailable("opencode has no run artifacts dir for its per-run config")
        }
        other => other,
    })?;
    Ok(dir.join("opencode.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::context_profile::ContextProfile;
    use crate::mcp::grant::McpAccess;

    fn tool_first_input(
        worktree: &std::path::Path,
        artifacts: Option<&std::path::Path>,
    ) -> AgentRunInput {
        AgentRunInput {
            agent_id: "agent-1".into(),
            agent_key: "agent-1".into(),
            agent_role: "Backend Engineer".into(),
            job_type: "work_on_ticket".into(),
            ticket_id: None,
            ticket_status: None,
            context_profile: ContextProfile::Full,
            context_path: worktree
                .join(".coppice")
                .join("context.md")
                .display()
                .to_string(),
            run_id: Some("run-1".into()),
            chat_session_id: None,
            artifacts_dir: artifacts.map(|p| p.display().to_string()),
            stream: None,
            cancel_rx: None,
            model_provider: None,
            model: None,
            session_created_tx: None,
            resume_context: None,
            resume_session_id: None,
            read_only_tools: false,
            mcp: Some(McpAccess {
                url: "http://127.0.0.1:5000/mcp".into(),
                token: "super-secret-run-token".into(),
            }),
        }
    }

    #[test]
    fn opencode_config_path_is_under_run_dir_not_worktree() {
        let worktree = tempfile::tempdir().expect("worktree");
        let artifacts = tempfile::tempdir().expect("artifacts");
        let input = tool_first_input(worktree.path(), Some(artifacts.path()));

        let path = run_config_path(&input).expect("config path");
        assert_eq!(
            path,
            run_dir(&input, "opencode").unwrap().join("opencode.json")
        );

        let worktree_dir = std::path::Path::new(&input.context_path)
            .parent()
            .and_then(|p| p.parent())
            .unwrap();
        assert!(!path.starts_with(worktree_dir), "{path:?}");
    }

    #[tokio::test]
    async fn opencode_tool_first_run_without_run_dir_is_mcp_unavailable() {
        let worktree = tempfile::tempdir().expect("worktree");
        let input = tool_first_input(worktree.path(), None);
        let provider = OpenCodeProvider::new(
            OpenCodeRunServers::new("does-not-exist-opencode".into(), "127.0.0.1".into()),
            OpenCodeProviderConfig::default(),
        );

        let err = provider.run(input).await.expect_err("no run dir");
        assert!(
            err.to_string()
                .starts_with("invalid input: mcp_unavailable:"),
            "{err}"
        );
    }
}
