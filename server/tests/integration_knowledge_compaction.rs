mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;
use coppice_server::config::{AppConfig, KnowledgeConfig};
use coppice_server::domain::knowledge_compaction::{BatchStatus, BatchTrigger, CompactionBatch};
use coppice_server::services::knowledge_compaction_service::{
    CompactionError, KnowledgeCompactionService,
};
use coppice_server::services::workspace_settings_service::WorkspaceSettingsService;
use coppice_server::AppState;
use sqlx::PgPool;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

struct Fixture {
    state: Arc<AppState>,
    app: Router,
    cookie: String,
    csrf: String,
    board_id: String,
    _env: Option<common::AgentTestEnv>,
}

impl Fixture {
    async fn new() -> Self {
        let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
        let board_id = common::create_test_board(&app, &cookie, &csrf).await;
        Self {
            state,
            app,
            cookie,
            csrf,
            board_id,
            _env: None,
        }
    }

    /// Fixture with job workers running the mock provider. `mock_response`
    /// overrides the fixture for every run (empty = agent-keyed fixture).
    async fn with_workers(mock_response: &str, configure: impl FnOnce(&mut AppConfig)) -> Self {
        let (state, app, cookie, csrf, env) =
            common::bootstrap_and_login_with_state_and_workers(mock_response, configure).await;
        let board_id = common::create_test_board(&app, &cookie, &csrf).await;
        Self {
            state,
            app,
            cookie,
            csrf,
            board_id,
            _env: Some(env),
        }
    }

    fn service(&self) -> KnowledgeCompactionService<'_> {
        KnowledgeCompactionService::new(self.pool(), &self.state.config.knowledge)
    }

    /// Run scheduler steps until no batch is active and the cycle has ended
    /// (queue drained or last batch failed). Returns the batches in order.
    async fn drive_until_idle(&self) -> Vec<CompactionBatch> {
        let service = self.service();
        for _ in 0..300 {
            service.tick(OffsetDateTime::now_utc()).await.unwrap();
            if service.active_batch().await.unwrap().is_none() {
                let last = service.last_batch().await.unwrap();
                let queued = service.queue_counts().await.unwrap().queued;
                if queued == 0 || last.is_some_and(|batch| batch.status == BatchStatus::Failed) {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(
            service.active_batch().await.unwrap().is_none(),
            "compaction did not settle"
        );
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM knowledge_compaction_batches ORDER BY created_at, id",
        )
        .fetch_all(self.pool())
        .await
        .unwrap();
        let mut batches = Vec::new();
        for id in ids {
            batches.push(service.batch(id).await.unwrap().unwrap());
        }
        batches
    }

    async fn api(&self, method: &str, path: &str) -> (axum::http::StatusCode, serde_json::Value) {
        let response = self
            .app
            .clone()
            .oneshot(common::json_request(method, path, "", &self.cookie, &self.csrf))
            .await
            .unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null))
    }

    async fn count(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(self.pool()).await.unwrap()
    }

    fn pool(&self) -> &PgPool {
        self.state.db.as_ref().unwrap()
    }

    async fn admin_id(&self) -> Uuid {
        sqlx::query_scalar("SELECT id FROM users WHERE email = 'admin@localhost'")
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    async fn configure_agent(&self) -> Uuid {
        let agent_id = Uuid::parse_str(
            &common::create_agent_with_preset_key(
                &self.app,
                "backend_engineer",
                "Compactor",
                &self.cookie,
                &self.csrf,
            )
            .await,
        )
        .unwrap();
        WorkspaceSettingsService::new(self.pool())
            .set_knowledge_compaction_agent(Some(agent_id), self.admin_id().await)
            .await
            .unwrap();
        agent_id
    }

    async fn ticket(&self) -> Uuid {
        Uuid::parse_str(
            &common::create_test_ticket(&self.app, &self.board_id, &self.cookie, &self.csrf).await,
        )
        .unwrap()
    }

    async fn set_status(&self, ticket_id: Uuid, status: &str) {
        sqlx::query("UPDATE tickets SET status = $2 WHERE id = $1")
            .bind(ticket_id)
            .bind(status)
            .execute(self.pool())
            .await
            .unwrap();
    }

    async fn done_ticket(&self) -> Uuid {
        let ticket_id = self.ticket().await;
        self.set_status(ticket_id, "done").await;
        ticket_id
    }

    async fn backdate_queue(&self, ticket_id: Uuid, by: Duration) {
        sqlx::query(
            "UPDATE knowledge_compaction_queue SET enqueued_at = now() - $2::interval WHERE ticket_id = $1",
        )
        .bind(ticket_id)
        .bind(format!("{} seconds", by.whole_seconds()))
        .execute(self.pool())
        .await
        .unwrap();
    }

    async fn queue_rows(&self) -> Vec<(Uuid, Option<Uuid>, i32)> {
        sqlx::query_as(
            "SELECT ticket_id, batch_id, attempts FROM knowledge_compaction_queue ORDER BY enqueued_at, ticket_id",
        )
        .fetch_all(self.pool())
        .await
        .unwrap()
    }

    /// Mark a batch succeeded the way the completion handler will.
    async fn succeed_batch(&self, batch_id: Uuid) {
        sqlx::query(
            "UPDATE knowledge_compaction_batches SET status = 'succeeded', ended_at = now() WHERE id = $1",
        )
        .bind(batch_id)
        .execute(self.pool())
        .await
        .unwrap();
        sqlx::query("UPDATE agent_runs SET status = 'succeeded' WHERE compaction_batch_id = $1")
            .bind(batch_id)
            .execute(self.pool())
            .await
            .unwrap();
        sqlx::query("DELETE FROM knowledge_compaction_queue WHERE batch_id = $1")
            .bind(batch_id)
            .execute(self.pool())
            .await
            .unwrap();
    }
}

