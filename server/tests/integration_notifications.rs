mod common;

use axum::http::StatusCode;
use coppice_server::services::notification_service::NotificationService;
use sqlx::Row;
use tower::ServiceExt;

async fn login_as(app: &axum::Router, email: &str, password: &str) -> (String, String) {
    use coppice_server::middleware::session::parse_session_cookie;
    use http_body_util::BodyExt;

    let login = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(format!(
                    r#"{{"email":"{email}","password":"{password}"}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);

    let set_cookie = login
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .expect("session cookie");
    let cookie_header = set_cookie.to_str().unwrap();
    let session_token = parse_session_cookie(cookie_header).expect("session token");
    let cookie = format!("coppice_session={session_token}");

    let body = login.into_body().collect().await.unwrap().to_bytes();
    let login_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let csrf_token = login_json["csrfToken"].as_str().expect("csrf token").to_string();

    (cookie, csrf_token)
}

async fn create_member(app: &axum::Router, admin_cookie: &str, admin_csrf: &str, email: &str) {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/users",
            &format!(r#"{{"email":"{email}","password":"secret123"}}"#),
            admin_cookie,
            admin_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
}

async fn get_unread_count(app: &axum::Router, cookie: &str, csrf: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            "/api/notifications/unread-count",
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    common::json_body(res).await
}

async fn list_notifications(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    query: &str,
) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/notifications{query}"),
            "",
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    common::json_body(res).await
}

async fn setup_ticket_with_agent(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
) -> String {
    let project_id = common::create_test_project(app, cookie, csrf).await;
    let ticket_id = common::create_test_ticket(app, &project_id, cookie, csrf).await;
    common::create_agent_with_preset_key(app, "backend_engineer", "Backend Engineer", cookie, csrf).await;
    ticket_id
}

/// Insert a notification row directly for `recipient_user_id` with an explicit
/// `created_at` so ordering tests are deterministic.
async fn insert_notification(
    pool: &sqlx::PgPool,
    recipient_user_id: uuid::Uuid,
    source_key: &str,
    title: &str,
    created_at: &str,
    read: bool,
) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    let created_ts = time::OffsetDateTime::parse(
        created_at,
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO notifications (
            id, recipient_user_id, type, title, source_key, read_at, created_at
        )
        VALUES ($1, $2, 'agent_run_finished', $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(recipient_user_id)
    .bind(title)
    .bind(source_key)
    .bind(if read { Some(time::OffsetDateTime::now_utc()) } else { None })
    .bind(created_ts)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn admin_user_id(pool: &sqlx::PgPool) -> uuid::Uuid {
    let row = sqlx::query("SELECT id FROM users WHERE email = 'admin@localhost'")
        .fetch_one(pool)
        .await
        .unwrap();
    row.get::<uuid::Uuid, _>("id")
}

#[tokio::test]
async fn unauthenticated_request_is_rejected() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, _cookie, _csrf) = common::bootstrap_and_login().await;

    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/notifications")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn mark_read_requires_csrf_token() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, _csrf) = common::bootstrap_and_login().await;

    let id = uuid::Uuid::new_v4();
    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/api/notifications/{id}/read"))
                .header(axum::http::header::COOKIE, &cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn run_finished_creates_notification_and_dedupes() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool").clone();

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "Backend Engineer", &cookie, &csrf).await;

    let run_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (id, ticket_id, agent_id, job_type, status, sandbox_profile_id)
        VALUES ($1, $2, $3, 'work_on_ticket', 'running', 'permissive-default')
        "#,
    )
    .bind(run_id)
    .bind(ticket_id.parse::<uuid::Uuid>().unwrap())
    .bind(agent_id.parse::<uuid::Uuid>().unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let ticket_uuid = ticket_id.parse::<uuid::Uuid>().unwrap();
    let agent_uuid = agent_id.parse::<uuid::Uuid>().unwrap();
    let svc = NotificationService::new(&pool);
    let created = svc
        .create_for_run_finished(run_id, ticket_uuid, agent_uuid, "succeeded")
        .await
        .unwrap();
    assert_eq!(created.len(), 1, "one notification per user");

    // Re-publishing the same finished run must not duplicate.
    let again = svc
        .create_for_run_finished(run_id, ticket_uuid, agent_uuid, "failed")
        .await
        .unwrap();
    assert!(again.is_empty(), "duplicate source event must be a no-op");

    let body = list_notifications(&app, &cookie, &csrf, "").await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["type"], "agent_run_finished");
    assert_eq!(items[0]["runId"].as_str().unwrap(), run_id.to_string());
    assert!(items[0]["title"].as_str().unwrap().contains("succeeded"));
}

#[tokio::test]
async fn mention_creates_notification_for_all_users() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, admin_cookie, admin_csrf) = common::bootstrap_and_login().await;

    // Create a second human user so fan-out hits two recipients.
    create_member(&app, &admin_cookie, &admin_csrf, "member@localhost").await;
    let (member_cookie, member_csrf) = login_as(&app, "member@localhost", "secret123").await;

    let ticket_id = setup_ticket_with_agent(&app, &admin_cookie, &admin_csrf).await;

    // Chat-mode mention does not require a repo and won't start a run, but still
    // creates the mention + notification.
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@backend_engineer can you look?","mentionMode":"chat"}"#,
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let admin_unread = get_unread_count(&app, &admin_cookie, &admin_csrf).await;
    assert_eq!(admin_unread["count"].as_i64().unwrap(), 1);

    let member_unread = get_unread_count(&app, &member_cookie, &member_csrf).await;
    assert_eq!(member_unread["count"].as_i64().unwrap(), 1);

    let admin_list = list_notifications(&app, &admin_cookie, &admin_csrf, "").await;
    assert_eq!(admin_list["items"][0]["type"], "agent_mentioned");
    assert!(admin_list["items"][0]["mentionId"].is_string());
}

