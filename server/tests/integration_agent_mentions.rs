mod common;

use axum::http::StatusCode;
use coppice_server::domain::comment::{AuthorType, CommentIntent};
use coppice_server::providers::AgentRequest;
use coppice_server::sandbox::permissive::PROFILE_ID;
use coppice_server::services::agent_request::append_agent_requests_to_comment;
use coppice_server::services::comment_service::CommentService;
use coppice_server::services::mention_service::MentionService;
use coppice_server::services::run_orchestrator::RunOrchestrator;
use coppice_server::services::run_service::RunService;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn successful_attention_mention_persists_without_starting_response_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let research_id =
        common::create_agent_with_preset_key(&app, "research", "Research Agent", &cookie, &csrf)
            .await;
    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM Agent", &cookie, &csrf).await;

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &research_id, &cookie, &csrf).await;

    let runs = common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "research attention mention completes",
        Duration::from_secs(30),
        |runs| {
            runs.iter().any(|run| {
                run["agentId"].as_str() == Some(research_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
                    && run["status"].as_str() == Some("succeeded")
            })
        },
    )
    .await;

    assert_eq!(
        runs.iter()
            .filter(|run| {
                run["agentId"].as_str() == Some(pm_id.as_str())
                    && run["jobType"].as_str() == Some("respond_to_mention")
            })
            .count(),
        0
    );
    let ticket_uuid = uuid::Uuid::parse_str(&ticket_id).expect("ticket UUID");
    let mentions = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid)>(
        r#"
        SELECT tm.comment_id, tm.mentioned_agent_id
        FROM ticket_mentions tm
        JOIN ticket_comments tc ON tc.id = tm.comment_id
        WHERE tm.ticket_id = $1 AND tc.author_type = 'agent'
        "#,
    )
    .bind(ticket_uuid)
    .fetch_all(pool)
    .await
    .expect("load agent-authored mentions");
    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0].1.to_string(), pm_id);

    let notification_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM notifications
        WHERE ticket_id = $1 AND agent_id = $2 AND type = 'agent_mentioned'
        "#,
    )
    .bind(ticket_uuid)
    .bind(Uuid::parse_str(&pm_id).expect("PM UUID"))
    .fetch_one(pool)
    .await
    .expect("count attention notifications");
    assert_eq!(notification_count, 1);

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "in_review");
    assert_eq!(ticket["assigneeAgentId"], research_id);
    assert!(ticket["substatus"].is_null());
}

