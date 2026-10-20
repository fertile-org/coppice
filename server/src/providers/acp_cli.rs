//! Connectors that are an ACP agent CLI and nothing else: GitHub Copilot CLI,
//! Qwen Code, Kimi Code, and Grok. The run contract gives their flags; this file
//! gives the few things that differ between them.

use super::acp::{self, AcpLaunch, AcpLimits, AcpTurn, NoObserver, ToolPolicy};
use super::acp_connector::{self, AcpRun};
use super::models::{ModelCatalog, ModelInfo, ModelListCache};
use super::{worktree_dir_from_context, AgentProvider, AgentRunInput, AgentRunResult, ProviderError};
use crate::services::ticket_attachments;
use crate::sessions::opencode_events::coppice_run_prompt;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What differs between the ACP CLI connectors.
#[derive(Debug, Clone, Copy)]
pub struct AcpCliSpec {
    pub id: &'static str,
    /// Live console events are `<prefix>.console.*`.
    pub console_prefix: &'static str,
    pub sign_in_hint: &'static str,
    /// Session `mode` option to set, for a CLI with no permission flags.
    pub session_mode: Option<&'static str>,
    /// Whether the CLI takes `--add-dir` for staged ticket attachments.
    pub add_dir: bool,
    /// Environment for read-only runs.
    pub read_only_env: &'static [(&'static str, &'static str)],
    /// Read-only holds only outside temp folders, so a read-only run in one is
    /// refused rather than started unprotected.
    pub read_only_needs_non_temp_dir: bool,
}

pub const COPILOT: AcpCliSpec = AcpCliSpec {
    id: coppice_connectors::COPILOT,
    console_prefix: "copilot",
    sign_in_hint: "Run `copilot login`, then try again.",
    session_mode: None,
    add_dir: true,
    read_only_env: &[],
    read_only_needs_non_temp_dir: false,
};

pub const QWEN_CODE: AcpCliSpec = AcpCliSpec {
    id: coppice_connectors::QWEN_CODE,
    console_prefix: "qwen",
    sign_in_hint: "Run `qwen` and sign in, then try again.",
    session_mode: None,
    add_dir: true,
    read_only_env: &[],
    read_only_needs_non_temp_dir: false,
};

/// `kimi acp` has no permission flags; yolo keeps a headless run from waiting
/// on approvals, and read-only runs refuse writes and commands themselves.
/// It takes no `--add-dir` either (only `--login` and `--region`).
pub const KIMI: AcpCliSpec = AcpCliSpec {
    id: coppice_connectors::KIMI,
    console_prefix: "kimi",
    sign_in_hint: "Run `kimi login`, then try again.",
    session_mode: Some("yolo"),
    add_dir: false,
    read_only_env: &[],
    read_only_needs_non_temp_dir: false,
};

/// `grok agent stdio` takes no permission flags beyond `--always-approve`.
/// Read-only runs use its kernel sandbox, which still lets the agent write to
/// temp folders, and Coppice refuses the shell commands Grok routes to it.
/// It takes no `--add-dir`; the sandbox lets it read staged files anyway.
pub const GROK: AcpCliSpec = AcpCliSpec {
    id: coppice_connectors::GROK,
    console_prefix: "grok",
    sign_in_hint: "Run `grok login`, then try again.",
    session_mode: None,
    add_dir: false,
    read_only_env: &[("GROK_SANDBOX", "read-only")],
    read_only_needs_non_temp_dir: true,
};

fn descriptor(spec: &AcpCliSpec) -> &'static coppice_connectors::ConnectorDescriptor {
    coppice_connectors::get(spec.id).expect("ACP CLI connector has a descriptor")
}

/// Folders a sandbox that keeps the agent read-only still lets it write to:
/// `/tmp`, `/var/tmp`, and the OS temp folder (`$TMPDIR` on macOS).
fn temp_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = vec!["/tmp".into(), "/var/tmp".into(), std::env::temp_dir()];
    for root in roots.clone() {
        if let Ok(real) = std::fs::canonicalize(&root) {
            roots.push(real);
        }
    }
    roots
}

pub fn in_temp_dir(dir: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| dir.starts_with(root))
}