fn config_with(edit: impl FnOnce(&mut KnowledgeConfig)) -> KnowledgeConfig {
    let mut config = coppice_server::AppConfig::load_defaults()
        .unwrap()
        .knowledge;
    edit(&mut config);
    config
}

#[tokio::test]
async fn done_transitions_queue_tickets_once_and_reopen_removes_unbatched_rows() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::new().await;

    let ticket_id = fx.done_ticket().await;
    fx.set_status(ticket_id, "done").await;
    assert_eq!(fx.queue_rows().await.len(), 1, "queued without a configured agent");

    fx.set_status(ticket_id, "in_progress").await;
    assert!(fx.queue_rows().await.is_empty());
    fx.set_status(ticket_id, "done").await;
    assert_eq!(fx.queue_rows().await.len(), 1);

    fx.configure_agent().await;
    let config = config_with(|_| {});
    let batch = KnowledgeCompactionService::new(fx.pool(), &config)
        .start_manual(fx.admin_id().await)
        .await
        .unwrap();
    fx.set_status(ticket_id, "in_progress").await;
    assert_eq!(
        fx.queue_rows().await,
        vec![(ticket_id, Some(batch.id), 0)],
        "batched rows survive a reopen"
    );
}

#[tokio::test]
async fn tick_does_nothing_without_an_enabled_agent() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::new().await;
    let ticket_id = fx.done_ticket().await;
    fx.backdate_queue(ticket_id, Duration::days(1)).await;
    let config = config_with(|_| {});
    let service = KnowledgeCompactionService::new(fx.pool(), &config);
    let later = OffsetDateTime::now_utc() + Duration::days(1);

    assert!(service.tick(later).await.unwrap().is_none());

    let agent_id = fx.configure_agent().await;
    sqlx::query("UPDATE agents SET enabled = false WHERE id = $1")
        .bind(agent_id)
        .execute(fx.pool())
        .await
        .unwrap();
    assert!(service.tick(later).await.unwrap().is_none());
    assert!(matches!(
        service.start_manual(fx.admin_id().await).await,
        Err(CompactionError::AgentDisabled)
    ));
}