#[tokio::test]
async fn successful_work_consultation_runs_once_and_response_cannot_chain() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_worker_count(2).await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let frontend_id = common::create_agent_with_preset_key(
        &app,
        "frontend_engineer",
        "Frontend Engineer",
        &cookie,
        &csrf,
    )
    .await;
    let dba_id =
        common::create_agent_with_preset_key(&app, "dba", "DBA Agent", &cookie, &csrf).await;

    sqlx::query("DROP TRIGGER IF EXISTS test_followup_after_source_terminal ON agent_runs")
        .execute(pool)
        .await
        .expect("drop stale ordering trigger");
    sqlx::query("DROP FUNCTION IF EXISTS test_followup_after_source_terminal()")
        .execute(pool)
        .await
        .expect("drop stale ordering function");
    sqlx::query(
        r#"
        CREATE FUNCTION test_followup_after_source_terminal()
        RETURNS trigger
        LANGUAGE plpgsql
        AS $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM agent_runs
                WHERE ticket_id = NEW.ticket_id
                  AND agent_id = TG_ARGV[0]::uuid
                  AND status IN ('queued', 'running')
            ) THEN
                RAISE EXCEPTION 'follow-up exposed before source run became terminal';
            END IF;
            RETURN NEW;
        END;
        $$
        "#,
    )
    .execute(pool)
    .await
    .expect("create ordering assertion function");
    sqlx::query(&format!(
        r#"
        CREATE TRIGGER test_followup_after_source_terminal
        BEFORE INSERT ON agent_runs
        FOR EACH ROW
        WHEN (
            NEW.agent_id = '{dba_id}'::uuid
            AND NEW.job_type = 'respond_to_mention'
        )
        EXECUTE FUNCTION test_followup_after_source_terminal('{frontend_id}')
        "#,
    ))
    .execute(pool)
    .await
    .expect("create ordering assertion trigger");

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &frontend_id, &cookie, &csrf).await;

    let runs = common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "frontend consultation receives DBA response",
        Duration::from_secs(30),
        |runs| {
            runs.iter().any(|run| {
                run["agentId"].as_str() == Some(frontend_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
                    && run["status"].as_str() == Some("succeeded")
            }) && runs.iter().any(|run| {
                run["agentId"].as_str() == Some(dba_id.as_str())
                    && run["jobType"].as_str() == Some("respond_to_mention")
                    && run["status"].as_str() == Some("succeeded")
            })
        },
    )
    .await;

    sqlx::query("DROP TRIGGER test_followup_after_source_terminal ON agent_runs")
        .execute(pool)
        .await
        .expect("drop ordering trigger");
    sqlx::query("DROP FUNCTION test_followup_after_source_terminal()")
        .execute(pool)
        .await
        .expect("drop ordering function");

    assert_eq!(
        runs.iter()
            .filter(|run| {
                run["agentId"].as_str() == Some(frontend_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
            })
            .count(),
        1
    );
    assert_eq!(
        runs.iter()
            .filter(|run| {
                run["agentId"].as_str() == Some(dba_id.as_str())
                    && run["jobType"].as_str() == Some("respond_to_mention")
            })
            .count(),
        1
    );
    assert_eq!(
        runs.iter()
            .filter(|run| {
                run["agentId"].as_str() == Some(frontend_id.as_str())
                    && run["jobType"].as_str() == Some("respond_to_mention")
            })
            .count(),
        0
    );

    let dba_response = runs
        .iter()
        .find(|run| {
            run["agentId"].as_str() == Some(dba_id.as_str())
                && run["jobType"].as_str() == Some("respond_to_mention")
        })
        .expect("DBA response run");
    let worktree_path = dba_response["worktreePath"]
        .as_str()
        .expect("response worktree path");
    let context = std::fs::read_to_string(
        std::path::Path::new(worktree_path)
            .join(".agent")
            .join("context.md"),
    )
    .expect("read consultation context");
    let exact_request = "Verify the data assumptions used by the frontend implementation.";
    let request_pos = context
        .find(exact_request)
        .expect("exact request in context");
    let ticket_pos = context.find("# Ticket context").expect("ticket context");
    assert!(request_pos < ticket_pos);
    assert!(context.contains("Coppice platform rules — consultation response"));
    assert!(context.contains("Do not edit"));
    assert!(context.contains("Do not commit"));
    assert!(context.contains("Do not take assignment"));

    let ticket_uuid = uuid::Uuid::parse_str(&ticket_id).expect("ticket UUID");
    let handled_mentions = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM ticket_mentions WHERE ticket_id = $1 AND status = 'handled'",
    )
    .bind(ticket_uuid)
    .fetch_one(pool)
    .await
    .expect("count handled chained mentions");
    assert_eq!(handled_mentions, 2);
}

#[tokio::test]
async fn pending_pm_assignment_wins_over_same_target_consultation_request() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let pm_id =
        common::create_agent_with_preset_key(&app, "pm", "Consulting PM Agent", &cookie, &csrf)
            .await;
    let tech_lead_id = common::create_agent_with_preset_key(
        &app,
        "tech_lead",
        "Pending Tech Lead",
        &cookie,
        &csrf,
    )
    .await;

    sqlx::query("UPDATE agents SET preset_source = 'pm_consult_tech_lead' WHERE id = $1")
        .bind(Uuid::parse_str(&pm_id).expect("PM UUID"))
        .execute(pool)
        .await
        .expect("select PM regression fixture");

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &pm_id, &cookie, &csrf).await;

    let ticket = common::poll_ticket_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "PM recommendation awaits human Tech Lead assignment",
        Duration::from_secs(30),
        |ticket| {
            ticket["status"].as_str() == Some("ready")
                && ticket["pendingAssignRecommendation"]["recommendedAgentKey"].as_str()
                    == Some("tech_lead")
        },
    )
    .await;
    assert_eq!(ticket["assigneeAgentId"], pm_id);

    common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "PM recommendation run finishes",
        Duration::from_secs(30),
        |runs| {
            runs.iter().any(|run| {
                run["agentId"].as_str() == Some(pm_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
                    && run["status"].as_str() == Some("succeeded")
            })
        },
    )
    .await;

    let ticket_uuid = Uuid::parse_str(&ticket_id).expect("ticket UUID");
    let tech_lead_uuid = Uuid::parse_str(&tech_lead_id).expect("Tech Lead UUID");
    let mention_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM ticket_mentions WHERE ticket_id = $1 AND mentioned_agent_id = $2",
    )
    .bind(ticket_uuid)
    .bind(tech_lead_uuid)
    .fetch_one(pool)
    .await
    .expect("count Tech Lead request mentions");
    assert_eq!(mention_count, 1);

    let premature_run_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM agent_runs
        WHERE ticket_id = $1 AND agent_id = $2
          AND job_type IN ('respond_to_mention', 'work_on_ticket')
        "#,
    )
    .bind(ticket_uuid)
    .bind(tech_lead_uuid)
    .fetch_one(pool)
    .await
    .expect("count premature Tech Lead runs");
    assert_eq!(premature_run_count, 0);

    let request_notification_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM notifications
        WHERE ticket_id = $1 AND agent_id = $2 AND type = 'agent_mentioned'
        "#,
    )
    .bind(ticket_uuid)
    .bind(tech_lead_uuid)
    .fetch_one(pool)
    .await
    .expect("count Tech Lead request notifications");
    assert_eq!(request_notification_count, 1);
}

