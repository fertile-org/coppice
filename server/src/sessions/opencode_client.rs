use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::json;
use tokio::sync::watch;

use crate::providers::{AgentRunResult, ProviderError};
use crate::sessions::live_message::LiveMessage;
use crate::sessions::opencode_events::{
    extract_result_from_messages, extract_result_from_snapshot, session_status_from_sse,
};
use crate::sessions::run_registry::RunStreamHandle;
use crate::sessions::session_snapshot::SessionSnapshot;

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const SSE_RECONNECT_DELAY: Duration = Duration::from_millis(750);
const RUN_IDLE_TIMEOUT: Duration = Duration::from_secs(600);
const STREAM_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

struct StreamContext {
    session_id: String,
    stream: Option<Arc<RunStreamHandle>>,
    snapshot: Arc<Mutex<SessionSnapshot>>,
    idle_flag: Arc<std::sync::atomic::AtomicBool>,
}

fn publish_sse_event(ctx: &StreamContext, event: &serde_json::Value) {
    if let Some(stream) = &ctx.stream {
        stream.publish(LiveMessage::Event {
            event: event.clone(),
        });
        if let Ok(mut snap) = ctx.snapshot.lock() {
            snap.apply_event(event);
            stream.set_snapshot(snap.to_value());
        }
    }
}

fn event_for_session(event: &serde_json::Value, session_id: &str) -> bool {
    event
        .get("properties")
        .and_then(|p| p.get("sessionID"))
        .and_then(|s| s.as_str())
        == Some(session_id)
}

pub struct OpenCodeClient {
    api: reqwest::Client,
    stream: reqwest::Client,
    base_url: String,
}

impl OpenCodeClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            api: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("opencode api client"),
            stream: reqwest::Client::builder()
                .tcp_keepalive(Duration::from_secs(30))
                .build()
                .expect("opencode stream client"),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn resolve_directory(path: &Path) -> Result<PathBuf, ProviderError> {
        path.canonicalize().map_err(|err| {
            ProviderError::InvalidInput(format!("worktree path {}: {err}", path.display()))
        })
    }

    fn url_with_directory(&self, path: &str, directory: &Path) -> Result<reqwest::Url, ProviderError> {
        let mut url = reqwest::Url::parse(&format!("{}{path}", self.base_url))
            .map_err(|err| ProviderError::InvalidInput(format!("opencode url: {err}")))?;
        url.query_pairs_mut()
            .append_pair("directory", &directory.to_string_lossy());
        Ok(url)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_session(
        &self,
        directory: &Path,
        model_provider: Option<&str>,
        model: Option<&str>,
        prompt: &str,
        stream: Option<Arc<RunStreamHandle>>,
        cancel_rx: Option<watch::Receiver<bool>>,
        session_created_tx: Option<watch::Sender<String>>,
    ) -> Result<AgentRunResult, ProviderError> {
        let directory = Self::resolve_directory(directory)?;
        let session_id = self
            .create_session(&directory, model_provider, model)
            .await?;

        if let Some(tx) = session_created_tx {
            let _ = tx.send(session_id.clone());
        }

        let idle_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let snapshot = Arc::new(Mutex::new(SessionSnapshot::empty(&session_id)));

        let ctx = StreamContext {
            session_id: session_id.clone(),
            stream,
            snapshot,
            idle_flag: idle_flag.clone(),
        };

        let events_handle = {
            let client = self.clone_inner();
            let directory = directory.clone();
            let mut cancel_rx = cancel_rx.clone();
            let ctx = ctx.clone_refs();
            tokio::spawn(async move {
                client
                    .stream_events_loop(&directory, &mut cancel_rx, ctx)
                    .await
            })
        };

        let prompt_result = self.prompt_async(&directory, &session_id, prompt).await;
        if let Err(err) = prompt_result {
            let _ = self.abort(&directory, &session_id).await;
            drain_events_task(events_handle, &idle_flag).await;
            return Err(err);
        }

        let wait_result = self
            .wait_idle(&directory, &session_id, cancel_rx, idle_flag.clone())
            .await;
        if let Err(err) = wait_result {
            let _ = self.abort(&directory, &session_id).await;
            drain_events_task(events_handle, &idle_flag).await;
            return Err(err);
        }

        drain_events_task(events_handle, &idle_flag).await;

        let messages = self.fetch_messages(&directory, &session_id).await?;
        let snapshot = ctx.snapshot.lock().map_err(|_| {
            ProviderError::InvalidFixture("snapshot lock poisoned".into())
        })?;
        extract_result_from_messages(&messages)
            .or_else(|| extract_result_from_snapshot(&snapshot))
            .ok_or_else(|| {
                ProviderError::InvalidFixture(
                    "no result contract in opencode session messages or snapshot".into(),
                )
            })
    }

    pub async fn reattach_events(
        &self,
        directory: &Path,
        session_id: &str,
        event_tx: tokio::sync::mpsc::Sender<LiveMessage>,
    ) -> Result<(), ProviderError> {
        let directory = Self::resolve_directory(directory)?;

        match self.session_status(&directory, session_id).await? {
            Some(status) if status == "idle" => return Ok(()),
            Some(_) => {}
            None => {
                return Err(ProviderError::InvalidFixture(format!(
                    "opencode session {session_id} not found"
                )));
            }
        }

        let idle_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.reattach_stream_loop(&directory, session_id, event_tx, idle_flag)
            .await
    }

}

