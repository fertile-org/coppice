use crate::sessions::TerminalFrame;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

pub struct RunStreamHandle {
    tx: broadcast::Sender<TerminalFrame>,
    cancel_tx: watch::Sender<bool>,
    buffer: Arc<std::sync::Mutex<Vec<TerminalFrame>>>,
}

impl RunStreamHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<TerminalFrame> {
        self.tx.subscribe()
    }

    pub fn publish(&self, frame: TerminalFrame) {
        let _ = self.tx.send(frame.clone());
        if let Ok(mut buf) = self.buffer.lock() {
            buf.push(frame);
            if buf.len() > 500 {
                let drop = buf.len() - 500;
                buf.drain(0..drop);
            }
        }
    }

    pub fn buffered_tail(&self) -> Vec<TerminalFrame> {
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
        let (tx, _) = broadcast::channel(256);
        let (cancel_tx, _) = watch::channel(false);
        let handle = Arc::new(RunStreamHandle {
            tx,
            cancel_tx,
            buffer: Arc::new(std::sync::Mutex::new(Vec::new())),
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
    use time::OffsetDateTime;

    #[tokio::test]
    async fn registry_broadcasts_frames() {
        let registry = RunStreamRegistry::new();
        let run_id = Uuid::new_v4();
        let handle = registry.register(run_id);
        let mut rx = handle.subscribe();

        handle.publish(TerminalFrame {
            seq: 0,
            data: b"hello\n".to_vec(),
            ts: OffsetDateTime::now_utc(),
        });

        let frame = rx.recv().await.unwrap();
        assert_eq!(frame.data, b"hello\n");
    }
}