#[tokio::test]
async fn list_returns_newest_first_with_cursor_pagination() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool").clone();
    let admin_id = admin_user_id(&pool).await;

    insert_notification(&pool, admin_id, "n1", "oldest", "2026-01-01T00:00:00Z", false).await;
    insert_notification(&pool, admin_id, "n2", "middle", "2026-02-01T00:00:00Z", false).await;
    insert_notification(&pool, admin_id, "n3", "newest", "2026-03-01T00:00:00Z", false).await;

    let page1 = list_notifications(&app, &cookie, &csrf, "?limit=2").await;
    let items = page1["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["title"], "newest");
    assert_eq!(items[1]["title"], "middle");
    assert!(page1["nextCursor"].is_string(), "cursor present when more remain");

    let cursor = page1["nextCursor"].as_str().unwrap();
    let page2 = list_notifications(
        &app,
        &cookie,
        &csrf,
        &format!("?limit=2&cursor={cursor}"),
    )
    .await;
    let items2 = page2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 1);
    assert_eq!(items2[0]["title"], "oldest");
    assert!(page2["nextCursor"].is_null(), "no cursor at end of results");
}

#[tokio::test]
async fn filter_unread_excludes_read_notifications() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool").clone();
    let admin_id = admin_user_id(&pool).await;

    insert_notification(&pool, admin_id, "read-one", "read", "2026-01-01T00:00:00Z", true).await;
    insert_notification(&pool, admin_id, "unread-one", "unread", "2026-02-01T00:00:00Z", false).await;

    let unread = list_notifications(&app, &cookie, &csrf, "?filter=unread").await;
    let titles: Vec<&str> = unread["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["unread"]);

    let all = list_notifications(&app, &cookie, &csrf, "?filter=all").await;
    assert_eq!(all["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn mark_one_read_decrements_unread_count() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool").clone();
    let admin_id = admin_user_id(&pool).await;

    let id_a = insert_notification(&pool, admin_id, "a", "a", "2026-01-01T00:00:00Z", false).await;
    insert_notification(&pool, admin_id, "b", "b", "2026-02-01T00:00:00Z", false).await;

    assert_eq!(get_unread_count(&app, &cookie, &csrf).await["count"].as_i64().unwrap(), 2);

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/notifications/{id_a}/read"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    assert_eq!(get_unread_count(&app, &cookie, &csrf).await["count"].as_i64().unwrap(), 1);

    // Marking an already-read notification is a no-op (idempotent), not an error.
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/notifications/{id_a}/read"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn mark_all_read_clears_unread_for_signed_in_user_only() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, admin_cookie, admin_csrf) = common::bootstrap_and_login().await;
    create_member(&app, &admin_cookie, &admin_csrf, "member@localhost").await;
    let (member_cookie, member_csrf) = login_as(&app, "member@localhost", "secret123").await;

    let ticket_id = setup_ticket_with_agent(&app, &admin_cookie, &admin_csrf).await;
    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@backend_engineer ping","mentionMode":"chat"}"#,
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    assert_eq!(get_unread_count(&app, &admin_cookie, &admin_csrf).await["count"].as_i64().unwrap(), 1);
    assert_eq!(get_unread_count(&app, &member_cookie, &member_csrf).await["count"].as_i64().unwrap(), 1);

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/notifications/mark-all-read",
            "",
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(body["marked"].as_u64().unwrap(), 1);

    assert_eq!(get_unread_count(&app, &admin_cookie, &admin_csrf).await["count"].as_i64().unwrap(), 0);
    // Member's notification is independent and still unread.
    assert_eq!(get_unread_count(&app, &member_cookie, &member_csrf).await["count"].as_i64().unwrap(), 1);
}

#[tokio::test]
async fn cannot_mark_another_users_notification_read() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, admin_cookie, admin_csrf) = common::bootstrap_and_login().await;
    create_member(&app, &admin_cookie, &admin_csrf, "member@localhost").await;
    let (member_cookie, member_csrf) = login_as(&app, "member@localhost", "secret123").await;

    let ticket_id = setup_ticket_with_agent(&app, &admin_cookie, &admin_csrf).await;
    let _ = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@backend_engineer ping","mentionMode":"chat"}"#,
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();

    // Member has their own notification; admin attempts to mark it read by id.
    let member_list = list_notifications(&app, &member_cookie, &member_csrf, "").await;
    let member_notif_id = member_list["items"][0]["id"].as_str().unwrap();

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/notifications/{member_notif_id}/read"),
            "",
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    // Admin is authenticated + CSRF-valid, but the notification is not theirs.
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Member's unread count is unchanged.
    assert_eq!(get_unread_count(&app, &member_cookie, &member_csrf).await["count"].as_i64().unwrap(), 1);
}