#[tokio::test]
async fn stopping_live_run_defers_consultation_until_worker_exit_with_two_workers() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_worker_count(2).await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let source_id =
        common::create_agent_with_preset_key(&app, "pm", "Source Agent", &cookie, &csrf).await;
    let target_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Target Agent",
        &cookie,
        &csrf,
    )
    .await;

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let ticket_id = Uuid::parse_str(&ticket_id).expect("ticket UUID");
    let source_id = Uuid::parse_str(&source_id).expect("source agent UUID");
    let target_id = Uuid::parse_str(&target_id).expect("target agent UUID");

    let mut comment_body = "Please respond".to_string();
    append_agent_requests_to_comment(
        &mut comment_body,
        &[AgentRequest {
            agent_key: "backend_engineer".into(),
            intent: "consult".into(),
            request: "Please respond".into(),
        }],
    );
    let comment = CommentService::new(pool)
        .create(
            ticket_id,
            AuthorType::Agent,
            Some(source_id),
            &comment_body,
            CommentIntent::ProgressUpdate,
            &[],
            &[],
        )
        .await
        .expect("create source comment");
    let mentions = MentionService::new(pool)
        .create_mentions(
            ticket_id,
            comment.id,
            &["backend_engineer".to_string()],
            None,
            Uuid::parse_str(&project_id).expect("project UUID"),
        )
        .await
        .expect("create pending mention");
    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0].mentioned_agent_id, target_id);

    let active_run_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
            context_profile, started_at
        )
        VALUES ($1, $2, $3, 'work_on_ticket', 'running', $4, 'full', now())
        "#,
    )
    .bind(active_run_id)
    .bind(ticket_id)
    .bind(target_id)
    .bind(PROFILE_ID)
    .execute(pool)
    .await
    .expect("insert live target run");

    let live_handle = state.run_streams.register(active_run_id);
    let cancel_rx = live_handle.cancelled_rx();
    let stop = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/agent-runs/{active_run_id}/stop"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .expect("stop live run");
    assert_eq!(stop.status(), StatusCode::OK);
    assert!(
        *cancel_rx.borrow(),
        "live provider should receive cancellation"
    );

    let responses_before_exit = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM agent_runs WHERE ticket_id = $1 AND agent_id = $2 AND job_type = 'respond_to_mention'",
    )
    .bind(ticket_id)
    .bind(target_id)
    .fetch_one(pool)
    .await
    .expect("count responses before worker exit");
    assert_eq!(responses_before_exit, 0);

    state.run_streams.remove(active_run_id);
    let cancelled_run = RunService::new(pool)
        .get(active_run_id)
        .await
        .expect("load cancelled run");
    RunOrchestrator::new(pool, &state.config.workflow)
        .handle_terminal_run(&cancelled_run)
        .await;

    let responses_after_exit = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM agent_runs WHERE ticket_id = $1 AND agent_id = $2 AND job_type = 'respond_to_mention'",
    )
    .bind(ticket_id)
    .bind(target_id)
    .fetch_one(pool)
    .await
    .expect("count responses after worker exit");
    assert_eq!(responses_after_exit, 1);
}