#[tokio::test]
async fn scheduled_tick_waits_for_the_interval_then_queues_one_batch_and_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::new().await;
    let agent_id = fx.configure_agent().await;
    let config = config_with(|config| {
        config.compaction.interval_secs = 1_800;
        config.compaction.batch_max_tickets = 2;
    });
    let service = KnowledgeCompactionService::new(fx.pool(), &config);

    let mut tickets = Vec::new();
    for _ in 0..3 {
        let ticket_id = fx.done_ticket().await;
        tickets.push(ticket_id);
    }
    for (offset, ticket_id) in tickets.iter().enumerate() {
        fx.backdate_queue(*ticket_id, Duration::minutes(10 - offset as i64))
            .await;
    }
    let now = OffsetDateTime::now_utc();
    assert!(service.tick(now).await.unwrap().is_none(), "interval not elapsed");

    let batch = service
        .tick(now + Duration::minutes(21))
        .await
        .unwrap()
        .expect("scheduled batch");
    assert_eq!(batch.status, BatchStatus::Queued);
    assert_eq!(batch.trigger, BatchTrigger::Scheduled);
    assert_eq!(batch.agent_id, agent_id);
    assert_eq!(batch.ticket_count, 2);

    let members: Vec<Uuid> = sqlx::query_scalar(
        "SELECT ticket_id FROM knowledge_compaction_batch_tickets WHERE batch_id = $1 ORDER BY ticket_id",
    )
    .bind(batch.id)
    .fetch_all(fx.pool())
    .await
    .unwrap();
    let mut oldest_two = tickets[..2].to_vec();
    oldest_two.sort();
    assert_eq!(members, oldest_two);
    let rows = fx.queue_rows().await;
    assert_eq!(rows[0].1, Some(batch.id));
    assert_eq!(rows[1].1, Some(batch.id));
    assert_eq!(rows[2].1, None);

    let run: (Uuid, Option<Uuid>, Option<Uuid>, String, String, String, Uuid) = sqlx::query_as(
        r#"
        SELECT id, ticket_id, chat_session_id, job_type, context_profile, status, agent_id
        FROM agent_runs WHERE compaction_batch_id = $1
        "#,
    )
    .bind(batch.id)
    .fetch_one(fx.pool())
    .await
    .unwrap();
    assert_eq!(Some(run.0), batch.run_id);
    assert_eq!(run.1, None);
    assert_eq!(run.2, None);
    assert_eq!(run.3, "compact_knowledge");
    assert_eq!(run.4, "knowledge_compaction");
    assert_eq!(run.5, "queued");
    assert_eq!(run.6, agent_id);
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM agent_jobs WHERE run_id = $1 AND job_type = 'compact_knowledge'",
    )
    .bind(run.0)
    .fetch_one(fx.pool())
    .await
    .unwrap();
    assert_eq!(jobs, 1);

    assert!(
        service
            .tick(now + Duration::hours(2))
            .await
            .unwrap()
            .is_none(),
        "one active batch at a time"
    );
    let racing_insert = sqlx::query(
        r#"
        INSERT INTO knowledge_compaction_batches (
            id, agent_id, status, trigger, cycle_id, cycle_started_at, ticket_count
        ) VALUES ($1, $2, 'queued', 'manual', $3, now(), 1)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(agent_id)
    .bind(Uuid::new_v4())
    .execute(fx.pool())
    .await
    .expect_err("the unique index rejects a second active batch");
    assert_eq!(
        racing_insert
            .as_database_error()
            .and_then(|error| error.constraint()),
        Some("knowledge_compaction_one_active_batch")
    );
}