impl StreamContext {
    fn clone_refs(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            stream: self.stream.clone(),
            snapshot: self.snapshot.clone(),
            idle_flag: self.idle_flag.clone(),
        }
    }
}

async fn drain_events_task(
    handle: tokio::task::JoinHandle<Result<(), ProviderError>>,
    idle_flag: &Arc<std::sync::atomic::AtomicBool>,
) {
    idle_flag.store(true, std::sync::atomic::Ordering::Relaxed);
    match tokio::time::timeout(STREAM_DRAIN_TIMEOUT, handle).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(err))) => tracing::warn!(%err, "opencode event stream ended with error"),
        Ok(Err(join_err)) => tracing::warn!(%join_err, "opencode event stream task panicked"),
        Err(_) => tracing::warn!("opencode event stream drain timed out; continuing"),
    }
}

impl OpenCodeClient {
    fn clone_inner(&self) -> Self {
        Self {
            api: self.api.clone(),
            stream: self.stream.clone(),
            base_url: self.base_url.clone(),
        }
    }

    async fn create_session(
        &self,
        directory: &Path,
        model_provider: Option<&str>,
        model: Option<&str>,
    ) -> Result<String, ProviderError> {
        let mut body = json!({});
        if let (Some(provider), Some(model)) = (model_provider, model) {
            body["model"] = json!({
                "id": model,
                "providerID": provider,
            });
        }

        let url = self.url_with_directory("/session", directory)?;
        let resp = self.api.post(url).json(&body).send().await.map_err(map_reqwest)?;
        let status = resp.status();
        let value: serde_json::Value = resp.json().await.map_err(map_reqwest)?;
        if !status.is_success() {
            return Err(api_error("create session", status, value));
        }
        value
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| ProviderError::InvalidFixture("opencode session missing id".into()))
    }

    async fn prompt_async(
        &self,
        directory: &Path,
        session_id: &str,
        prompt: &str,
    ) -> Result<(), ProviderError> {
        let url = self.url_with_directory(&format!("/session/{session_id}/prompt_async"), directory)?;
        let body = json!({
            "parts": [{ "type": "text", "text": prompt }],
        });
        let resp = self
            .api
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest)?;
        let status = resp.status();
        if status == reqwest::StatusCode::NO_CONTENT || status.is_success() {
            Ok(())
        } else {
            let value: serde_json::Value = resp.json().await.unwrap_or(json!({}));
            Err(api_error("prompt session", status, value))
        }
    }

    async fn abort(&self, directory: &Path, session_id: &str) -> Result<(), ProviderError> {
        let url = self.url_with_directory(&format!("/session/{session_id}/abort"), directory)?;
        let resp = self.api.post(url).send().await.map_err(map_reqwest)?;
        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let value: serde_json::Value = resp.json().await.unwrap_or(json!({}));
            Err(api_error("abort session", status, value))
        }
    }

    async fn fetch_messages(
        &self,
        directory: &Path,
        session_id: &str,
    ) -> Result<Vec<serde_json::Value>, ProviderError> {
        let url = self.url_with_directory(&format!("/session/{session_id}/message"), directory)?;
        let resp = self.api.get(url).send().await.map_err(map_reqwest)?;
        let status = resp.status();
        let value: serde_json::Value = resp.json().await.map_err(map_reqwest)?;
        if !status.is_success() {
            return Err(api_error("fetch messages", status, value));
        }
        value
            .as_array()
            .cloned()
            .ok_or_else(|| ProviderError::InvalidFixture("opencode messages not an array".into()))
    }

    pub async fn session_status(
        &self,
        directory: &Path,
        session_id: &str,
    ) -> Result<Option<String>, ProviderError> {
        let url = self.url_with_directory("/session/status", directory)?;
        let resp = self.api.get(url).send().await.map_err(map_reqwest)?;
        let status = resp.status();
        let value: serde_json::Value = resp.json().await.map_err(map_reqwest)?;
        if !status.is_success() {
            return Err(api_error("session status", status, value));
        }
        Ok(value
            .get(session_id)
            .and_then(|entry| entry.get("type"))
            .and_then(|v| v.as_str())
            .map(str::to_string))
    }

    async fn wait_idle(
        &self,
        directory: &Path,
        session_id: &str,
        cancel_rx: Option<watch::Receiver<bool>>,
        idle_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), ProviderError> {
        let deadline = tokio::time::Instant::now() + RUN_IDLE_TIMEOUT;
        let mut cancel_rx = cancel_rx;

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(ProviderError::InvalidFixture(
                    "opencode session timed out".into(),
                ));
            }

            if is_cancelled(&mut cancel_rx) {
                return Err(ProviderError::Cancelled);
            }

            // SSE session.status idle is authoritative; do not wait for REST to agree.
            if idle_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            match self.session_status(directory, session_id).await? {
                Some(status) => {
                    if mark_idle_when_status(Some(&status), &idle_flag) {
                        return Ok(());
                    }
                }
                None => {}
            }

            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn stream_events_loop(
        &self,
        directory: &Path,
        cancel_rx: &mut Option<watch::Receiver<bool>>,
        ctx: StreamContext,
    ) -> Result<(), ProviderError> {
        loop {
            if is_cancelled(cancel_rx) {
                return Err(ProviderError::Cancelled);
            }
            if ctx.idle_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            let url = self.url_with_directory("/event", directory)?;
            let resp = match self.stream.get(url).send().await {
                Ok(resp) => resp,
                Err(err) => {
                    tracing::warn!(%err, "opencode event stream connect failed, retrying");
                    tokio::time::sleep(SSE_RECONNECT_DELAY).await;
                    continue;
                }
            };

            if !resp.status().is_success() {
                tokio::time::sleep(SSE_RECONNECT_DELAY).await;
                continue;
            }

            let mut lines = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = lines.next().await {
                if is_cancelled(cancel_rx) {
                    return Err(ProviderError::Cancelled);
                }
                if ctx.idle_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    return Ok(());
                }

                let chunk = match chunk {
                    Ok(c) => c,
                    Err(err) => {
                        tracing::warn!(%err, "opencode event stream read error, reconnecting");
                        break;
                    }
                };
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim_end_matches('\r').to_string();
                    buffer = buffer[pos + 1..].to_string();

                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };
                    if data.is_empty() {
                        continue;
                    }
                    let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
                        continue;
                    };

                    if session_status_from_sse(&event, &ctx.session_id).as_deref() == Some("idle") {
                        ctx.idle_flag
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                    }

                    if event_for_session(&event, &ctx.session_id) {
                        publish_sse_event(&ctx, &event);
                    }
                }
            }

            tokio::time::sleep(SSE_RECONNECT_DELAY).await;
        }
    }

    async fn reattach_stream_loop(
        &self,
        directory: &Path,
        session_id: &str,
        event_tx: tokio::sync::mpsc::Sender<LiveMessage>,
        idle_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), ProviderError> {
        loop {
            if idle_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            let url = self.url_with_directory("/event", directory)?;
            let resp = match self.stream.get(url).send().await {
                Ok(resp) => resp,
                Err(err) => {
                    tracing::warn!(%err, "opencode reattach stream connect failed, retrying");
                    tokio::time::sleep(SSE_RECONNECT_DELAY).await;
                    continue;
                }
            };

            if !resp.status().is_success() {
                tokio::time::sleep(SSE_RECONNECT_DELAY).await;
                continue;
            }

            let mut lines = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = lines.next().await {
                if idle_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    return Ok(());
                }

                let chunk = match chunk {
                    Ok(c) => c,
                    Err(err) => {
                        tracing::warn!(%err, "opencode reattach stream read error, reconnecting");
                        break;
                    }
                };
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim_end_matches('\r').to_string();
                    buffer = buffer[pos + 1..].to_string();

                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };
                    if data.is_empty() {
                        continue;
                    }
                    let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
                        continue;
                    };

                    if session_status_from_sse(&event, session_id).as_deref() == Some("idle") {
                        idle_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        return Ok(());
                    }

                    if event_for_session(&event, session_id)
                        && event_tx
                            .send(LiveMessage::Event {
                                event: event.clone(),
                            })
                            .await
                            .is_err()
                    {
                        return Ok(());
                    }
                }
            }

            tokio::time::sleep(SSE_RECONNECT_DELAY).await;
        }
    }
}

fn is_cancelled(cancel_rx: &mut Option<watch::Receiver<bool>>) -> bool {
    match cancel_rx {
        Some(rx) => *rx.borrow(),
        None => false,
    }
}

fn map_reqwest(err: reqwest::Error) -> ProviderError {
    ProviderError::InvalidFixture(format!("opencode http: {err}"))
}

fn api_error(action: &str, status: reqwest::StatusCode, body: serde_json::Value) -> ProviderError {
    ProviderError::InvalidFixture(format!(
        "opencode {action} failed ({status}): {body}"
    ))
}

fn mark_idle_when_status(status: Option<&str>, idle_flag: &std::sync::atomic::AtomicBool) -> bool {
    if status == Some("idle") {
        idle_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_idle_when_status_sets_flag() {
        let flag = std::sync::atomic::AtomicBool::new(false);
        assert!(mark_idle_when_status(Some("idle"), &flag));
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!mark_idle_when_status(Some("busy"), &flag));
    }

    #[test]
    fn sse_idle_flag_survives_rest_busy_status() {
        let flag = std::sync::atomic::AtomicBool::new(false);
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(!mark_idle_when_status(Some("busy"), &flag));
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
    }
}
