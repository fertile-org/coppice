use crate::sessions::LiveMessage;
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
    /// First non-empty model the CLI reported for this run.
    reported: Mutex<Option<String>>,
    reported_tx: watch::Sender<String>,
}

impl RunStreamHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<LiveMessage> {
        self.tx.subscribe()
    }

    pub fn publish(&self, msg: LiveMessage) {
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
    }

    pub fn snapshot(&self) -> Option<Value> {
        self.snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
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
}

#[derive(Default)]
pub struct RunStreamRegistry {
    inner: DashMap<Uuid, Arc<RunStreamHandle>>,
}

impl RunStreamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, run_id: Uuid) -> Arc<RunStreamHandle> {
        let (tx, _) = broadcast::channel(2048);
        let (cancel_tx, _) = watch::channel(false);
        let (reported_tx, _) = watch::channel(String::new());
        let handle = Arc::new(RunStreamHandle {
            tx,
            cancel_tx,
            buffer: Arc::new(Mutex::new(Vec::new())),
            snapshot: Arc::new(Mutex::new(None)),
            reported: Mutex::new(None),
            reported_tx,
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
}
