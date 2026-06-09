use crate::sessions::LiveMessage;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

pub struct RunStreamHandle {
    tx: broadcast::Sender<LiveMessage>,
    cancel_tx: watch::Sender<bool>,
    buffer: Arc<std::sync::Mutex<Vec<LiveMessage>>>,
    snapshot: Arc<std::sync::Mutex<Option<Value>>>,
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
        self.snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn publish_frame(&self, seq: u64, data: Vec<u8>) {
        self.publish(LiveMessage::Frame { seq, data });
    }

    pub fn buffered_tail(&self) -> Vec<LiveMessage> {
        self.buffer.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn cancel(&self) {
        let _ = self.cancel_tx.send(true);
    }

    pub fn cancelled_rx(&self) -> watch::Receiver<bool> {
        self.cancel_tx.subscribe()
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
        let handle = Arc::new(RunStreamHandle {
            tx,
            cancel_tx,
            buffer: Arc::new(std::sync::Mutex::new(Vec::new())),
            snapshot: Arc::new(std::sync::Mutex::new(None)),
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
}