#[tokio::test]
async fn successful_batches_drain_the_cycle_but_not_newer_tickets() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::new().await;
    fx.configure_agent().await;
    let config = config_with(|config| config.compaction.batch_max_tickets = 1);
    let service = KnowledgeCompactionService::new(fx.pool(), &config);
    let first = fx.done_ticket().await;
    let second = fx.done_ticket().await;
    fx.backdate_queue(first, Duration::minutes(2)).await;
    fx.backdate_queue(second, Duration::minutes(1)).await;

    let batch = service.start_manual(fx.admin_id().await).await.unwrap();
    let late = fx.done_ticket().await;
    fx.succeed_batch(batch.id).await;

    let now = OffsetDateTime::now_utc();
    let next = service
        .tick(now)
        .await
        .unwrap()
        .expect("drain continues immediately");
    assert_eq!(next.cycle_id, batch.cycle_id);
    assert_eq!(next.trigger, BatchTrigger::Manual);
    assert_eq!(fx.queue_rows().await[0], (second, Some(next.id), 0));

    fx.succeed_batch(next.id).await;
    assert!(
        service.tick(now).await.unwrap().is_none(),
        "tickets queued after the cycle started wait for the next cycle"
    );
    assert_eq!(fx.queue_rows().await, vec![(late, None, 0)]);
}

#[tokio::test]
async fn byte_budget_splits_batches_and_never_skips_an_oversized_ticket() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::new().await;
    fx.configure_agent().await;
    let config = config_with(|config| config.compaction.batch_max_source_bytes = 1_000);
    let service = KnowledgeCompactionService::new(fx.pool(), &config);
    let huge = fx.done_ticket().await;
    let small = fx.done_ticket().await;
    fx.backdate_queue(huge, Duration::minutes(2)).await;
    fx.backdate_queue(small, Duration::minutes(1)).await;
    sqlx::query("UPDATE tickets SET description = repeat('x', 5000) WHERE id = $1")
        .bind(huge)
        .execute(fx.pool())
        .await
        .unwrap();

    let batch = service.start_manual(fx.admin_id().await).await.unwrap();
    assert_eq!(batch.ticket_count, 1);
    assert_eq!(fx.queue_rows().await[0], (huge, Some(batch.id), 0));
    assert_eq!(fx.queue_rows().await[1], (small, None, 0));
}

#[tokio::test]
async fn exhausted_tickets_wait_for_retry() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::new().await;
    fx.configure_agent().await;
    let config = config_with(|config| config.compaction.max_attempts = 2);
    let service = KnowledgeCompactionService::new(fx.pool(), &config);
    let admin_id = fx.admin_id().await;

    assert!(matches!(
        service.start_manual(admin_id).await,
        Err(CompactionError::NothingQueued)
    ));
    assert!(matches!(
        service.retry(admin_id).await,
        Err(CompactionError::NothingToRetry)
    ));

    let ticket_id = fx.done_ticket().await;
    sqlx::query("UPDATE knowledge_compaction_queue SET attempts = 2 WHERE ticket_id = $1")
        .bind(ticket_id)
        .execute(fx.pool())
        .await
        .unwrap();
    let counts = service.queue_counts().await.unwrap();
    assert_eq!((counts.queued, counts.blocked), (0, 1));
    assert!(matches!(
        service.start_manual(admin_id).await,
        Err(CompactionError::NothingQueued)
    ));

    let batch = service.retry(admin_id).await.unwrap();
    assert_eq!(batch.trigger, BatchTrigger::Retry);
    assert_eq!(fx.queue_rows().await, vec![(ticket_id, Some(batch.id), 0)]);
    assert!(matches!(
        service.start_manual(admin_id).await,
        Err(CompactionError::ActiveBatch)
    ));
}

#[tokio::test]
async fn manual_start_requires_a_configured_agent() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::new().await;
    fx.done_ticket().await;
    let config = config_with(|_| {});
    assert!(matches!(
        KnowledgeCompactionService::new(fx.pool(), &config)
            .start_manual(fx.admin_id().await)
            .await,
        Err(CompactionError::NotConfigured)
    ));
}