#[tokio::test]
async fn mark_read_unknown_id_returns_not_found() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/notifications/{}/read", uuid::Uuid::new_v4()),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stopping_queued_run_creates_cancelled_notification() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let agent_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;
    let run_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO agent_runs (id, ticket_id, agent_id, job_type, status, sandbox_profile_id)
        VALUES ($1, $2, $3, 'work_on_ticket', 'queued', 'permissive-default')
        "#,
    )
    .bind(run_id)
    .bind(ticket_id.parse::<uuid::Uuid>().unwrap())
    .bind(agent_id.parse::<uuid::Uuid>().unwrap())
    .execute(pool)
    .await
    .unwrap();

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(before, 0, "setup should not create notifications");
    let stopped = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/agent-runs/{run_id}/stop"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(stopped.status(), StatusCode::OK);
    let stopped: serde_json::Value = common::json_body(stopped).await;
    assert_eq!(stopped["run"]["status"], "cancelled");

    let unread = get_unread_count(&app, &cookie, &csrf).await;
    assert_eq!(unread["count"].as_i64().unwrap(), 1);
    let listed = list_notifications(&app, &cookie, &csrf, "?filter=all").await;
    assert_eq!(listed["items"][0]["type"], "agent_run_finished");
    assert!(listed["items"][0]["title"]
        .as_str()
        .unwrap()
        .contains("cancelled"));
}
