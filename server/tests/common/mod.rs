use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use coppice_server::middleware::session::parse_session_cookie;
use coppice_server::{db, AppConfig, AppState};
use http_body_util::BodyExt;
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;
use tower::ServiceExt;

pub static DB_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub async fn db_available() -> bool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://coppice:coppice@localhost:5432/coppice".into());
    db::connect_and_migrate(&database_url).await.is_ok()
}

pub async fn truncate_workspace(pool: &sqlx::PgPool) {
    sqlx::query(
        r#"
        TRUNCATE
            attachments,
            ticket_comments,
            tickets,
            repos,
            agents,
            projects,
            sessions,
            users
        RESTART IDENTITY CASCADE
        "#,
    )
    .execute(pool)
    .await
    .expect("truncate workspace tables");
}

async fn test_state_with_db() -> Arc<AppState> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://coppice:coppice@localhost:5432/coppice".into());
    let pool = db::connect_and_migrate(&database_url)
        .await
        .expect("connect to test database");
    truncate_workspace(&pool).await;

    let config = AppConfig::load(None).expect("test config");
    Arc::new(AppState {
        config,
        db: Some(pool),
    })
}

fn bootstrap_password_header() -> (&'static str, &'static str) {
    ("x-bootstrap-password", "changeme")
}

pub async fn bootstrap_and_login() -> (Router, String, String) {
    let state = test_state_with_db().await;
    let app = coppice_server::app(state);

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/bootstrap")
                .header("content-type", "application/json")
                .header(bootstrap_password_header().0, bootstrap_password_header().1)
                .body(Body::from(
                    r#"{"email":"admin@localhost","password":"changeme"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"email":"admin@localhost","password":"changeme"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);

    let set_cookie = login
        .headers()
        .get(header::SET_COOKIE)
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

    (app, cookie, csrf_token)
}

pub fn json_request(
    method: &str,
    uri: &str,
    body: &str,
    cookie: &str,
    csrf: &str,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header(header::COOKIE, cookie);

    if method != "GET" {
        builder = builder.header("x-csrf-token", csrf);
    }

    builder.body(Body::from(body.to_string())).unwrap()
}

pub async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

pub async fn create_test_project(app: &Router, cookie: &str, csrf: &str) -> String {
    let res = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/projects",
            r#"{"name":"Test Project"}"#,
            cookie,
            csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = json_body(res).await;
    body["id"].as_str().unwrap().to_string()
}