#[tokio::test]
async fn compaction_run_creates_pending_candidates_without_notifications() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::with_workers("", |_| {}).await;
    fx.configure_agent().await;
    let first = fx.done_ticket().await;
    fx.backdate_queue(first, Duration::minutes(1)).await;
    let newest = fx.done_ticket().await;

    fx.service().start_manual(fx.admin_id().await).await.unwrap();
    let batches = fx.drive_until_idle().await;
    assert_eq!(batches.len(), 1);
    let batch = &batches[0];
    assert_eq!(batch.status, BatchStatus::Succeeded, "{:?}", batch.error_message);
    assert_eq!(batch.candidate_count, Some(2));
    assert!(fx.queue_rows().await.is_empty());

    let run_id = batch.run_id.unwrap();
    let run_status: String = sqlx::query_scalar("SELECT status FROM agent_runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(fx.pool())
        .await
        .unwrap();
    assert_eq!(run_status, "succeeded");

    let items: Vec<(String, String, Option<Uuid>, Option<Uuid>, String)> = sqlx::query_as(
        r#"
        SELECT i.status, r.source_type, r.source_id, r.source_run_id, r.title
        FROM knowledge_items i
        JOIN knowledge_revisions r ON r.id = i.current_revision_id
        WHERE i.compaction_batch_id = $1
        ORDER BY i.compaction_candidate_index
        "#,
    )
    .bind(batch.id)
    .fetch_all(fx.pool())
    .await
    .unwrap();
    assert_eq!(items.len(), 2);
    for item in &items {
        assert_eq!(item.0, "pending", "default policy is fail-closed");
        assert_eq!(item.1, "ticket");
        assert_eq!(item.3, Some(run_id));
    }
    assert_eq!(items[0].2, Some(newest));
    assert_eq!(items[1].2, Some(first));
    assert_eq!(items[0].4, "Use make test-unit while iterating");
    assert_eq!(fx.count("SELECT count(*) FROM knowledge_item_sources").await, 2);
    assert_eq!(fx.count("SELECT count(*) FROM notifications").await, 0);
    assert_eq!(fx.count("SELECT count(*) FROM knowledge_usage_logs").await, 0);
}

#[tokio::test]
async fn drain_cycle_runs_sequential_batches_until_the_queue_is_empty() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::with_workers("compact_knowledge_empty", |config| {
        config.knowledge.compaction.batch_max_tickets = 10;
    })
    .await;
    fx.configure_agent().await;
    for _ in 0..25 {
        fx.done_ticket().await;
    }
    sqlx::query("UPDATE knowledge_compaction_queue SET enqueued_at = now() - interval '1 minute'")
        .execute(fx.pool())
        .await
        .unwrap();

    fx.service().start_manual(fx.admin_id().await).await.unwrap();
    let batches = fx.drive_until_idle().await;
    assert_eq!(
        batches.iter().map(|batch| batch.ticket_count).collect::<Vec<_>>(),
        vec![10, 10, 5]
    );
    assert!(batches.iter().all(|batch| batch.status == BatchStatus::Succeeded
        && batch.candidate_count == Some(0)
        && batch.cycle_id == batches[0].cycle_id));
    assert!(fx.queue_rows().await.is_empty());
}

#[tokio::test]
async fn invalid_candidates_are_dropped_with_reasons_in_the_summary() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::with_workers("compact_knowledge_invalid_source", |_| {}).await;
    fx.configure_agent().await;
    fx.done_ticket().await;

    fx.service().start_manual(fx.admin_id().await).await.unwrap();
    let batch = fx.drive_until_idle().await.remove(0);
    assert_eq!(batch.status, BatchStatus::Succeeded);
    assert_eq!(batch.candidate_count, Some(1));
    let summary = batch.summary.unwrap();
    assert!(summary.contains("Dropped 1 candidate"), "{summary}");
    assert!(summary.contains("not in this batch"), "{summary}");
}

