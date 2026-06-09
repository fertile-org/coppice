use super::{AgentProvider, AgentRunInput, AgentRunResult, ProviderError};
use crate::sessions::opencode_events::{event_line_to_frame, extract_result_from_events};
use crate::sessions::opencode_serve::OpenCodeServeManager;
use async_trait::async_trait;
use coppice_config::OpenCodeProviderConfig;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::watch;

pub struct OpenCodeProvider {
    serve: Arc<OpenCodeServeManager>,
    config: OpenCodeProviderConfig,
}

impl OpenCodeProvider {
    pub fn new(serve: Arc<OpenCodeServeManager>, config: OpenCodeProviderConfig) -> Self {
        Self { serve, config }
    }
}

#[async_trait]
impl AgentProvider for OpenCodeProvider {
    fn id(&self) -> &str {
        "opencode"
    }

    async fn run(&self, input: AgentRunInput) -> Result<AgentRunResult, ProviderError> {
        let context_path = PathBuf::from(&input.context_path);
        let worktree = context_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| ProviderError::InvalidInput("bad context path".into()))?;

        let prompt =
            "Read .agent/context.md and return the Expected output contract JSON.".to_string();
        let mut args = vec![
            "run".into(),
            "--attach".into(),
            self.serve.base_url().into(),
            "--format".into(),
            "json".into(),
            "--dir".into(),
            worktree.display().to_string(),
        ];
        if let Some(model) = input.model.as_ref() {
            args.push("--model".into());
            args.push(model.clone());
        }
        // Prompt is a positional message; `-p` is `--password` in the opencode CLI.
        args.push(prompt);

        let mut child = tokio::process::Command::new(&self.config.command)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let collected = read_stdout_lines(
            &mut child,
            input.stream.as_ref(),
            input.cancel_rx,
        )
        .await?;

        let status = child.wait().await?;
        if !status.success() {
            return Err(ProviderError::InvalidFixture(format!(
                "opencode run exited with {status}"
            )));
        }

        extract_result_from_events(&collected).ok_or_else(|| {
            ProviderError::InvalidFixture("no result contract in opencode output".into())
        })
    }
}

async fn read_stdout_lines(
    child: &mut Child,
    stream: Option<&Arc<crate::sessions::run_registry::RunStreamHandle>>,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> Result<Vec<String>, ProviderError> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ProviderError::InvalidFixture("opencode run missing stdout".into()))?;
    let mut reader = BufReader::new(stdout).lines();
    let mut collected = Vec::new();
    let mut seq = 0u64;
    let mut cancel_rx = cancel_rx;

    loop {
        let line = tokio::select! {
            res = reader.next_line() => res?,
            _ = wait_for_cancel(&mut cancel_rx) => {
                let _ = child.start_kill();
                return Err(ProviderError::Cancelled);
            }
        };

        match line {
            Some(l) => {
                if let Some(handle) = stream {
                    if let Some(frame) = event_line_to_frame(seq, &l) {
                        handle.publish(frame);
                        seq += 1;
                    }
                }
                collected.push(l);
            }
            None => break,
        }
    }

    Ok(collected)
}

async fn wait_for_cancel(cancel_rx: &mut Option<watch::Receiver<bool>>) {
    loop {
        match cancel_rx {
            Some(rx) => {
                if *rx.borrow() {
                    break;
                }
                if rx.changed().await.is_err() {
                    std::future::pending::<()>().await;
                }
            }
            None => std::future::pending::<()>().await,
        }
    }
}
