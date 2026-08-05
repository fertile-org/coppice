mod common;

use coppice_server::middleware::session::parse_session_cookie;
use coppice_server::{AppConfig, AppState};
use std::sync::Arc;
use tower::ServiceExt;

async fn test_state_with_db() -> Arc<AppState> {
    let pool = common::prepare_test_pool_for_auth().await;

    std::env::set_var(
        "COPPICE_STORAGE__ARTIFACTS_DIR",
        "/tmp/coppice-test-artifacts",
    );
    let config = AppConfig::load_defaults().expect("test config");
    Arc::new(AppState {
        attachments: AppState::attachment_store_from_config(&config),
        connector_registry: AppState::connector_registry_from_config(&config, None),
        agent_health: Arc::new(coppice_server::services::agent_health::AgentHealthRegistry::new()),
        run_streams: Arc::new(coppice_server::sessions::run_registry::RunStreamRegistry::new()),
        event_bus: Arc::new(coppice_server::events::bus::EventBus::new()),
        opencode_serve: None,
        agent_templates: coppice_server::AppState::load_agent_templates(),
        config,
        db: Some(pool),
    })
}

fn bootstrap_password_header() -> (&'static str, &'static str) {
    ("x-bootstrap-password", "changeme")
}

#[tokio::test]
async fn bootstrap_login_me_logout_flow() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
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
        .clone()
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
    let session_token = parse_session_cookie(cookie).expect("session token");

    let body = axum::body::to_bytes(login.into_body(), usize::MAX)
        .await
        .unwrap();
    let login_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let csrf_token = login_json["csrfToken"]
        .as_str()
        .expect("csrf token")
        .to_string();

    let me = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/auth/me")
                .header(
                    axum::http::header::COOKIE,
                    format!("coppice_session={session_token}"),
                )
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me.status(), axum::http::StatusCode::OK);

    let me_body = axum::body::to_bytes(me.into_body(), usize::MAX)
        .await
        .unwrap();
    let me_json: serde_json::Value = serde_json::from_slice(&me_body).unwrap();
    assert_eq!(me_json["user"]["email"], "admin@localhost");
    assert_eq!(
        me_json["csrfToken"].as_str(),
        Some(csrf_token.as_str()),
        "session restore should return the same CSRF token as login"
    );

    let logout = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header(
                    axum::http::header::COOKIE,
                    format!("coppice_session={session_token}"),
                )
                .header("x-csrf-token", csrf_token)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), axum::http::StatusCode::NO_CONTENT);

    let me_after = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/auth/me")
                .header(
                    axum::http::header::COOKIE,
                    format!("coppice_session={session_token}"),
                )
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me_after.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auto_bootstrap_creates_admin_when_configured() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let pool = common::prepare_test_pool_for_auth().await;
    std::env::set_var(
        "COPPICE_STORAGE__ARTIFACTS_DIR",
        "/tmp/coppice-test-artifacts",
    );
    let mut config = AppConfig::load_defaults().expect("test config");
    config.auth.bootstrap_admin_email = Some("seeded@localhost".into());
    config.auth.bootstrap_admin_password = Some("seed-secret".into());

    let auth = coppice_server::services::auth_service::AuthService::new(&pool, &config.auth);
    let created = auth
        .maybe_auto_bootstrap(&config.auth)
        .await
        .expect("auto-bootstrap");
    assert_eq!(
        created.as_ref().map(|u| u.email.as_str()),
        Some("seeded@localhost")
    );

    let again = auth
        .maybe_auto_bootstrap(&config.auth)
        .await
        .expect("second auto-bootstrap");
    assert!(again.is_none());

    let login = auth
        .login("seeded@localhost", "seed-secret")
        .await
        .expect("login seeded admin");
    assert_eq!(login.user.email, "seeded@localhost");
}

#[tokio::test]
async fn auto_bootstrap_skips_when_not_configured() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let pool = common::prepare_test_pool_for_auth().await;
    std::env::set_var(
        "COPPICE_STORAGE__ARTIFACTS_DIR",
        "/tmp/coppice-test-artifacts",
    );
    let config = AppConfig::load_defaults().expect("test config");
    assert!(config.auth.bootstrap_admin_email.is_none());
    assert!(config.auth.bootstrap_admin_password.is_none());

    let auth = coppice_server::services::auth_service::AuthService::new(&pool, &config.auth);
    let created = auth
        .maybe_auto_bootstrap(&config.auth)
        .await
        .expect("auto-bootstrap");
    assert!(created.is_none());

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .expect("count users");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn logout_without_csrf_is_forbidden() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let state = test_state_with_db().await;
    let app = coppice_server::app(state);

    app.clone()
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

    let login = app
        .clone()
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

    let set_cookie = login
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .expect("session cookie");
    let cookie = set_cookie.to_str().unwrap();
    let session_token = parse_session_cookie(cookie).expect("session token");

    let logout = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header(
                    axum::http::header::COOKIE,
                    format!("coppice_session={session_token}"),
                )
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn me_without_session_is_unauthorized() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }
    let state = test_state_with_db().await;
    let app = coppice_server::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/auth/me")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}
