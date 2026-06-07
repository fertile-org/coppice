use coppice_server::{db, AppConfig, AppState};
use std::sync::Arc;
use tower::ServiceExt;

async fn db_available() -> bool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://coppice:coppice@localhost:5432/coppice".into());
    db::connect_and_migrate(&database_url).await.is_ok()
}

async fn test_state_with_db() -> Arc<AppState> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://coppice:coppice@localhost:5432/coppice".into());
    let pool = db::connect_and_migrate(&database_url)
        .await
        .expect("connect to test database");
    sqlx::query("TRUNCATE sessions, users RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await
        .expect("truncate auth tables");

    let config = AppConfig::load(None).expect("test config");
    Arc::new(AppState {
        config,
        db: Some(pool),
    })
}

fn bootstrap_password_header() -> (&'static str, &'static str) {
    ("x-bootstrap-password", "changeme")
}

#[tokio::test]
async fn bootstrap_and_login_flow() {
    if !db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let state = test_state_with_db().await;
    let app = coppice_server::app(state);

    let bootstrap = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/bootstrap")
                .header("content-type", "application/json")
                .header(bootstrap_password_header().0, bootstrap_password_header().1)
                .body(axum::body::Body::from(
                    r#"{"email":"admin@localhost","password":"changeme"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), axum::http::StatusCode::OK);

    let login = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"email":"admin@localhost","password":"changeme"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), axum::http::StatusCode::OK);

    let set_cookie = login
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .expect("session cookie");
    let cookie = set_cookie.to_str().unwrap();
    assert!(cookie.contains("coppice_session="));
    assert!(cookie.contains("HttpOnly"));

    let body = axum::body::to_bytes(login.into_body(), usize::MAX)
        .await
        .unwrap();
    let login_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(login_json["csrfToken"].as_str().is_some());
}
