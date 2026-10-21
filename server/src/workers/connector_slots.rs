use coppice_config::AgentConnectorsConfig;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// In-process per-connector in-flight counts plus the claim mutex.
///
/// One server process owns a database. These counters are not shared across
/// processes; if we ever run several servers on one DB, switch the claim
/// critical section to `pg_advisory_xact_lock`.
pub struct ConnectorSlots {
    claim_lock: tokio::sync::Mutex<()>,
    in_flight: Mutex<HashMap<String, usize>>,
}

/// Decrements the connector's in-flight count when dropped.
pub struct SlotGuard {
    slots: Arc<ConnectorSlots>,
    connector: String,
}

impl ConnectorSlots {
    pub fn new() -> Self {
        Self {
            claim_lock: tokio::sync::Mutex::new(()),
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    pub async fn lock_claim(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.claim_lock.lock().await
    }

    pub fn saturated(&self, config: &AgentConnectorsConfig) -> Vec<String> {
        let in_flight = self.in_flight.lock().expect("connector slots");
        in_flight
            .iter()
            .filter(|(id, count)| **count >= config.max_concurrent_runs(id) as usize)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn acquire(self: &Arc<Self>, connector: &str) -> SlotGuard {
        {
            let mut in_flight = self.in_flight.lock().expect("connector slots");
            *in_flight.entry(connector.to_string()).or_insert(0) += 1;
        }
        SlotGuard {
            slots: Arc::clone(self),
            connector: connector.to_string(),
        }
    }

    pub fn in_flight(&self, connector: &str) -> usize {
        self.in_flight
            .lock()
            .expect("connector slots")
            .get(connector)
            .copied()
            .unwrap_or(0)
    }
}

impl Default for ConnectorSlots {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        let mut in_flight = self.slots.in_flight.lock().expect("connector slots");
        if let Some(count) = in_flight.get_mut(&self.connector) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                in_flight.remove(&self.connector);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_increments_and_drop_releases() {
        let slots = Arc::new(ConnectorSlots::new());
        assert_eq!(slots.in_flight("cursor"), 0);
        {
            let _guard = slots.acquire("cursor");
            assert_eq!(slots.in_flight("cursor"), 1);
            let _second = slots.acquire("cursor");
            assert_eq!(slots.in_flight("cursor"), 2);
        }
        assert_eq!(slots.in_flight("cursor"), 0);
    }

    #[test]
    fn saturated_lists_connectors_at_their_limit() {
        let slots = Arc::new(ConnectorSlots::new());
        let mut config = AgentConnectorsConfig::default();
        config.cursor.max_concurrent_runs = 2;
        let _a = slots.acquire("cursor");
        assert!(slots.saturated(&config).is_empty());
        let _b = slots.acquire("cursor");
        assert_eq!(slots.saturated(&config), vec!["cursor".to_string()]);
        let _c = slots.acquire("claude-code");
        let mut full = slots.saturated(&config);
        full.sort();
        assert_eq!(full, vec!["cursor".to_string()]);
    }

    #[test]
    fn drop_on_early_return_releases_the_slot() {
        let slots = Arc::new(ConnectorSlots::new());
        fn take_and_return_early(slots: &Arc<ConnectorSlots>) {
            let _guard = slots.acquire("mock");
            assert_eq!(slots.in_flight("mock"), 1);
        }
        take_and_return_early(&slots);
        assert_eq!(slots.in_flight("mock"), 0);
    }

    #[test]
    fn arc_clones_share_the_same_counters() {
        let slots = Arc::new(ConnectorSlots::new());
        let clone = Arc::clone(&slots);
        let _guard = slots.acquire("codex");
        assert_eq!(clone.in_flight("codex"), 1);
        drop(_guard);
        assert_eq!(clone.in_flight("codex"), 0);
        assert_eq!(slots.in_flight("codex"), 0);
    }
}