#[tokio::test]
async fn failed_run_releases_tickets_notifies_every_user_once_and_retry_recovers() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::with_workers("compact_knowledge_malformed", |_| {}).await;
    fx.configure_agent().await;
    let ticket_id = fx.done_ticket().await;

    fx.service().start_manual(fx.admin_id().await).await.unwrap();
    let batches = fx.drive_until_idle().await;
    assert_eq!(batches.len(), 1, "a failure ends the cycle");
    let failed = &batches[0];
    assert_eq!(failed.status, BatchStatus::Failed);
    assert!(failed.error_message.is_some());
    assert_eq!(fx.queue_rows().await, vec![(ticket_id, None, 1)]);

    let users = fx.count("SELECT count(*) FROM users").await;
    let notifications: Vec<(String, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT type, ticket_id, source_key FROM notifications ORDER BY recipient_user_id",
    )
    .fetch_all(fx.pool())
    .await
    .unwrap();
    assert_eq!(notifications.len() as i64, users);
    for (kind, ticket, source_key) in &notifications {
        assert_eq!(kind, "knowledge_compaction_failed");
        assert_eq!(*ticket, None);
        assert_eq!(source_key, &format!("knowledge_compaction_failed:{}", failed.id));
    }
    fx.service().reconcile().await.unwrap();
    assert_eq!(fx.count("SELECT count(*) FROM notifications").await, users);

    std::env::set_var("MOCK_AGENT_RESPONSE", "compact_knowledge_empty");
    let retry = fx.service().retry(fx.admin_id().await).await.unwrap();
    assert_eq!(retry.trigger, BatchTrigger::Retry);
    let batches = fx.drive_until_idle().await;
    assert_eq!(batches.last().unwrap().status, BatchStatus::Succeeded);
    assert!(fx.queue_rows().await.is_empty());
    assert_eq!(fx.count("SELECT count(*) FROM notifications").await, users);
}

#[tokio::test]
async fn cancelled_run_fails_the_batch_without_counting_or_notifying() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::new().await;
    fx.configure_agent().await;
    let ticket_id = fx.done_ticket().await;
    let config = config_with(|_| {});
    let service = KnowledgeCompactionService::new(fx.pool(), &config);
    let batch = service.start_manual(fx.admin_id().await).await.unwrap();
    sqlx::query("UPDATE agent_runs SET status = 'cancelled', ended_at = now() WHERE compaction_batch_id = $1")
        .bind(batch.id)
        .execute(fx.pool())
        .await
        .unwrap();

    let resolved = service.reconcile().await.unwrap();
    assert_eq!(resolved.len(), 1);
    assert!(!resolved[0].notified);
    let batch = service.batch(batch.id).await.unwrap().unwrap();
    assert_eq!(batch.status, BatchStatus::Failed);
    assert_eq!(batch.error_message.as_deref(), Some("cancelled"));
    assert_eq!(fx.queue_rows().await, vec![(ticket_id, None, 0)]);
    assert_eq!(fx.count("SELECT count(*) FROM notifications").await, 0);
}

#[tokio::test]
async fn connector_without_read_only_enforcement_fails_closed() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::with_workers("", |_| {}).await;
    let agent_id = fx.configure_agent().await;
    sqlx::query("UPDATE agents SET connector = 'codex' WHERE id = $1")
        .bind(agent_id)
        .execute(fx.pool())
        .await
        .unwrap();
    fx.done_ticket().await;

    fx.service().start_manual(fx.admin_id().await).await.unwrap();
    let batch = fx.drive_until_idle().await.remove(0);
    assert_eq!(batch.status, BatchStatus::Failed);
    let message = batch.error_message.unwrap();
    assert!(message.contains("cannot enforce read-only"), "{message}");
    assert_eq!(fx.count("SELECT count(*) FROM knowledge_items").await, 0);
}

