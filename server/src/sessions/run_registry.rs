use crate::domain::usage::{UsageSnapshot, UsageTracker};
use crate::providers::usage::{reader_for, UsageReader};
use crate::sessions::run_artifact_log::RunArtifactLog;
use crate::sessions::LiveMessage;
use coppice_connectors::UsageFormat;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

pub struct RunStreamHandle {
    tx: broadcast::Sender<LiveMessage>,
    cancel_tx: watch::Sender<bool>,
    buffer: Arc<Mutex<Vec<LiveMessage>>>,
    snapshot: Arc<Mutex<Option<Value>>>,
    /// A live session snapshot, read only when someone asks for it.
    snapshot_source: Mutex<Option<Arc<Mutex<crate::sessions::session_snapshot::SessionSnapshot>>>>,
    /// First non-empty model the CLI reported for this run.
    reported: Mutex<Option<String>>,
    reported_tx: watch::Sender<String>,
    /// Token usage read from the connector's output, in its declared format.
    usage: Mutex<UsageState>,
    /// The latest usage snapshot. Holds no handle, so a writer subscribed to
    /// it ends when the run's handle is dropped.
    usage_tx: watch::Sender<Option<Arc<UsageSnapshot>>>,
    /// Appends console events and frames to the run folder as they arrive.
    artifact_log: Option<RunArtifactLog>,
    /// Bumped on every snapshot change, so a checkpoint can wait for idle.
    snapshot_version: watch::Sender<u64>,
}

/// Structured console events are typed `<connector>.console.<kind>`.
#[derive(Default)]
struct UsageState {
    reader: Option<Box<dyn UsageReader>>,
    tracker: UsageTracker,
}

pub fn is_console_event_type(ty: &str) -> bool {
    let mut parts = ty.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(prefix), Some("console"), Some(kind), None) if !prefix.is_empty() && !kind.is_empty()
    )
}