/// The CLI's arguments for a run: the pinned flags and the permission flags
/// for its policy.
pub fn launch_args(spec: &AcpCliSpec, read_only: bool, version: Option<&str>) -> Vec<String> {
    descriptor(spec).run_contract.argv(&coppice_connectors::LaunchSubst {
        read_only,
        plan: false,
        worktree: "",
        hostname: "",
        port: "",
        version,
    })
}

fn env_for(spec: &AcpCliSpec, read_only: bool) -> Vec<(String, String)> {
    if !read_only {
        return Vec::new();
    }
    spec.read_only_env
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

pub struct AcpCliProvider {
    spec: AcpCliSpec,
    command: String,
    run_timeout: Duration,
}

impl AcpCliProvider {
    pub fn new(spec: AcpCliSpec, command: String, run_timeout_secs: u64) -> Self {
        Self {
            spec,
            command,
            run_timeout: Duration::from_secs(run_timeout_secs),
        }
    }
}

#[async_trait]
impl AgentProvider for AcpCliProvider {
    fn id(&self) -> &str {
        self.spec.id
    }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        let worktree = worktree_dir_from_context(&input.context_path)?;
        // Planning is always read-only.
        let read_only = input.read_only_tools
            || crate::domain::workflow::is_read_only_job(&input.job_type);
        if read_only
            && self.spec.read_only_needs_non_temp_dir
            && in_temp_dir(&worktree, &temp_roots())
        {
            return Err(ProviderError::InvalidInput(format!(
                "{} can't be kept read-only in a temp folder ({}). Move Coppice's worktrees folder out of the temp folder, then try again.",
                descriptor(&self.spec).display_name,
                worktree.display()
            )));
        }
        let version = super::cli_version::detect(&self.command).await.ok();
        let staged = ticket_attachments::load_for_run(
            input.artifacts_dir.as_deref(),
            input.ticket_id.as_deref(),
        );
        let mut args = launch_args(&self.spec, read_only, version.as_deref());
        if self.spec.add_dir {
            args.extend(ticket_attachments::add_dir_args(&staged));
        }
        let prompt = ticket_attachments::launch_prompt(coppice_run_prompt(), &staged);
        let model = super::model_flag::selected_model(input.model.as_deref()).map(str::to_string);
        let input = AgentRunInput {
            read_only_tools: read_only,
            ..input
        };
        acp_connector::run(
            input,
            AcpRun {
                display_name: descriptor(&self.spec).display_name,
                console_prefix: self.spec.console_prefix,
                program: &self.command,
                args,
                env: env_for(&self.spec, read_only),
                worktree: &worktree,
                prompt: &prompt,
                run_timeout: self.run_timeout,
                model,
                sign_in_hint: Some(self.spec.sign_in_hint.to_string()),
                session_mode: self.spec.session_mode,
            },
        )
        .await
    }
}

/// Models come from the session the CLI opens: one handshake, no prompt.
pub struct AcpCliModels {
    spec: AcpCliSpec,
    command: String,
    model_providers: Vec<String>,
    cache: ModelListCache,
    gate: tokio::sync::Mutex<()>,
}

impl AcpCliModels {
    pub fn new(spec: AcpCliSpec, command: String, model_providers: Vec<String>) -> Self {
        Self {
            spec,
            command,
            model_providers,
            cache: ModelListCache::new(),
            gate: tokio::sync::Mutex::new(()),
        }
    }

    fn namespace(&self) -> &'static str {
        descriptor(&self.spec)
            .default_model_providers
            .first()
            .copied()
            .unwrap_or(self.spec.id)
    }

    async fn load(&self) -> anyhow::Result<Vec<ModelInfo>> {
        let dir = tempfile::tempdir()?;
        let launch = AcpLaunch {
            display_name: descriptor(&self.spec).display_name.to_string(),
            program: self.command.clone(),
            args: launch_args(&self.spec, true, None),
            env: env_for(&self.spec, true),
            cwd: dir.path().to_path_buf(),
            run_id: None,
            ticket_id: None,
            task_log_dir: None,
            sign_in_hint: Some(self.spec.sign_in_hint.to_string()),
        };
        let turn = AcpTurn {
            cwd: dir.path(),
            mcp: None,
            model: None,
            resume_session_id: None,
            prompt: "",
            policy: ToolPolicy::ReadOnly,
            session_mode: None,
        };
        let outcome = acp::run_turn(
            launch,
            turn,
            AcpLimits::for_run(Duration::from_secs(60)),
            &mut NoObserver,
            None,
            None,
        )
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
        Ok(outcome
            .models
            .into_iter()
            .map(|(id, name)| ModelInfo { id, name })
            .collect())
    }
}

