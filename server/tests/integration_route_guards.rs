//! Table-driven admin / CSRF / unauthenticated guards for admin-only routes.
//! Domain behavior stays in the per-area integration files; auth middleware
//! itself stays pinned in `integration_auth::logout_without_csrf_is_forbidden`.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

#[derive(Clone, Copy)]
enum Auth {
    /// No session cookie.
    None,
    /// Member session with a valid CSRF token.
    Member,
    /// Admin session, CSRF omitted on a mutating method.
    AdminNoCsrf,
}

struct Case {
    name: &'static str,
    method: &'static str,
    uri: &'static str,
    body: &'static str,
    auth: Auth,
    expected: StatusCode,
}

#[tokio::test]
async fn admin_and_csrf_route_guards() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        eprintln!("skipping: postgres not available");
        return;
    }

    let (_state, app, admin_cookie, admin_csrf) = common::bootstrap_and_login_with_state().await;

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

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"email":"member@localhost","password":"secret123"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let set_cookie = login
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    let token = coppice_server::middleware::session::parse_session_cookie(set_cookie).unwrap();
    let member_body = common::json_body(login).await;
    let member_cookie = format!("coppice_session={token}");
    let member_csrf = member_body["csrfToken"].as_str().unwrap().to_string();

    let unknown = uuid::Uuid::new_v4();
    let cases = [
        Case {
            name: "users_create_as_member",
            method: "POST",
            uri: "/api/users",
            body: r#"{"email":"other@localhost","password":"secret456"}"#,
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "connectors_list_unauthenticated",
            method: "GET",
            uri: "/api/tools/connectors",
            body: "",
            auth: Auth::None,
            expected: StatusCode::UNAUTHORIZED,
        },
        Case {
            name: "connectors_list_as_member",
            method: "GET",
            uri: "/api/tools/connectors",
            body: "",
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "connector_check_as_member",
            method: "POST",
            uri: "/api/tools/connectors/kilo-code/check",
            body: "",
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "connector_check_without_csrf",
            method: "POST",
            uri: "/api/tools/connectors/kilo-code/check",
            body: "",
            auth: Auth::AdminNoCsrf,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "plugins_rescan_as_member",
            method: "POST",
            uri: "/api/plugins/rescan",
            body: "null",
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "plugins_rescan_without_csrf",
            method: "POST",
            uri: "/api/plugins/rescan",
            body: "null",
            auth: Auth::AdminNoCsrf,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "marketplace_source_as_member",
            method: "POST",
            uri: "/api/marketplace/sources",
            body: r#"{"name":"acme","gitUrl":"https://example.com/acme.git"}"#,
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "marketplace_source_without_csrf",
            method: "POST",
            uri: "/api/marketplace/sources",
            body: r#"{"name":"acme","gitUrl":"https://example.com/acme.git"}"#,
            auth: Auth::AdminNoCsrf,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "marketplace_plugin_update_as_member",
            method: "POST",
            uri: "", // filled below with unknown id
            body: "null",
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "logs_unauthenticated",
            method: "GET",
            uri: "/api/logs",
            body: "",
            auth: Auth::None,
            expected: StatusCode::UNAUTHORIZED,
        },
        Case {
            name: "logs_as_member",
            method: "GET",
            uri: "/api/logs",
            body: "",
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "notification_read_without_csrf",
            method: "POST",
            uri: "", // filled below
            body: "",
            auth: Auth::AdminNoCsrf,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "plugin_builds_list_as_member",
            method: "GET",
            uri: "/api/plugin-builds",
            body: "",
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "plugin_builds_create_as_member",
            method: "POST",
            uri: "/api/plugin-builds",
            body: "null",
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "knowledge_settings_as_member",
            method: "PUT",
            uri: "/api/settings/knowledge",
            body: r#"{"compactionAgentId":null}"#,
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "knowledge_settings_without_csrf",
            method: "PUT",
            uri: "/api/settings/knowledge",
            body: r#"{"compactionAgentId":null}"#,
            auth: Auth::AdminNoCsrf,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "plugin_settings_as_member",
            method: "PUT",
            uri: "", // filled below
            body: r#"{"values":{"API_TOKEN":"tok"}}"#,
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "plugin_test_as_member",
            method: "POST",
            uri: "", // filled below
            body: "null",
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "plugin_skill_as_member",
            method: "PUT",
            uri: "", // filled below
            body: r#"{"enabled":false}"#,
            auth: Auth::Member,
            expected: StatusCode::FORBIDDEN,
        },
        Case {
            name: "plugin_skill_without_csrf",
            method: "PUT",
            uri: "", // filled below
            body: r#"{"enabled":false}"#,
            auth: Auth::AdminNoCsrf,
            expected: StatusCode::FORBIDDEN,
        },
    ];

    let marketplace_update = format!("/api/marketplace/plugins/{unknown}/update");
    let notification_read = format!("/api/notifications/{unknown}/read");
    let plugin_settings = format!("/api/plugins/{unknown}/settings");
    let plugin_test = format!("/api/plugins/{unknown}/test");
    let plugin_skill = format!("/api/plugins/{unknown}/skills/brainstorming");

    for case in &cases {
        let uri = match case.name {
            "marketplace_plugin_update_as_member" => marketplace_update.as_str(),
            "notification_read_without_csrf" => notification_read.as_str(),
            "plugin_settings_as_member" => plugin_settings.as_str(),
            "plugin_test_as_member" => plugin_test.as_str(),
            "plugin_skill_as_member" | "plugin_skill_without_csrf" => plugin_skill.as_str(),
            _ => case.uri,
        };

        let mut builder = Request::builder()
            .method(case.method)
            .uri(uri)
            .header("content-type", "application/json");

        match case.auth {
            Auth::None => {}
            Auth::Member => {
                builder = builder.header(header::COOKIE, &member_cookie);
                if case.method != "GET" {
                    builder = builder.header("x-csrf-token", &member_csrf);
                }
            }
            Auth::AdminNoCsrf => {
                builder = builder.header(header::COOKIE, &admin_cookie);
            }
        }

        let res = app
            .clone()
            .oneshot(builder.body(Body::from(case.body.to_string())).unwrap())
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            case.expected,
            "{} {} {}",
            case.name,
            case.method,
            uri
        );
    }
}