impl RunStreamHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<LiveMessage> {
        self.tx.subscribe()
    }

    pub fn publish(&self, msg: LiveMessage) {
        if let Some(log) = &self.artifact_log {
            match &msg {
                LiveMessage::Frame { data, .. } => log.frame(data),
                LiveMessage::Event { event }
                    if event
                        .get("type")
                        .and_then(|v| v.as_str())
                        .is_some_and(is_console_event_type) =>
                {
                    log.console(event)
                }
                _ => {}
            }
        }
        let _ = self.tx.send(msg.clone());
        if let Ok(mut buf) = self.buffer.lock() {
            buf.push(msg);
            if buf.len() > 500 {
                let drop = buf.len() - 500;
                buf.drain(0..drop);
            }
        }
    }

    pub fn set_snapshot(&self, snapshot: Value) {
        if let Ok(mut snap) = self.snapshot.lock() {
            *snap = Some(snapshot);
        }
        self.snapshot_version.send_modify(|version| *version += 1);
    }

    pub fn subscribe_snapshot_changes(&self) -> watch::Receiver<u64> {
        self.snapshot_version.subscribe()
    }

    /// The on-disk console and terminal writer, when the registry has one.
    pub fn artifact_log(&self) -> Option<&RunArtifactLog> {
        self.artifact_log.as_ref()
    }

    pub fn snapshot(&self) -> Option<Value> {
        let source = self
            .snapshot_source
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(source) = source {
            return Some(source.lock().unwrap_or_else(|e| e.into_inner()).to_value());
        }
        self.snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Serve snapshots from a live session that the caller keeps updating.
    /// Nothing is serialized until a reader asks.
    pub fn set_snapshot_source(
        &self,
        source: Arc<Mutex<crate::sessions::session_snapshot::SessionSnapshot>>,
    ) {
        *self
            .snapshot_source
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(source);
    }

    /// The live session changed; idle checkpoints wait for this to settle.
    pub fn touch_snapshot(&self) {
        self.snapshot_version.send_modify(|version| *version += 1);
    }

    pub fn publish_frame(&self, seq: u64, data: Vec<u8>) {
        self.publish(LiveMessage::Frame { seq, data });
    }

    pub fn buffered_tail(&self) -> Vec<LiveMessage> {
        self.buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn cancel(&self) {
        let _ = self.cancel_tx.send(true);
    }

    pub fn cancelled_rx(&self) -> watch::Receiver<bool> {
        self.cancel_tx.subscribe()
    }

    /// Keep the first non-empty model. Later reports, including a different
    /// model, do not replace it. Blank strings are not reports.
    pub fn note_reported_model(&self, model: &str) {
        let model = model.trim();
        if model.is_empty() {
            return;
        }
        let mut guard = self.reported.lock().unwrap_or_else(|err| err.into_inner());
        if guard.is_some() {
            return;
        }
        *guard = Some(model.to_string());
        drop(guard);
        let _ = self.reported_tx.send(model.to_string());
    }

    pub fn reported_model(&self) -> Option<String> {
        self.reported
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }

    pub fn subscribe_reported_model(&self) -> watch::Receiver<String> {
        self.reported_tx.subscribe()
    }

    /// A CLI process starts writing to this run; read its usage in `format`.
    /// Call once per process, before its output. The first format with a
    /// reader wins; a format without one leaves usage off. `resumed` says the
    /// process continues an earlier session.
    ///
    /// Publishes a snapshot right away, so a tracked run that reports nothing
    /// is still stored as tracked.
    pub fn track_usage(&self, format: UsageFormat, resumed: bool) {
        let mut state = self.usage.lock().unwrap_or_else(|err| err.into_inner());
        if state.reader.is_none() {
            state.reader = reader_for(format);
        }
        let UsageState { reader, tracker } = &mut *state;
        let Some(reader) = reader.as_mut() else {
            return;
        };
        for usage in reader.start_process(resumed) {
            tracker.apply(usage);
        }
        self.usage_tx
            .send_replace(Some(Arc::new(tracker.snapshot())));
    }

    /// One output event from the connector. Ignored until [`Self::track_usage`].
    pub fn observe_usage(&self, event: &Value) {
        let mut state = self.usage.lock().unwrap_or_else(|err| err.into_inner());
        let UsageState { reader, tracker } = &mut *state;
        let Some(reader) = reader.as_mut() else {
            return;
        };
        let events = reader.read(event);
        if events.is_empty() {
            return;
        }
        for usage in events {
            tracker.apply(usage);
        }
        // Sent under the lock so snapshots go out in version order.
        self.usage_tx
            .send_replace(Some(Arc::new(tracker.snapshot())));
    }

    /// `None` until usage is tracked ([`Self::track_usage`] with a readable format).
    pub fn usage_snapshot(&self) -> Option<Arc<UsageSnapshot>> {
        self.usage_tx.borrow().clone()
    }

    pub fn subscribe_usage(&self) -> watch::Receiver<Option<Arc<UsageSnapshot>>> {
        self.usage_tx.subscribe()
    }
}

#[derive(Default)]
pub struct RunStreamRegistry {
    inner: DashMap<Uuid, Arc<RunStreamHandle>>,
    /// `artifacts_dir` and the per-run byte cap. Without it, nothing is
    /// written while a run streams (unit tests).
    artifacts: Option<(String, u64)>,
}

impl RunStreamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Streams registered here append their console and terminal output under
    /// `<artifacts_dir>/runs/<run_id>/` as it arrives.
    pub fn with_artifacts(artifacts_dir: impl Into<String>, max_bytes: u64) -> Self {
        Self {
            inner: DashMap::new(),
            artifacts: Some((artifacts_dir.into(), max_bytes)),
        }
    }

    pub fn register(&self, run_id: Uuid) -> Arc<RunStreamHandle> {
        let (tx, _) = broadcast::channel(2048);
        let (cancel_tx, _) = watch::channel(false);
        let (reported_tx, _) = watch::channel(String::new());
        let (usage_tx, _) = watch::channel(None);
        let (snapshot_version, _) = watch::channel(0);
        let artifact_log = self.artifacts.as_ref().map(|(dir, max_bytes)| {
            RunArtifactLog::start(
                std::path::Path::new(dir)
                    .join("runs")
                    .join(run_id.to_string()),
                *max_bytes,
            )
        });
        let handle = Arc::new(RunStreamHandle {
            tx,
            cancel_tx,
            buffer: Arc::new(Mutex::new(Vec::new())),
            snapshot: Arc::new(Mutex::new(None)),
            snapshot_source: Mutex::new(None),
            reported: Mutex::new(None),
            reported_tx,
            usage: Mutex::new(UsageState::default()),
            usage_tx,
            artifact_log,
            snapshot_version,
        });
        self.inner.insert(run_id, handle.clone());
        handle
    }

    pub fn get(&self, run_id: Uuid) -> Option<Arc<RunStreamHandle>> {
        self.inner.get(&run_id).map(|e| e.clone())
    }

    pub fn remove(&self, run_id: Uuid) {
        self.inner.remove(&run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::usage::UsageStatus;

    #[tokio::test]
    async fn usage_is_read_only_after_track_usage() {
        let registry = RunStreamRegistry::new();
        let run_id = Uuid::new_v4();
        let handle = registry.register(run_id);
        let event = serde_json::json!({ "kind": "request", "tokens": { "output": 4 } });
        handle.observe_usage(&event);
        assert!(handle.usage_snapshot().is_none());

        let mut rx = handle.subscribe_usage();
        // A format without a reader leaves usage off.
        handle.track_usage(UsageFormat::None, false);
        assert!(handle.usage_snapshot().is_none());

        handle.track_usage(UsageFormat::Normalized, false);
        // Tracked before anything is reported: a version-0 snapshot.
        let snapshot = handle.usage_snapshot().expect("tracked");
        assert_eq!(snapshot.version, 0);
        assert_eq!(snapshot.totals.status, UsageStatus::None);

        // A later format does not replace the first reader.
        handle.track_usage(UsageFormat::None, true);
        handle.observe_usage(&event);
        handle.observe_usage(&serde_json::json!({ "type": "unrelated" }));
        let snapshot = handle.usage_snapshot().expect("snapshot");
        assert_eq!(snapshot.version, 1);
        assert_eq!(snapshot.totals.tokens.output, Some(4));
        assert!(rx.borrow_and_update().is_some());
        // The writer's receiver closes once the run's handle is gone.
        registry.remove(run_id);
        drop(handle);
        assert!(rx.changed().await.is_err());
    }

    #[tokio::test]
    async fn registry_broadcasts_live_messages() {
        let registry = RunStreamRegistry::new();
        let run_id = Uuid::new_v4();
        let handle = registry.register(run_id);
        let mut rx = handle.subscribe();

        handle.publish(LiveMessage::Event {
            event: serde_json::json!({"type": "message.part.delta"}),
        });

        let msg = rx.recv().await.unwrap();
        assert!(matches!(msg, LiveMessage::Event { .. }));
    }

    #[tokio::test]
    async fn registry_broadcasts_frames() {
        let registry = RunStreamRegistry::new();
        let run_id = Uuid::new_v4();
        let handle = registry.register(run_id);
        let mut rx = handle.subscribe();

        handle.publish_frame(0, b"hello\n".to_vec());

        let msg = rx.recv().await.unwrap();
        match msg {
            LiveMessage::Frame { data, .. } => assert_eq!(data, b"hello\n"),
            _ => panic!("expected frame"),
        }
    }

    #[test]
    fn reported_model_keeps_the_first_non_blank_value() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(Uuid::new_v4());
        handle.note_reported_model("  ");
        assert_eq!(handle.reported_model(), None);
        handle.note_reported_model(" composer-2.5 ");
        handle.note_reported_model("gpt-5");
        assert_eq!(handle.reported_model().as_deref(), Some("composer-2.5"));
    }

    #[test]
    fn live_buffer_keeps_only_the_last_500_messages() {
        let registry = RunStreamRegistry::new();
        let handle = registry.register(Uuid::new_v4());
        for seq in 0..600 {
            handle.publish_frame(seq, format!("line-{seq}\n").into_bytes());
        }
        let tail = handle.buffered_tail();
        assert_eq!(tail.len(), 500);
        match &tail[0] {
            LiveMessage::Frame { seq, .. } => assert_eq!(*seq, 100),
            other => panic!("expected first kept frame, got {other:?}"),
        }
        match tail.last() {
            Some(LiveMessage::Frame { seq, .. }) => assert_eq!(*seq, 599),
            other => panic!("expected last frame, got {other:?}"),
        }
    }

    #[test]
    fn remove_drops_the_handle_so_ten_runs_do_not_accumulate() {
        let registry = RunStreamRegistry::new();
        let mut ids = Vec::new();
        for _ in 0..10 {
            let id = Uuid::new_v4();
            let handle = registry.register(id);
            for seq in 0..80 {
                handle.publish_frame(seq, b"chunk\n".to_vec());
            }
            ids.push(id);
            registry.remove(id);
        }
        for id in ids {
            assert!(
                registry.get(id).is_none(),
                "stream {id} stayed after remove"
            );
        }
    }
}