#[async_trait]
impl ModelCatalog for AcpCliModels {
    async fn model_providers(&self, _refresh: bool) -> Vec<String> {
        let mut providers = super::cli_catalog::effective_model_providers(
            &self.model_providers,
            descriptor(&self.spec).default_model_providers,
        );
        // One model namespace; keep it askable even if an old config left it out.
        let namespace = self.namespace().to_string();
        if !providers.contains(&namespace) {
            providers.push(namespace);
        }
        providers
    }

    async fn list_models(
        &self,
        model_provider: &str,
        refresh: bool,
    ) -> anyhow::Result<Vec<ModelInfo>> {
        if model_provider != self.namespace() {
            return Err(anyhow::anyhow!("unknown model provider `{model_provider}`"));
        }
        let _guard = self.gate.lock().await;
        if let Some(hit) = self.cache.get(model_provider, refresh) {
            return Ok(hit);
        }
        let models = self.load().await?;
        if !models.is_empty() {
            self.cache.store(model_provider, models.clone());
        }
        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_pair(args: &[String], a: &str, b: &str) -> bool {
        args.windows(2).any(|w| w[0] == a && w[1] == b)
    }

    #[test]
    fn copilot_read_only_denies_shell_and_write_and_built_in_mcp() {
        let ro = launch_args(&COPILOT, true, None);
        assert_eq!(ro.first().map(String::as_str), Some("--acp"));
        assert!(has_pair(&ro, "--deny-tool", "shell"));
        assert!(has_pair(&ro, "--deny-tool", "write"));
        assert!(ro.iter().any(|a| a == "--disable-builtin-mcps"));
        assert!(ro.iter().any(|a| a == "--no-ask-user"));
        let write = launch_args(&COPILOT, false, None);
        assert!(write.iter().any(|a| a == "--allow-all-tools"));
        assert!(!write.iter().any(|a| a == "--deny-tool"));
    }

    #[test]
    fn qwen_never_uses_plan_mode_and_read_only_drops_writing_tools() {
        for read_only in [true, false] {
            let args = launch_args(&QWEN_CODE, read_only, None);
            assert!(has_pair(&args, "--approval-mode", "yolo"), "{args:?}");
            assert!(!args.iter().any(|a| a == "plan"));
            let excluded = args[args.iter().position(|a| a == "--exclude-tools").unwrap() + 1].clone();
            assert!(excluded.contains("enter_plan_mode"));
            assert_eq!(excluded.contains("run_shell_command"), read_only, "{excluded}");
            assert_eq!(excluded.contains("write_file"), read_only, "{excluded}");
        }
    }

    #[test]
    fn grok_puts_agent_options_before_stdio_and_sandboxes_read_only() {
        for read_only in [true, false] {
            assert_eq!(
                launch_args(&GROK, read_only, None),
                ["agent", "--always-approve", "--no-leader", "stdio"]
            );
        }
        assert_eq!(
            env_for(&GROK, true),
            [("GROK_SANDBOX".to_string(), "read-only".to_string())]
        );
        assert!(env_for(&GROK, false).is_empty());
        assert!(env_for(&KIMI, true).is_empty());
    }

    #[test]
    fn temp_folders_are_spotted_after_resolving_links() {
        let roots = vec![PathBuf::from("/tmp"), PathBuf::from("/private/var/folders/x/T")];
        assert!(in_temp_dir(Path::new("/tmp/wt"), &roots));
        assert!(in_temp_dir(Path::new("/private/var/folders/x/T/coppice/wt"), &roots));
        assert!(!in_temp_dir(Path::new("/home/me/.local/share/coppice/worktrees/a"), &roots));
        assert!(!in_temp_dir(Path::new("/tmpfoo/wt"), &roots));
        let real = temp_roots();
        assert!(real.iter().any(|r| r == &std::env::temp_dir()));
    }

    #[test]
    fn kimi_is_just_acp_and_sets_yolo_in_the_session() {
        assert_eq!(launch_args(&KIMI, true, None), ["acp"]);
        assert_eq!(launch_args(&KIMI, false, None), ["acp"]);
        assert_eq!(KIMI.session_mode, Some("yolo"));
    }
}
