mod common;

use axum::http::StatusCode;
use coppice_server::middleware::session::parse_session_cookie;
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn login_as(app: &axum::Router, email: &str, password: &str) -> (String, String) {
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
    let csrf_token = login_json["csrfToken"]
        .as_str()
        .expect("csrf token")
        .to_string();

    (cookie, csrf_token)
}

#[tokio::test]
async fn admin_can_create_user() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/users",
            r#"{"email":"member@localhost","password":"secret123"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let body: serde_json::Value = common::json_body(res).await;
    assert_eq!(body["email"], "member@localhost");
    assert_eq!(body["role"], "member");
    assert!(body["id"].is_string());
    assert!(body["createdAt"].is_string());
}

#[tokio::test]
async fn member_cannot_create_user() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let (app, admin_cookie, admin_csrf) = common::bootstrap_and_login().await;

    let create = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/users",
            r#"{"email":"member@localhost","password":"secret123"}"#,
            &admin_cookie,
            &admin_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);

    let (member_cookie, member_csrf) =
        login_as(&app, "member@localhost", "secret123").await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/users",
            r#"{"email":"other@localhost","password":"secret456"}"#,
            &member_cookie,
            &member_csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
