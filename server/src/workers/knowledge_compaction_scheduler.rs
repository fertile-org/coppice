use std::sync::Arc;
use std::time::Duration;

use time::OffsetDateTime;

use crate::events::AppEvent;
use crate::services::knowledge_compaction_service::KnowledgeCompactionService;
use crate::AppState;

pub fn spawn_knowledge_compaction_scheduler(state: Arc<AppState>) {
    if !state.config.knowledge.enabled {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(
            state.config.knowledge.poll_interval_ms.max(100),
        ));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let Some(pool) = state.db.as_ref() else {
                continue;
            };
            let service = KnowledgeCompactionService::new(pool, &state.config.knowledge);
            match service.reconcile().await {
                Ok(failures) => {
                    for failure in &failures {
                        tracing::warn!(batch_id = %failure.batch_id, "knowledge compaction batch failed");
                    }
                    if failures.iter().any(|failure| failure.notified) {
                        state.event_bus.publish(AppEvent::NotificationChanged {
                            recipient_user_id: None,
                        });
                    }
                }
                Err(error) => tracing::warn!(error = %error, "knowledge compaction reconcile failed"),
            }
            match service.tick(OffsetDateTime::now_utc()).await {
                Ok(Some(batch)) => tracing::info!(
                    batch_id = %batch.id,
                    trigger = batch.trigger.as_str(),
                    ticket_count = batch.ticket_count,
                    "knowledge compaction batch queued"
                ),
                Ok(None) => {}
                Err(error) => tracing::warn!(error = %error, "knowledge compaction tick failed"),
            }
        }
    });
}