#[tokio::test]
async fn status_api_reports_each_state() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::new().await;
    fx.done_ticket().await;

    let (status, body) = fx.api("GET", "/api/knowledge/compaction").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "not_configured");
    assert_eq!(body["configured"], false);
    assert_eq!(body["queuedCount"], 1);
    assert!(body["agent"].is_null());

    let agent_id = fx.configure_agent().await;
    let (_, body) = fx.api("GET", "/api/knowledge/compaction").await;
    assert_eq!(body["state"], "idle");
    assert_eq!(body["agent"]["id"], agent_id.to_string());
    assert!(body["nextScheduledAt"].is_string());
    assert!(body["activeBatch"].is_null());

    sqlx::query("UPDATE agents SET enabled = false WHERE id = $1")
        .bind(agent_id)
        .execute(fx.pool())
        .await
        .unwrap();
    let (_, body) = fx.api("GET", "/api/knowledge/compaction").await;
    assert_eq!(body["state"], "agent_disabled");
    assert!(body["nextScheduledAt"].is_null());
    sqlx::query("UPDATE agents SET enabled = true WHERE id = $1")
        .bind(agent_id)
        .execute(fx.pool())
        .await
        .unwrap();

    let (status, batch) = fx.api("POST", "/api/knowledge/compaction/run").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(batch["status"], "queued");
    assert_eq!(batch["trigger"], "manual");
    assert_eq!(batch["ticketCount"], 1);
    let (_, body) = fx.api("GET", "/api/knowledge/compaction").await;
    assert_eq!(body["state"], "running");
    assert_eq!(body["activeBatch"]["id"], batch["id"]);
    assert!(body["activeBatch"]["runId"].is_string());
    assert_eq!(body["queuedCount"], 0);

    let batch_id = Uuid::parse_str(batch["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE agent_runs SET status = 'failed', error_message = 'provider exploded' WHERE compaction_batch_id = $1")
        .bind(batch_id)
        .execute(fx.pool())
        .await
        .unwrap();
    let (_, body) = fx.api("GET", "/api/knowledge/compaction").await;
    assert_eq!(body["state"], "running", "status does not reconcile on read");
    fx.service().reconcile().await.unwrap();
    let (_, body) = fx.api("GET", "/api/knowledge/compaction").await;
    assert_eq!(body["state"], "failed");
    assert_eq!(body["lastBatch"]["errorMessage"], "provider exploded");
    assert_eq!(body["queuedCount"], 1);
}

#[tokio::test]
async fn control_api_conflicts_and_cancel() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let fx = Fixture::new().await;

    let (status, body) = fx.api("POST", "/api/knowledge/compaction/run").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["message"].as_str().unwrap().contains("no knowledge compaction agent"));

    fx.configure_agent().await;
    let (status, _) = fx.api("POST", "/api/knowledge/compaction/run").await;
    assert_eq!(status, StatusCode::CONFLICT, "nothing queued");
    let (status, _) = fx.api("POST", "/api/knowledge/compaction/cancel").await;
    assert_eq!(status, StatusCode::CONFLICT, "nothing to cancel");

    let ticket_id = fx.done_ticket().await;
    let (status, batch) = fx.api("POST", "/api/knowledge/compaction/run").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (status, _) = fx.api("POST", "/api/knowledge/compaction/run").await;
    assert_eq!(status, StatusCode::CONFLICT, "a batch is already active");

    let (status, cancelled) = fx.api("POST", "/api/knowledge/compaction/cancel").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(cancelled["id"], batch["id"]);
    assert_eq!(cancelled["status"], "failed");
    assert_eq!(cancelled["errorMessage"], "cancelled");
    let run_status: String = sqlx::query_scalar(
        "SELECT status FROM agent_runs WHERE compaction_batch_id = $1",
    )
    .bind(Uuid::parse_str(batch["id"].as_str().unwrap()).unwrap())
    .fetch_one(fx.pool())
    .await
    .unwrap();
    assert_eq!(run_status, "cancelled");
    assert_eq!(fx.queue_rows().await, vec![(ticket_id, None, 0)]);
    assert_eq!(fx.count("SELECT count(*) FROM notifications").await, 0);

    let (status, _) = fx.api("POST", "/api/knowledge/compaction/retry").await;
    assert_eq!(status, StatusCode::CONFLICT, "cancel does not count an attempt");
    sqlx::query("UPDATE knowledge_compaction_queue SET attempts = 1")
        .execute(fx.pool())
        .await
        .unwrap();
    let (status, retried) = fx.api("POST", "/api/knowledge/compaction/retry").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(retried["trigger"], "retry");

    for path in [
        "/api/knowledge/compaction/run",
        "/api/knowledge/compaction/retry",
        "/api/knowledge/compaction/cancel",
    ] {
        let response = fx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header(axum::http::header::COOKIE, &fx.cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path} requires CSRF");
    }
}
