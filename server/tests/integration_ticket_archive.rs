mod common;

use axum::http::StatusCode;
use tower::ServiceExt;
use uuid::Uuid;

async fn create_ticket(
    app: &axum::Router,
    board_id: &str,
    cookie: &str,
    csrf: &str,
) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/boards/{board_id}/tickets"),
            r#"{"title":"Archive me","description":"soft archive"}"#,
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    common::json_body(res).await
}

#[tokio::test]
async fn archive_and_unarchive_ticket() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    assert!(
        common::db_available().await,
        "embedded test db unavailable"
    );
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket = create_ticket(&app, &board_id, &cookie, &csrf).await;
    let ticket_id = ticket["id"].as_str().unwrap();
    assert!(ticket["archivedAt"].is_null());

    let archive = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/archive"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(archive.status(), StatusCode::OK);
    let archived: serde_json::Value = common::json_body(archive).await;
    assert!(archived["archivedAt"].as_str().is_some());
    assert_eq!(archived["status"], "backlog");

    let again = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/archive"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(again.status(), StatusCode::OK);

    let get = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let got: serde_json::Value = common::json_body(get).await;
    assert!(got["archivedAt"].as_str().is_some());

    let unarchive = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/unarchive"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(unarchive.status(), StatusCode::OK);
    let active: serde_json::Value = common::json_body(unarchive).await;
    assert!(active["archivedAt"].is_null());
    assert_eq!(active["status"], "backlog");
}

#[tokio::test]
async fn default_list_excludes_archived_include_flag_includes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    assert!(
        common::db_available().await,
        "embedded test db unavailable"
    );
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let active = create_ticket(&app, &board_id, &cookie, &csrf).await;
    let to_archive = create_ticket(&app, &board_id, &cookie, &csrf).await;
    let archived_id = to_archive["id"].as_str().unwrap();

    let archive = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{archived_id}/archive"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(archive.status(), StatusCode::OK);

    let default_list = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/boards/{board_id}/tickets"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(default_list.status(), StatusCode::OK);
    let default_tickets: serde_json::Value = common::json_body(default_list).await;
    let default_ids: Vec<&str> = default_tickets
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert!(default_ids.contains(&active["id"].as_str().unwrap()));
    assert!(!default_ids.contains(&archived_id));

    let with_archived = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/boards/{board_id}/tickets?includeArchived=true"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(with_archived.status(), StatusCode::OK);
    let all_tickets: serde_json::Value = common::json_body(with_archived).await;
    let all_ids: Vec<&str> = all_tickets
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert!(all_ids.contains(&active["id"].as_str().unwrap()));
    assert!(all_ids.contains(&archived_id));
}

#[tokio::test]
async fn reject_archive_while_active_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    assert!(
        common::db_available().await,
        "embedded test db unavailable"
    );

    let (app, cookie, csrf, _env) = common::bootstrap_and_login_with_workers("done").await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &agent_id, &cookie, &csrf).await;

    let run = app
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
    assert_eq!(run.status(), StatusCode::CREATED);

    let archive = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/archive"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(archive.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn reject_mutations_while_archived() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    assert!(
        common::db_available().await,
        "embedded test db unavailable"
    );
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket = create_ticket(&app, &board_id, &cookie, &csrf).await;
    let ticket_id = ticket["id"].as_str().unwrap();

    let archive = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/archive"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(archive.status(), StatusCode::OK);

    let status = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::CONFLICT);

    let patch = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}"),
            r#"{"title":"nope"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::CONFLICT);

    let assign = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/assign"),
            r#"{"agentId":null}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(assign.status(), StatusCode::CONFLICT);

    let run = app
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
    assert_eq!(run.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn archive_parent_does_not_archive_children() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    assert!(
        common::db_available().await,
        "embedded test db unavailable"
    );
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let parent = create_ticket(&app, &board_id, &cookie, &csrf).await;
    let parent_id = Uuid::parse_str(parent["id"].as_str().unwrap()).unwrap();

    let parent_ticket = coppice_server::services::ticket_service::TicketService::new(pool)
        .get(parent_id)
        .await
        .expect("load parent");
    let child = coppice_server::services::ticket_service::TicketService::new(pool)
        .create_child(
            &parent_ticket.ticket,
            "Child ticket",
            "independent archive",
            parent_ticket.ticket.created_by_id.unwrap(),
        )
        .await
        .expect("create child");
    let child_id = child.ticket.id.to_string();

    let archive = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{parent_id}/archive"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(archive.status(), StatusCode::OK);

    let child_get = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{child_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(child_get.status(), StatusCode::OK);
    let child_body: serde_json::Value = common::json_body(child_get).await;
    assert!(child_body["archivedAt"].is_null());

    let list = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/boards/{board_id}/tickets"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    let tickets: serde_json::Value = common::json_body(list).await;
    let ids: Vec<&str> = tickets
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert!(!ids.contains(&parent["id"].as_str().unwrap()));
    assert!(ids.contains(&child_id.as_str()));
}
