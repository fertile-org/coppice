mod common;

use axum::http::StatusCode;
use coppice_config::AutoSplitConfig;
use coppice_server::domain::workflow::SplitTicketSpec;
use coppice_server::services::split_service::SplitService;
use coppice_server::services::ticket_service::TicketService;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn approve_splits_creates_children() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let pm_id =
        common::create_agent_with_preset_key(&app, "pm", "PM Agent", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let ticket_uuid = Uuid::parse_str(&ticket_id).expect("ticket uuid");
    let pm_uuid = Uuid::parse_str(&pm_id).expect("pm uuid");

    let parent = TicketService::new(pool)
        .get(ticket_uuid)
        .await
        .expect("load parent");

    let splits = vec![
        SplitTicketSpec {
            title: "Child A".into(),
            description: "First deliverable".into(),
            acceptance_criteria: None,
            assign_to: None,
        },
        SplitTicketSpec {
            title: "Child B".into(),
            description: "Second deliverable".into(),
            acceptance_criteria: None,
            assign_to: None,
        },
    ];

    SplitService::new(pool, &state.config.workflow)
        .apply_splits(&parent.ticket, &splits, pm_uuid, false)
        .await
        .expect("set pending splits");

    let approve = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/approve-splits"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);

    let created: serde_json::Value = common::json_body(approve).await;
    let created_items = created.as_array().expect("children array");
    assert_eq!(created_items.len(), 2);
    assert_eq!(created_items[0]["title"], "Child A");
    assert_eq!(created_items[1]["title"], "Child B");

    let list = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/children"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);

    let children: serde_json::Value = common::json_body(list).await;
    let child_items = children.as_array().expect("children list");
    assert_eq!(child_items.len(), 2);

    let pending: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT pending_split_recommendation FROM tickets WHERE id = $1",
    )
    .bind(ticket_uuid)
    .fetch_one(pool)
    .await
    .expect("load pending");
    assert!(pending.is_none());
}

#[tokio::test]
async fn auto_split_creates_children_on_pm_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) = common::bootstrap_and_login_with_state_and_workers(
        "pm/split_auto",
        |config| {
            config.workflow.auto_split = AutoSplitConfig {
                default: true,
                ..Default::default()
            };
        },
    )
    .await;
    let pool = state.db.as_ref().expect("db pool");

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let pm_id =
        common::create_agent_with_preset_key(&app, "pm", "PM Agent", &cookie, &csrf).await;
    let _engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let ticket_uuid = Uuid::parse_str(&ticket_id).expect("ticket uuid");
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &pm_id, &cookie, &csrf).await;

    let run_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/run-agent"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(run_res.status(), StatusCode::CREATED);

    common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "PM split run succeeded",
        Duration::from_secs(15),
        |runs| {
            runs.iter().any(|run| {
                run["jobType"].as_str() == Some("work_on_ticket")
                    && run["status"].as_str() == Some("succeeded")
            })
        },
    )
    .await;

    let list = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/children"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);

    let children: serde_json::Value = common::json_body(list).await;
    let child_items = children.as_array().expect("children list");
    assert_eq!(child_items.len(), 2);
    assert_eq!(child_items[0]["title"], "Add retry logic to API client");
    assert_eq!(child_items[1]["title"], "Add circuit breaker dashboard");
    for child in child_items {
        assert_eq!(child["parentTicketId"].as_str().unwrap(), ticket_id);
        assert_eq!(child["status"], "backlog");
    }

    let pending: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT pending_split_recommendation FROM tickets WHERE id = $1",
    )
    .bind(ticket_uuid)
    .fetch_one(pool)
    .await
    .expect("load pending split");
    assert!(pending.is_none());

    // Default auto_assign has backlog=false: assignTo becomes a recommendation, not assignment.
    let retry_child = &child_items[0];
    assert!(retry_child["assigneeAgentId"].is_null());
    assert_eq!(
        retry_child["pendingAssignRecommendation"]["recommendedAgentKey"],
        "backend_engineer"
    );

    let dashboard_child = &child_items[1];
    assert!(dashboard_child["assigneeAgentId"].is_null());
    assert!(dashboard_child["pendingAssignRecommendation"].is_null());

}
