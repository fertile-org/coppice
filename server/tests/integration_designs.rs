mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use coppice_server::copy::plan::{
    DESIGN_SUPERSEDED, NO_PM, PLAN_ALREADY_RUNNING, PLAN_DESIGN_FINAL,
};
use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::mcp::token::{NewRunToolScope, TokenService};
use coppice_server::services::design_service::{
    CreateScope, DesignError, DesignOption, DesignService,
};
use coppice_server::services::planning_service::PlanningService;
use coppice_server::services::run_service::RunService;
use coppice_server::services::ticket_service::TicketService;
use coppice_server::AppState;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

macro_rules! require_db {
    () => {
        if !common::db_available().await {
            eprintln!("skipping: postgres not available");
            return;
        }
    };
}

fn files_with_marker(root: &Path, marker: &[u8], out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            files_with_marker(&path, marker, out);
        } else if kind.is_file() {
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            if bytes.windows(marker.len()).any(|window| window == marker) {
                out.push(path);
            }
        }
    }
}

#[tokio::test]
async fn create_write_publish_choose_roundtrip() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;

    let pool = state.db.clone().expect("db pool");
    let ticket_uuid: Uuid = ticket_id.parse().unwrap();
    let agent_uuid: Uuid = agent_id.parse().unwrap();
    let run = RunService::new(&pool)
        .start_run_for_agent(
            ticket_uuid,
            agent_uuid,
            "work_on_ticket",
            Default::default(),
        )
        .await
        .expect("start run");

    let artifacts = PathBuf::from(&state.config.storage.artifacts_dir);
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("user");

    let svc = DesignService::new(&pool, &artifacts);
    let created = svc
        .create(
            "Home",
            CreateScope {
                ticket_id: Some(ticket_uuid),
                chat_session_id: None,
                run_id: run.id,
                agent_id: agent_uuid,
            },
        )
        .await
        .expect("create");

    let set_dir = artifacts.join(format!("designs/{}", created.id));
    assert!(set_dir.is_dir(), "dir on disk is designs/{{set_id}}");

    let stamp = Uuid::new_v4();
    let html_a =
        format!("<!doctype html><title>A</title>design-preview-marker-a-{stamp}").into_bytes();
    let html_b =
        format!("<!doctype html><title>B</title>design-preview-marker-b-{stamp}").into_bytes();
    svc.write(created.id, run.id, "a/index.html", &html_a)
        .await
        .expect("write a");
    let wrong_run = svc
        .write(created.id, Uuid::new_v4(), "a/index.html", b"hijack")
        .await
        .unwrap_err();
    assert!(
        matches!(wrong_run, DesignError::Invalid(ref msg) if msg.contains("run")),
        "{wrong_run}"
    );
    assert_eq!(fs::read(set_dir.join("a/index.html")).unwrap(), html_a);
    svc.write(created.id, run.id, "b/index.html", &html_b)
        .await
        .expect("write b");
    assert_eq!(fs::read(set_dir.join("a/index.html")).unwrap(), html_a);
    assert_eq!(fs::read(set_dir.join("b/index.html")).unwrap(), html_b);

    let published = svc
        .publish(
            created.id,
            run.id,
            vec![
                DesignOption {
                    id: "a".into(),
                    title: "A".into(),
                    entry: "a/index.html".into(),
                },
                DesignOption {
                    id: "b".into(),
                    title: "B".into(),
                    entry: "b/index.html".into(),
                },
            ],
            "a",
        )
        .await
        .expect("publish");
    assert_eq!(published, created.id);

    svc.choose(created.id, "a", user_id).await.expect("choose");

    let listed = svc.list_for_ticket(ticket_uuid).await.expect("list");
    assert_eq!(listed[0].id, created.id, "newest first");
    let chosen = listed[0].chosen.as_ref().expect("chosen");
    assert_eq!(chosen.id, "a");
    assert_eq!(chosen.title, "A");

    let set_root = set_dir.canonicalize().unwrap();
    for marker in [html_a.as_slice(), html_b.as_slice()] {
        for root in [&artifacts, local_path.as_path()] {
            let mut found = Vec::new();
            files_with_marker(root, marker, &mut found);
            for path in found {
                let canon = path.canonicalize().unwrap();
                assert!(
                    canon.starts_with(&set_root),
                    "file outside set dir: {}",
                    canon.display()
                );
            }
        }
    }
}

#[tokio::test]
async fn design_plugin_starts_enabled_and_stays_off_after_rescan() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.clone().expect("db pool");
    let (enabled, agent_access): (bool, String) =
        sqlx::query_as("SELECT enabled, agent_access FROM plugins WHERE name = 'design'")
            .fetch_one(&pool)
            .await
            .expect("design plugin");
    assert!(enabled);
    assert_eq!(agent_access, "all");

    let id: Uuid = sqlx::query_scalar("SELECT id FROM plugins WHERE name = 'design'")
        .fetch_one(&pool)
        .await
        .expect("design id");
    let patched = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/plugins/{id}"),
            r#"{"enabled":false}"#,
            &cookie,
            &csrf,
        ))
        .await
        .expect("patch");
    assert_eq!(patched.status(), StatusCode::OK);
    let rescanned = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/plugins/rescan",
            "null",
            &cookie,
            &csrf,
        ))
        .await
        .expect("rescan");
    assert_eq!(rescanned.status(), StatusCode::OK);

    let still: bool = sqlx::query_scalar("SELECT enabled FROM plugins WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("design enabled");
    assert!(!still);
}

#[tokio::test]
async fn design_named_plugin_in_default_dir_stays_disabled() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.clone().expect("db pool");
    let manifest =
        PathBuf::from(&state.config.plugins.dir).join("design/.claude-plugin/plugin.json");
    fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    fs::write(
        &manifest,
        r#"{"name":"design","description":"A user plugin, not the built-in."}"#,
    )
    .unwrap();

    let rescanned = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/plugins/rescan",
            "null",
            &cookie,
            &csrf,
        ))
        .await
        .expect("rescan");
    assert_eq!(rescanned.status(), StatusCode::OK);

    let enabled: bool = sqlx::query_scalar(
        r#"
        SELECT p.enabled FROM plugins p
        JOIN plugin_dirs d ON d.id = p.plugin_dir_id
        WHERE p.name = 'design' AND d.is_default
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("user design plugin");
    assert!(!enabled);

    let (status, builtin_enabled): (String, bool) = sqlx::query_as(
        r#"
        SELECT p.status, p.enabled FROM plugins p
        JOIN plugin_dirs d ON d.id = p.plugin_dir_id
        WHERE p.name = 'design' AND NOT d.is_default
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("builtin design plugin");
    // Earlier dir wins, even when that row is disabled.
    assert_eq!(status, "shadowed");
    assert!(builtin_enabled);
}

struct DesignRun {
    state: Arc<AppState>,
    app: Router,
    pool: PgPool,
    run_id: Uuid,
    agent_id: Uuid,
    ticket_id: Uuid,
    board_id: Uuid,
    design_id: Uuid,
    _git_dir: tempfile::TempDir,
}

async fn design_run() -> DesignRun {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;

    let pool = state.db.clone().expect("db pool");
    let ticket_uuid: Uuid = ticket_id.parse().unwrap();
    let agent_uuid: Uuid = agent_id.parse().unwrap();
    let run = RunService::new(&pool)
        .start_run_for_agent(
            ticket_uuid,
            agent_uuid,
            "work_on_ticket",
            Default::default(),
        )
        .await
        .expect("start run");
    let design_id: Uuid =
        sqlx::query_scalar("SELECT id FROM plugins WHERE name = 'design' AND enabled")
            .fetch_one(&pool)
            .await
            .expect("design plugin");
    DesignRun {
        state,
        app,
        pool,
        run_id: run.id,
        agent_id: agent_uuid,
        ticket_id: ticket_uuid,
        board_id: board_id.parse().unwrap(),
        design_id,
        _git_dir: git_dir,
    }
}

async fn rpc(url: &str, token: &str, method: &str, params: Value) -> Value {
    reqwest::Client::new()
        .post(url)
        .bearer_auth(token)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .expect("mcp request")
        .json()
        .await
        .expect("mcp json")
}

async fn mcp_url(app: Router) -> String {
    let addr = common::spawn_test_server(app).await;
    format!("http://{addr}/mcp")
}

fn tool_names(list: &Value) -> Vec<String> {
    list["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list failed: {list}"))
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect()
}

fn tool_json(call: &Value) -> Value {
    assert_eq!(call["result"]["isError"], false, "{call}");
    let text = call["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tool text missing: {call}"));
    serde_json::from_str(text).unwrap_or_else(|err| panic!("{err}: {text}"))
}

#[tokio::test]
async fn mcp_design_create_write_publish_list() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = design_run().await;
    let token = common::mint_test_token_with_plugins(
        &fx.state,
        fx.run_id,
        ContextProfile::Full,
        vec![fx.design_id],
    )
    .await;
    let url = mcp_url(fx.app.clone()).await;
    let names = tool_names(&rpc(&url, &token, "tools/list", json!({})).await);
    assert!(
        names.iter().any(|name| name == "design__create"),
        "{names:?}"
    );

    let created = tool_json(
        &rpc(
            &url,
            &token,
            "tools/call",
            json!({"name":"design__create","arguments":{"title":"Home"}}),
        )
        .await,
    );
    let set_id = created["setId"].as_str().expect("setId").to_string();
    let dir = PathBuf::from(created["dir"].as_str().expect("dir"));
    let artifacts = PathBuf::from(&fx.state.config.storage.artifacts_dir);
    assert!(dir.is_absolute());
    assert!(dir.starts_with(&artifacts), "{}", dir.display());

    let html_a = "<!doctype html><title>A</title>";
    let html_b = "<!doctype html><title>B</title>";
    for (path, content) in [("a/index.html", html_a), ("b/index.html", html_b)] {
        let wrote = tool_json(
            &rpc(
                &url,
                &token,
                "tools/call",
                json!({"name":"design__write","arguments":{"setId":set_id,"path":path,"content":content}}),
            )
            .await,
        );
        assert_eq!(wrote["path"], path);
    }
    assert_eq!(
        fs::read_to_string(dir.join("a/index.html")).unwrap(),
        html_a
    );
    assert_eq!(
        fs::read_to_string(dir.join("b/index.html")).unwrap(),
        html_b
    );

    let published = tool_json(
        &rpc(
            &url,
            &token,
            "tools/call",
            json!({
                "name": "design__publish",
                "arguments": {
                    "setId": set_id,
                    "recommendedOptionId": "b",
                    "options": [
                        {"id":"a","title":"A","entry":"a/index.html"},
                        {"id":"b","title":"B","entry":"b/index.html"}
                    ]
                }
            }),
        )
        .await,
    );
    assert_eq!(published["setId"], set_id);

    let listed = tool_json(
        &rpc(
            &url,
            &token,
            "tools/call",
            json!({"name":"design__list","arguments":{}}),
        )
        .await,
    );
    let sets = listed["sets"].as_array().expect("sets");
    assert_eq!(sets[0]["id"], set_id);
    assert_eq!(sets[0]["title"], "Home");
    assert!(sets[0]["chosen"].is_null());
    assert_eq!(sets[0]["recommended"]["id"], "b");
    assert_eq!(sets[0]["selected"]["id"], "b");
    assert_eq!(sets[0]["options"][0]["id"], "a");
    assert_eq!(sets[0]["options"][1]["id"], "b");
}

#[tokio::test]
async fn mcp_design_tools_absent_without_plugin() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = design_run().await;
    let token =
        common::mint_test_token_with_plugins(&fx.state, fx.run_id, ContextProfile::Full, vec![])
            .await;
    let url = mcp_url(fx.app).await;
    let names = tool_names(&rpc(&url, &token, "tools/list", json!({})).await);
    assert!(
        names.iter().all(|name| !name.starts_with("design__")),
        "{names:?}"
    );
    let denied = rpc(
        &url,
        &token,
        "tools/call",
        json!({"name":"design__create","arguments":{"title":"Home"}}),
    )
    .await;
    assert_eq!(denied["result"]["isError"], true, "{denied}");
}

#[tokio::test]
async fn mcp_plan_ticket_lists_design_tools() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = design_run().await;
    let token = TokenService::new(&fx.pool)
        .mint(
            &NewRunToolScope {
                run_id: fx.run_id,
                agent_id: fx.agent_id,
                ticket_id: Some(fx.ticket_id),
                chat_session_id: None,
                board_id: Some(fx.board_id),
                profile: ContextProfile::Full,
                job_type: "plan_ticket".into(),
                compaction_ticket_ids: vec![],
                plugin_ids: vec![fx.design_id],
            },
            Duration::from_secs(60),
        )
        .await
        .expect("mint");
    let url = mcp_url(fx.app).await;
    let listed = rpc(&url, &token, "tools/list", json!({})).await;
    let names = tool_names(&listed);
    for name in [
        "design__create",
        "design__write",
        "design__publish",
        "design__list",
    ] {
        assert!(names.iter().any(|got| got == name), "{names:?}");
    }
    let tools = listed["result"]["tools"].as_array().unwrap();
    let create = tools
        .iter()
        .find(|tool| tool["name"] == "design__create")
        .unwrap();
    assert_eq!(create["annotations"]["readOnlyHint"], false);
    let list = tools
        .iter()
        .find(|tool| tool["name"] == "design__list")
        .unwrap();
    assert_eq!(list["annotations"]["readOnlyHint"], true);
}

struct TicketHttp {
    state: Arc<AppState>,
    app: Router,
    cookie: String,
    csrf: String,
    pool: PgPool,
    ticket_id: Uuid,
    agent_id: Uuid,
    design_id: Uuid,
    _git_dir: Option<tempfile::TempDir>,
}

async fn ticket_http() -> TicketHttp {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let pool = state.db.clone().expect("db pool");
    let design_id: Uuid =
        sqlx::query_scalar("SELECT id FROM plugins WHERE name = 'design' AND enabled")
            .fetch_one(&pool)
            .await
            .expect("design plugin");
    TicketHttp {
        state,
        app,
        cookie,
        csrf,
        pool,
        ticket_id: ticket_id.parse().unwrap(),
        agent_id: agent_id.parse().unwrap(),
        design_id,
        _git_dir: Some(git_dir),
    }
}

async fn start_ticket_run(fx: &TicketHttp, agent_id: Uuid) -> Uuid {
    RunService::new(&fx.pool)
        .start_run_for_agent(fx.ticket_id, agent_id, "work_on_ticket", Default::default())
        .await
        .expect("start run")
        .id
}

fn two_options() -> Vec<DesignOption> {
    vec![
        DesignOption {
            id: "a".into(),
            title: "A".into(),
            entry: "a/index.html".into(),
        },
        DesignOption {
            id: "b".into(),
            title: "B".into(),
            entry: "b/index.html".into(),
        },
    ]
}

async fn publish_html(
    fx: &TicketHttp,
    run_id: Uuid,
    agent_id: Uuid,
    title: &str,
    ticket_id: Option<Uuid>,
    chat_session_id: Option<Uuid>,
    html_a: &str,
    html_b: &str,
) -> Uuid {
    let artifacts = PathBuf::from(&fx.state.config.storage.artifacts_dir);
    let svc = DesignService::new(&fx.pool, &artifacts);
    let created = svc
        .create(
            title,
            CreateScope {
                ticket_id,
                chat_session_id,
                run_id,
                agent_id,
            },
        )
        .await
        .expect("create");
    svc.write(created.id, run_id, "a/index.html", html_a.as_bytes())
        .await
        .expect("write a");
    svc.write(created.id, run_id, "b/index.html", html_b.as_bytes())
        .await
        .expect("write b");
    svc.publish(created.id, run_id, two_options(), "b")
        .await
        .expect("publish");
    created.id
}

#[tokio::test]
async fn choose_requires_session_and_known_option() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let run_id = start_ticket_run(&fx, fx.agent_id).await;
    let set_id = publish_html(
        &fx,
        run_id,
        fx.agent_id,
        "Home",
        Some(fx.ticket_id),
        None,
        "<!doctype html><title>A</title>",
        "<!doctype html><title>B</title>",
    )
    .await;

    let chosen = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/designs/{set_id}/choose"),
            r#"{"optionId":"a"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("choose");
    assert_eq!(chosen.status(), StatusCode::NO_CONTENT);

    let listed = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{}/designs", fx.ticket_id),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("list");
    assert_eq!(listed.status(), StatusCode::OK);
    let sets = common::json_body(listed).await;
    let sets = sets.as_array().expect("design list");
    assert_eq!(sets[0]["id"], set_id.to_string());
    assert_eq!(sets[0]["chosen"]["id"], "a");
    assert_eq!(sets[0]["chosen"]["title"], "A");
    assert_eq!(sets[0]["runId"], run_id.to_string());

    let got = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/designs/{set_id}"),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("get");
    assert_eq!(got.status(), StatusCode::OK);
    let got = common::json_body(got).await;
    assert_eq!(got["chosen"]["id"], "a");
    assert_eq!(got["runId"], run_id.to_string());
    assert_eq!(got["options"][0]["entry"], "a/index.html");

    let wrong = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/designs/{set_id}/choose"),
            r#"{"optionId":"nope"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("wrong option");
    assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);

    let missing = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/designs/{}/choose", Uuid::new_v4()),
            r#"{"optionId":"a"}"#,
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("missing set");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let unauth = fx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/designs/{set_id}/choose"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"optionId":"a"}"#))
                .unwrap(),
        )
        .await
        .expect("unauth");
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ticket_design_list_is_published_newest_first() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let older_run = start_ticket_run(&fx, fx.agent_id).await;
    let older = publish_html(
        &fx,
        older_run,
        fx.agent_id,
        "Older",
        Some(fx.ticket_id),
        None,
        "<!doctype html><title>old-a</title>",
        "<!doctype html><title>old-b</title>",
    )
    .await;
    let artifacts = PathBuf::from(&fx.state.config.storage.artifacts_dir);
    DesignService::new(&fx.pool, &artifacts)
        .create(
            "Draft",
            CreateScope {
                ticket_id: Some(fx.ticket_id),
                chat_session_id: None,
                run_id: older_run,
                agent_id: fx.agent_id,
            },
        )
        .await
        .expect("draft");

    let reviewer =
        common::create_test_agent_from_preset(&fx.app, "Reviewer", &fx.cookie, &fx.csrf).await;
    let newer_run = start_ticket_run(&fx, reviewer.parse().unwrap()).await;
    let newer = publish_html(
        &fx,
        newer_run,
        reviewer.parse().unwrap(),
        "Newer",
        Some(fx.ticket_id),
        None,
        "<!doctype html><title>new-a</title>",
        "<!doctype html><title>new-b</title>",
    )
    .await;

    let listed = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{}/designs", fx.ticket_id),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("list");
    assert_eq!(listed.status(), StatusCode::OK);
    let sets = common::json_body(listed).await;
    let sets = sets.as_array().expect("design list");
    assert_eq!(sets.len(), 2, "{sets:?}");
    assert_eq!(sets[0]["id"], newer.to_string());
    assert_eq!(sets[0]["title"], "Newer");
    assert_eq!(sets[0]["runId"], newer_run.to_string());
    assert_eq!(sets[0]["options"][0]["id"], "a");
    assert_eq!(sets[0]["options"][0]["entry"], "a/index.html");
    assert!(sets[0]["chosen"].is_null());
    assert_eq!(sets[1]["id"], older.to_string());
    assert_eq!(sets[1]["title"], "Older");
    assert_eq!(sets[1]["runId"], older_run.to_string());
}

#[tokio::test]
async fn design_file_is_html_without_attachment() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let run_id = start_ticket_run(&fx, fx.agent_id).await;
    let html_a = "<!doctype html><title>A</title>design-preview-marker";
    let set_id = publish_html(
        &fx,
        run_id,
        fx.agent_id,
        "Home",
        Some(fx.ticket_id),
        None,
        html_a,
        "<!doctype html><title>B</title>",
    )
    .await;

    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/designs/{set_id}/files/a/index.html"),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("file");
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/html")
    );
    assert_eq!(
        res.headers()
            .get(header::X_CONTENT_TYPE_OPTIONS)
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        res.headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        res.headers()
            .get(header::CONTENT_SECURITY_POLICY)
            .and_then(|value| value.to_str().ok()),
        Some("default-src 'none'; style-src 'unsafe-inline'; sandbox")
    );
    if let Some(disposition) = res.headers().get(header::CONTENT_DISPOSITION) {
        let disposition = disposition.to_str().unwrap_or("");
        assert!(
            !disposition.to_ascii_lowercase().contains("attachment"),
            "{disposition}"
        );
    }
    let bytes = BodyExt::collect(res.into_body())
        .await
        .expect("body")
        .to_bytes();
    assert_eq!(bytes.as_ref(), html_a.as_bytes());
}

#[tokio::test]
async fn ticket_get_includes_design_summary_without_html() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let run_id = start_ticket_run(&fx, fx.agent_id).await;
    let marker = "design-preview-marker-ticket-get";
    let set_id = publish_html(
        &fx,
        run_id,
        fx.agent_id,
        "Home",
        Some(fx.ticket_id),
        None,
        &format!("<!doctype html><title>A</title>{marker}"),
        "<!doctype html><title>B</title>",
    )
    .await;
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&fx.pool)
        .await
        .expect("user");
    DesignService::new(
        &fx.pool,
        PathBuf::from(&fx.state.config.storage.artifacts_dir),
    )
    .choose(set_id, "a", user_id)
    .await
    .expect("choose");

    let token = common::mint_test_token_with_plugins(
        &fx.state,
        run_id,
        ContextProfile::Full,
        vec![fx.design_id],
    )
    .await;
    let url = mcp_url(fx.app).await;
    let body = tool_json(
        &rpc(
            &url,
            &token,
            "tools/call",
            json!({"name":"ticket_get","arguments":{}}),
        )
        .await,
    );
    let designs = body["designs"].as_array().expect("designs");
    assert_eq!(designs[0]["title"], "Home");
    assert_eq!(designs[0]["options"][0]["id"], "a");
    assert_eq!(designs[0]["options"][0]["title"], "A");
    assert_eq!(designs[0]["options"][1]["id"], "b");
    assert!(designs[0]["options"][0].get("entry").is_none());
    assert_eq!(designs[0]["chosen"]["id"], "a");
    assert_eq!(designs[0]["chosen"]["title"], "A");
    assert!(!body.to_string().contains(marker));
    assert!(!body.to_string().contains("<!doctype"));
}

#[tokio::test]
async fn chat_design_list_returns_published_set() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let agent_id = common::create_test_agent_from_preset(&app, "Worker", &cookie, &csrf).await;
    let created = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/chat/sessions",
            &format!(r#"{{"agentId":"{agent_id}"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .expect("session");
    assert_eq!(created.status(), StatusCode::CREATED);
    let session_id = Uuid::parse_str(common::json_body(created).await["id"].as_str().unwrap())
        .expect("session id");
    let pool = state.db.clone().expect("db pool");
    let message_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_messages (id, session_id, seq, role, body) VALUES ($1, $2, 0, 'human', 'hi')",
    )
    .bind(message_id)
    .bind(session_id)
    .execute(&pool)
    .await
    .expect("message");
    let run = RunService::new(&pool)
        .start_chat_turn(session_id, message_id, agent_id.parse().unwrap())
        .await
        .expect("chat run");
    let fx = TicketHttp {
        state,
        app: app.clone(),
        cookie: cookie.clone(),
        csrf: csrf.clone(),
        pool,
        ticket_id: Uuid::nil(),
        agent_id: agent_id.parse().unwrap(),
        design_id: Uuid::nil(),
        _git_dir: None,
    };
    let set_id = publish_html(
        &fx,
        run.id,
        fx.agent_id,
        "Chat home",
        None,
        Some(session_id),
        "<!doctype html><title>A</title>",
        "<!doctype html><title>B</title>",
    )
    .await;

    let listed = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/chat/sessions/{session_id}/designs"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .expect("chat designs");
    assert_eq!(listed.status(), StatusCode::OK);
    let sets = common::json_body(listed).await;
    let sets = sets.as_array().expect("design list");
    assert_eq!(sets[0]["id"], set_id.to_string());
    assert_eq!(sets[0]["title"], "Chat home");
    assert_eq!(sets[0]["runId"], run.id.to_string());
    assert!(sets[0]["chosen"].is_null());

    let member_email = format!("member-{}@localhost", Uuid::new_v4());
    create_member(&app, &cookie, &csrf, &member_email).await;
    let (member_cookie, member_csrf) = login_as(&app, &member_email, "secret123").await;
    let denied = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/designs/{set_id}/choose"),
            r#"{"optionId":"a"}"#,
            &member_cookie,
            &member_csrf,
        ))
        .await
        .expect("member choose");
    assert_eq!(denied.status(), StatusCode::NOT_FOUND);
    let still = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/chat/sessions/{session_id}/designs"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .expect("owner list");
    assert_eq!(still.status(), StatusCode::OK);
    let still = common::json_body(still).await;
    assert!(still.as_array().expect("design list")[0]["chosen"].is_null());
}

#[tokio::test]
async fn unpublished_design_is_hidden() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let run_id = start_ticket_run(&fx, fx.agent_id).await;
    let artifacts = PathBuf::from(&fx.state.config.storage.artifacts_dir);
    let svc = DesignService::new(&fx.pool, &artifacts);
    let created = svc
        .create(
            "Draft",
            CreateScope {
                ticket_id: Some(fx.ticket_id),
                chat_session_id: None,
                run_id,
                agent_id: fx.agent_id,
            },
        )
        .await
        .expect("draft");
    svc.write(created.id, run_id, "a/index.html", b"<p>draft</p>")
        .await
        .expect("write");

    for (method, uri) in [
        ("GET", format!("/api/designs/{}", created.id)),
        ("POST", format!("/api/designs/{}/choose", created.id)),
        (
            "GET",
            format!("/api/designs/{}/files/a/index.html", created.id),
        ),
    ] {
        let body = if method == "POST" {
            r#"{"optionId":"a"}"#
        } else {
            ""
        };
        let res = fx
            .app
            .clone()
            .oneshot(common::json_request(
                method,
                &uri,
                body,
                &fx.cookie,
                &fx.csrf,
            ))
            .await
            .expect(method);
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{method} {uri}");
    }
}

#[tokio::test]
async fn design_file_rejects_oversize() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let run_id = start_ticket_run(&fx, fx.agent_id).await;
    let set_id = publish_html(
        &fx,
        run_id,
        fx.agent_id,
        "Home",
        Some(fx.ticket_id),
        None,
        "<!doctype html><title>A</title>",
        "<!doctype html><title>B</title>",
    )
    .await;
    let path = PathBuf::from(&fx.state.config.storage.artifacts_dir)
        .join(format!("designs/{set_id}/a/index.html"));
    fs::write(&path, vec![b'x'; 512 * 1024 + 1]).expect("oversize");

    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/designs/{set_id}/files/a/index.html"),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("oversize file");
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

async fn create_member(app: &Router, admin_cookie: &str, admin_csrf: &str, email: &str) {
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
        .expect("create member");
    assert_eq!(res.status(), StatusCode::CREATED);
}

async fn login_as(app: &Router, email: &str, password: &str) -> (String, String) {
    use coppice_server::middleware::session::parse_session_cookie;

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    r#"{{"email":"{email}","password":"{password}"}}"#
                )))
                .unwrap(),
        )
        .await
        .expect("login");
    assert_eq!(login.status(), StatusCode::OK);
    let set_cookie = login
        .headers()
        .get(header::SET_COOKIE)
        .expect("session cookie");
    let session_token = parse_session_cookie(set_cookie.to_str().unwrap()).expect("session token");
    let cookie = format!("coppice_session={session_token}");
    let login_json = common::json_body(login).await;
    let csrf = login_json["csrfToken"]
        .as_str()
        .expect("csrf token")
        .to_string();
    (cookie, csrf)
}

#[tokio::test]
async fn publish_rejects_unknown_recommendation() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let run_id = start_ticket_run(&fx, fx.agent_id).await;
    let svc = DesignService::new(
        &fx.pool,
        PathBuf::from(&fx.state.config.storage.artifacts_dir),
    );
    let created = svc
        .create(
            "Home",
            CreateScope {
                ticket_id: Some(fx.ticket_id),
                chat_session_id: None,
                run_id,
                agent_id: fx.agent_id,
            },
        )
        .await
        .expect("create");
    for path in ["a/index.html", "b/index.html"] {
        svc.write(created.id, run_id, path, b"<!doctype html>")
            .await
            .expect("write");
    }
    let err = svc
        .publish(created.id, run_id, two_options(), "z")
        .await
        .unwrap_err();
    assert!(
        matches!(err, DesignError::Invalid(ref msg)
            if msg == "recommendedOptionId must be one of the options"),
        "{err}"
    );
}

#[tokio::test]
async fn selected_follows_choice_then_falls_back_to_recommendation() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let run_id = start_ticket_run(&fx, fx.agent_id).await;
    let set_id = publish_html(
        &fx,
        run_id,
        fx.agent_id,
        "Home",
        Some(fx.ticket_id),
        None,
        "<!doctype html><title>A</title>",
        "<!doctype html><title>B</title>",
    )
    .await;
    let svc = DesignService::new(
        &fx.pool,
        PathBuf::from(&fx.state.config.storage.artifacts_dir),
    );
    let summary = svc.get(set_id).await.expect("get").summary;
    assert_eq!(summary.recommended.as_ref().expect("rec").id, "b");
    assert_eq!(summary.selected.as_ref().expect("sel").id, "b");
    assert!(summary.chosen.is_none());
    assert!(!summary.superseded);
    assert!(!summary.from_plan_run);

    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&fx.pool)
        .await
        .expect("user");
    svc.choose(set_id, "a", user_id).await.expect("choose");
    let summary = svc.get(set_id).await.expect("get").summary;
    assert_eq!(summary.selected.as_ref().expect("sel").id, "a");
    assert_eq!(summary.recommended.as_ref().expect("rec").id, "b");

    let token = common::mint_test_token_with_plugins(
        &fx.state,
        run_id,
        ContextProfile::Full,
        vec![fx.design_id],
    )
    .await;
    let url = mcp_url(fx.app.clone()).await;
    let listed = tool_json(
        &rpc(
            &url,
            &token,
            "tools/call",
            json!({"name":"design__list","arguments":{}}),
        )
        .await,
    );
    assert_eq!(listed["sets"][0]["recommended"]["id"], "b");
    assert_eq!(listed["sets"][0]["selected"]["id"], "a");
    let body = tool_json(
        &rpc(
            &url,
            &token,
            "tools/call",
            json!({"name":"ticket_get","arguments":{}}),
        )
        .await,
    );
    assert_eq!(body["designs"][0]["recommended"]["id"], "b");
    assert_eq!(body["designs"][0]["selected"]["id"], "a");

    let got = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/designs/{set_id}"),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("get");
    let got = common::json_body(got).await;
    assert_eq!(got["recommended"]["id"], "b");
    assert_eq!(got["selected"]["id"], "a");
    assert_eq!(got["superseded"], false);
    assert_eq!(got["fromPlanRun"], false);
}

#[tokio::test]
async fn set_from_before_recommendations_has_no_selection() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let run_id = start_ticket_run(&fx, fx.agent_id).await;
    let set_id = publish_html(
        &fx,
        run_id,
        fx.agent_id,
        "Home",
        Some(fx.ticket_id),
        None,
        "<!doctype html><title>A</title>",
        "<!doctype html><title>B</title>",
    )
    .await;
    sqlx::query("UPDATE design_sets SET recommended_option_id = NULL WHERE id = $1")
        .bind(set_id)
        .execute(&fx.pool)
        .await
        .expect("clear");
    let summary = DesignService::new(
        &fx.pool,
        PathBuf::from(&fx.state.config.storage.artifacts_dir),
    )
    .get(set_id)
    .await
    .expect("get")
    .summary;
    assert!(summary.recommended.is_none());
    assert!(summary.selected.is_none());
}

const PICK_PLAN: &str = "## Plan\n\n### Approach\nShip it.\n\n### Steps\n- [ ] Add the button\n\n### Risks\nNone.\n\n### Out of scope\nNone.\n";
const B_NOTE: &str = "Update the plan to use design option B: Full-width button with Show all.";
const A_NOTE: &str = "Update the plan to use design option A: Quiet button after the last card.";

fn planning(fx: &TicketHttp) -> PlanningService<'_> {
    PlanningService::new(&fx.pool)
}

fn design_svc(fx: &TicketHttp) -> DesignService<'_> {
    DesignService::new(
        &fx.pool,
        PathBuf::from(&fx.state.config.storage.artifacts_dir),
    )
}

async fn publish_plan_options(fx: &TicketHttp, run_id: Uuid, agent_id: Uuid) -> Uuid {
    let svc = design_svc(fx);
    let created = svc
        .create(
            "Home",
            CreateScope {
                ticket_id: Some(fx.ticket_id),
                chat_session_id: None,
                run_id,
                agent_id,
            },
        )
        .await
        .expect("create");
    for entry in ["a/index.html", "b/index.html"] {
        svc.write(created.id, run_id, entry, b"<!doctype html><title>x</title>")
            .await
            .expect("write");
    }
    let options = vec![
        DesignOption {
            id: "a".into(),
            title: "Quiet button after the last card".into(),
            entry: "a/index.html".into(),
        },
        DesignOption {
            id: "b".into(),
            title: "Full-width button with Show all".into(),
            entry: "b/index.html".into(),
        },
    ];
    svc.publish(created.id, run_id, options, "a")
        .await
        .expect("publish");
    created.id
}

async fn active_plan_run(fx: &TicketHttp) -> Uuid {
    planning(fx)
        .active_plan_run(fx.ticket_id)
        .await
        .expect("active run")
        .expect("a plan run is active")
}

async fn set_run_status(fx: &TicketHttp, run_id: Uuid, status: &str) {
    sqlx::query("UPDATE agent_runs SET status = $2, ended_at = now() WHERE id = $1")
        .bind(run_id)
        .bind(status)
        .execute(&fx.pool)
        .await
        .expect("finish run");
}

/// A ticket without a plan or a plan run yet, and with a PM to write one.
async fn unplanned_with_pm() -> TicketHttp {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM", &cookie, &csrf).await;
    let ticket_id = common::create_unplanned_ticket(&app, &board_id, &cookie, &csrf).await;
    let pool = state.db.clone().expect("db pool");
    TicketHttp {
        state,
        app,
        cookie,
        csrf,
        pool,
        ticket_id: ticket_id.parse().unwrap(),
        agent_id: pm_id.parse().unwrap(),
        design_id: Uuid::nil(),
        _git_dir: None,
    }
}

/// A Plan Review ticket whose current plan was written around option "a",
/// the recommendation of the set its plan run published.
async fn plan_review_with_set() -> (TicketHttp, Uuid) {
    let fx = unplanned_with_pm().await;
    let plans = planning(&fx);
    plans
        .insert_plan(fx.ticket_id, fx.agent_id, PICK_PLAN, None)
        .await
        .expect("first plan");
    plans.bind_latest_plan(fx.ticket_id).await.expect("bind");
    plans
        .ask_for_changes(fx.ticket_id, "Show me options.")
        .await
        .expect("ask");
    let run_id = active_plan_run(&fx).await;
    let set_id = publish_plan_options(&fx, run_id, fx.agent_id).await;
    plans
        .insert_plan(fx.ticket_id, fx.agent_id, PICK_PLAN, Some(run_id))
        .await
        .expect("plan with design");
    set_run_status(&fx, run_id, "succeeded").await;
    (fx, set_id)
}

async fn pick(fx: &TicketHttp, set_id: Uuid, option_id: &str) -> axum::response::Response {
    fx.app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/designs/{set_id}/choose"),
            &format!(r#"{{"optionId":"{option_id}"}}"#),
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("pick")
}

async fn set_json(fx: &TicketHttp, set_id: Uuid) -> Value {
    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/designs/{set_id}"),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("get set");
    assert_eq!(res.status(), StatusCode::OK);
    common::json_body(res).await
}

async fn plan_runs(fx: &TicketHttp) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM agent_runs WHERE ticket_id = $1 AND job_type = 'plan_ticket'",
    )
    .bind(fx.ticket_id)
    .fetch_one(&fx.pool)
    .await
    .expect("plan runs")
}

async fn change_request(fx: &TicketHttp) -> Option<String> {
    planning(fx)
        .latest_plan(fx.ticket_id)
        .await
        .expect("latest plan")
        .expect("plan")
        .change_request
}

async fn assert_refused(fx: &TicketHttp, set_id: Uuid, option_id: &str, expected: &str) {
    let runs = plan_runs(fx).await;
    let res = pick(fx, set_id, option_id).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(common::json_body(res).await["message"], expected);
    let set = set_json(fx, set_id).await;
    assert!(set["chosen"].is_null(), "nothing saved: {set}");
    assert_eq!(plan_runs(fx).await, runs, "no run started");
}

#[tokio::test]
async fn picking_the_selected_option_starts_no_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    let before = set_json(&fx, set_id).await;
    assert_eq!(before["choice"]["mode"], "update_plan");
    assert_eq!(before["choice"]["planOutOfDate"], false);
    let runs = plan_runs(&fx).await;

    let res = pick(&fx, set_id, "a").await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let set = set_json(&fx, set_id).await;
    assert_eq!(set["chosen"]["id"], "a");
    assert_eq!(set["choice"]["planOutOfDate"], false);
    assert_eq!(plan_runs(&fx).await, runs);
    assert_eq!(change_request(&fx).await, None);
}

#[tokio::test]
async fn picking_another_starts_a_run_with_the_note() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    let runs = plan_runs(&fx).await;

    let res = pick(&fx, set_id, "b").await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    assert_eq!(plan_runs(&fx).await, runs + 1);
    active_plan_run(&fx).await;
    assert_eq!(change_request(&fx).await.as_deref(), Some(B_NOTE));
    let set = set_json(&fx, set_id).await;
    assert_eq!(set["chosen"]["id"], "b");
    assert_eq!(set["choice"]["mode"], "plan_running");
    assert_eq!(set["choice"]["planOutOfDate"], false);
}

#[tokio::test]
async fn picking_back_to_the_earlier_option_starts_another_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    assert_eq!(pick(&fx, set_id, "b").await.status(), StatusCode::NO_CONTENT);
    let run_id = active_plan_run(&fx).await;
    planning(&fx)
        .insert_plan(fx.ticket_id, fx.agent_id, PICK_PLAN, Some(run_id))
        .await
        .expect("rewritten plan");
    set_run_status(&fx, run_id, "succeeded").await;
    let runs = plan_runs(&fx).await;
    assert_eq!(set_json(&fx, set_id).await["choice"]["planOutOfDate"], false);

    assert_eq!(pick(&fx, set_id, "a").await.status(), StatusCode::NO_CONTENT);

    assert_eq!(plan_runs(&fx).await, runs + 1);
    assert_eq!(change_request(&fx).await.as_deref(), Some(A_NOTE));
    assert_eq!(set_json(&fx, set_id).await["chosen"]["id"], "a");
}

#[tokio::test]
async fn pick_is_refused_during_a_plan_run_and_saves_nothing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    planning(&fx)
        .ask_for_changes(fx.ticket_id, "Tighten the steps.")
        .await
        .expect("ask");
    assert_eq!(set_json(&fx, set_id).await["choice"]["mode"], "plan_running");

    assert_refused(&fx, set_id, "b", PLAN_ALREADY_RUNNING).await;
}

#[tokio::test]
async fn pick_is_refused_after_approval_and_saves_nothing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    let approved = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{}/approve-plan", fx.ticket_id),
            "{}",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("approve");
    assert_eq!(approved.status(), StatusCode::OK);
    assert_eq!(set_json(&fx, set_id).await["choice"]["mode"], "plan_final");

    assert_refused(&fx, set_id, "b", PLAN_DESIGN_FINAL).await;
}

#[tokio::test]
async fn pick_is_refused_without_a_planner_and_saves_nothing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    sqlx::query("UPDATE agents SET enabled = false")
        .execute(&fx.pool)
        .await
        .expect("disable agents");

    assert_refused(&fx, set_id, "b", NO_PM).await;

    let notes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ticket_comments WHERE ticket_id = $1 AND body = $2",
    )
    .bind(fx.ticket_id)
    .bind(NO_PM)
    .fetch_one(&fx.pool)
    .await
    .expect("notes");
    assert_eq!(notes, 0, "the refusal posts no note");
    assert_eq!(change_request(&fx).await, None);
}

#[tokio::test]
async fn pick_is_refused_on_a_superseded_set() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    planning(&fx)
        .ask_for_changes(fx.ticket_id, "New options, please.")
        .await
        .expect("ask");
    let run_id = active_plan_run(&fx).await;
    publish_plan_options(&fx, run_id, fx.agent_id).await;
    set_run_status(&fx, run_id, "succeeded").await;
    assert_eq!(set_json(&fx, set_id).await["choice"]["mode"], "read_only");

    assert_refused(&fx, set_id, "b", DESIGN_SUPERSEDED).await;
}

#[tokio::test]
async fn pick_is_refused_on_an_archived_ticket() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    TicketService::new(&fx.pool)
        .archive(fx.ticket_id)
        .await
        .expect("archive");
    assert_eq!(set_json(&fx, set_id).await["choice"]["mode"], "read_only");

    assert_refused(
        &fx,
        set_id,
        "b",
        "ticket is archived; unarchive before continuing",
    )
    .await;
}

#[tokio::test]
async fn failed_start_restores_the_earlier_choice() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    sqlx::raw_sql(&format!(
        r#"
        CREATE OR REPLACE FUNCTION test_refuse_change_request() RETURNS trigger AS $$
        BEGIN
            RAISE EXCEPTION 'change request refused by test';
        END;
        $$ LANGUAGE plpgsql;
        DROP TRIGGER IF EXISTS test_refuse_change_request ON ticket_plans;
        CREATE TRIGGER test_refuse_change_request
            BEFORE UPDATE OF change_request ON ticket_plans
            FOR EACH ROW WHEN (OLD.ticket_id = '{}')
            EXECUTE FUNCTION test_refuse_change_request();
        "#,
        fx.ticket_id
    ))
    .execute(&fx.pool)
    .await
    .expect("install trigger");

    // No earlier choice: the set falls back to its recommendation again.
    let res = pick(&fx, set_id, "b").await;
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let set = set_json(&fx, set_id).await;
    assert!(set["chosen"].is_null(), "{set}");
    assert_eq!(set["selected"]["id"], "a");

    // An earlier choice comes back with who made it and when.
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&fx.pool)
        .await
        .expect("user");
    let at = time::macros::datetime!(2026-01-02 03:04:05 UTC);
    sqlx::query(
        r#"
        UPDATE design_sets
        SET chosen_option_id = 'a', chosen_by_user_id = $2, chosen_at = $3
        WHERE id = $1
        "#,
    )
    .bind(set_id)
    .bind(user_id)
    .bind(at)
    .execute(&fx.pool)
    .await
    .expect("earlier choice");

    let res = pick(&fx, set_id, "b").await;
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let restored: (Option<String>, Option<Uuid>, Option<time::OffsetDateTime>) = sqlx::query_as(
        "SELECT chosen_option_id, chosen_by_user_id, chosen_at FROM design_sets WHERE id = $1",
    )
    .bind(set_id)
    .fetch_one(&fx.pool)
    .await
    .expect("restored");
    sqlx::raw_sql(
        r#"
        DROP TRIGGER test_refuse_change_request ON ticket_plans;
        DROP FUNCTION test_refuse_change_request();
        "#,
    )
    .execute(&fx.pool)
    .await
    .expect("drop trigger");
    assert_eq!(restored, (Some("a".into()), Some(user_id), Some(at)));
    assert!(planning(&fx)
        .active_plan_run(fx.ticket_id)
        .await
        .expect("active run")
        .is_none());
}

#[tokio::test]
async fn failed_rewrite_keeps_choice_and_try_again_starts_a_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    assert_eq!(pick(&fx, set_id, "b").await.status(), StatusCode::NO_CONTENT);
    let run_id = active_plan_run(&fx).await;
    set_run_status(&fx, run_id, "failed").await;

    let set = set_json(&fx, set_id).await;
    assert_eq!(set["chosen"]["id"], "b");
    assert_eq!(set["choice"]["mode"], "update_plan");
    assert_eq!(set["choice"]["planOutOfDate"], true);
    let runs = plan_runs(&fx).await;

    assert_eq!(pick(&fx, set_id, "b").await.status(), StatusCode::NO_CONTENT);

    assert_eq!(plan_runs(&fx).await, runs + 1);
    active_plan_run(&fx).await;
    assert_eq!(set_json(&fx, set_id).await["chosen"]["id"], "b");
    // Try again requests the same change, and doesn't list it twice.
    assert_eq!(change_request(&fx).await.as_deref(), Some(B_NOTE));
}

#[tokio::test]
async fn pick_with_no_plan_saves_and_starts_no_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = unplanned_with_pm().await;
    planning(&fx)
        .start_planning(fx.ticket_id)
        .await
        .expect("start planning");
    let run_id = active_plan_run(&fx).await;
    let set_id = publish_plan_options(&fx, run_id, fx.agent_id).await;
    set_run_status(&fx, run_id, "failed").await;
    assert_eq!(set_json(&fx, set_id).await["choice"]["mode"], "save_only");
    let runs = plan_runs(&fx).await;

    assert_eq!(pick(&fx, set_id, "b").await.status(), StatusCode::NO_CONTENT);

    assert_eq!(set_json(&fx, set_id).await["chosen"]["id"], "b");
    assert_eq!(plan_runs(&fx).await, runs);
    let plan = planning(&fx)
        .insert_plan(fx.ticket_id, fx.agent_id, PICK_PLAN, None)
        .await
        .expect("next plan");
    assert_eq!(plan.design_option_id.as_deref(), Some("b"));
}

#[tokio::test]
async fn pick_on_a_set_from_before_recommendations_saves_and_starts_no_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    sqlx::query("UPDATE design_sets SET recommended_option_id = NULL WHERE id = $1")
        .bind(set_id)
        .execute(&fx.pool)
        .await
        .expect("clear recommendation");
    // A plan written around such a set records the set and no option.
    sqlx::query("UPDATE ticket_plans SET design_option_id = NULL WHERE ticket_id = $1")
        .bind(fx.ticket_id)
        .execute(&fx.pool)
        .await
        .expect("clear recorded option");
    let before = set_json(&fx, set_id).await;
    assert_eq!(before["choice"]["mode"], "update_plan");
    assert_eq!(before["choice"]["planOutOfDate"], false);
    let runs = plan_runs(&fx).await;

    assert_eq!(pick(&fx, set_id, "b").await.status(), StatusCode::NO_CONTENT);

    let after = set_json(&fx, set_id).await;
    assert_eq!(after["chosen"]["id"], "b");
    assert_eq!(plan_runs(&fx).await, runs);
    assert_eq!(change_request(&fx).await, None);
    // The plan recorded no option, so nothing says it wasn't updated.
    assert_eq!(after["choice"]["planOutOfDate"], false);
}

#[tokio::test]
async fn plan_with_no_recorded_option_counts_as_up_to_date() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    sqlx::query("UPDATE ticket_plans SET design_option_id = NULL WHERE ticket_id = $1")
        .bind(fx.ticket_id)
        .execute(&fx.pool)
        .await
        .expect("clear recorded option");

    let set = set_json(&fx, set_id).await;
    assert_eq!(set["selected"]["id"], "a");
    assert_eq!(set["choice"]["mode"], "update_plan");
    assert_eq!(set["choice"]["planOutOfDate"], false);
}

#[tokio::test]
async fn work_run_and_chat_sets_save_and_start_no_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let run_id = start_ticket_run(&fx, fx.agent_id).await;
    let set_id = publish_html(
        &fx,
        run_id,
        fx.agent_id,
        "Home",
        Some(fx.ticket_id),
        None,
        "<!doctype html><title>A</title>",
        "<!doctype html><title>B</title>",
    )
    .await;
    let before = set_json(&fx, set_id).await;
    assert_eq!(before["choice"]["mode"], "save_only");
    // Confirming the recommendation is a pick like any other: it has to save.
    assert_eq!(before["selected"]["id"], before["recommended"]["id"]);
    assert_eq!(before["chosen"], Value::Null);
    let recommended = before["recommended"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        pick(&fx, set_id, &recommended).await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(set_json(&fx, set_id).await["chosen"]["id"], recommended);
    assert_eq!(plan_runs(&fx).await, 0);

    let created = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "POST",
            "/api/chat/sessions",
            &format!(r#"{{"agentId":"{}"}}"#, fx.agent_id),
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("session");
    assert_eq!(created.status(), StatusCode::CREATED);
    let session_id: Uuid = common::json_body(created).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let message_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO chat_messages (id, session_id, seq, role, body) VALUES ($1, $2, 0, 'human', 'hi')",
    )
    .bind(message_id)
    .bind(session_id)
    .execute(&fx.pool)
    .await
    .expect("message");
    let chat_run = RunService::new(&fx.pool)
        .start_chat_turn(session_id, message_id, fx.agent_id)
        .await
        .expect("chat run");
    let chat_set = publish_html(
        &fx,
        chat_run.id,
        fx.agent_id,
        "Chat home",
        None,
        Some(session_id),
        "<!doctype html><title>A</title>",
        "<!doctype html><title>B</title>",
    )
    .await;
    assert_eq!(set_json(&fx, chat_set).await["choice"]["mode"], "save_only");
    assert_eq!(pick(&fx, chat_set, "a").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(set_json(&fx, chat_set).await["chosen"]["id"], "a");
    let all_plan_runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM agent_runs WHERE job_type = 'plan_ticket'")
            .fetch_one(&fx.pool)
            .await
            .expect("all plan runs");
    assert_eq!(all_plan_runs, 0);
}

#[tokio::test]
async fn failed_run_insert_leaves_note_split_approval_and_choice_unchanged() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    let split = json!({"fromPlan": true, "reason": "Too big"});
    sqlx::query(
        r#"
        UPDATE ticket_plans SET change_request = 'Earlier request.'
        WHERE ticket_id = $1 AND revision = (
            SELECT max(revision) FROM ticket_plans WHERE ticket_id = $1
        )
        "#,
    )
    .bind(fx.ticket_id)
    .execute(&fx.pool)
    .await
    .expect("earlier request");
    sqlx::query("UPDATE tickets SET pending_split_recommendation = $2 WHERE id = $1")
        .bind(fx.ticket_id)
        .bind(&split)
        .execute(&fx.pool)
        .await
        .expect("split");
    sqlx::raw_sql(&format!(
        r#"
        CREATE OR REPLACE FUNCTION test_refuse_plan_run() RETURNS trigger AS $$
        BEGIN
            RAISE EXCEPTION 'plan run refused by test';
        END;
        $$ LANGUAGE plpgsql;
        DROP TRIGGER IF EXISTS test_refuse_plan_run ON agent_runs;
        CREATE TRIGGER test_refuse_plan_run
            BEFORE INSERT ON agent_runs
            FOR EACH ROW WHEN (NEW.ticket_id = '{}')
            EXECUTE FUNCTION test_refuse_plan_run();
        "#,
        fx.ticket_id
    ))
    .execute(&fx.pool)
    .await
    .expect("install trigger");
    let runs = plan_runs(&fx).await;

    let picked = pick(&fx, set_id, "b").await.status();
    let after_pick = (
        change_request(&fx).await,
        ticket_split(&fx).await,
        set_json(&fx, set_id).await,
    );

    // A pick is refused on an approved plan, so approval goes through the ask itself.
    let plan_id = planning(&fx)
        .latest_plan(fx.ticket_id)
        .await
        .expect("latest plan")
        .expect("plan")
        .id;
    sqlx::query(
        r#"
        UPDATE tickets
        SET approved_plan_id = $2, approved_plan_content_version = content_version
        WHERE id = $1
        "#,
    )
    .bind(fx.ticket_id)
    .bind(plan_id)
    .execute(&fx.pool)
    .await
    .expect("approval");
    let asked = planning(&fx).ask_for_changes(fx.ticket_id, "Change it.").await;
    let approved: Option<Uuid> =
        sqlx::query_scalar("SELECT approved_plan_id FROM tickets WHERE id = $1")
            .bind(fx.ticket_id)
            .fetch_one(&fx.pool)
            .await
            .expect("approved plan");
    let after_ask = (change_request(&fx).await, ticket_split(&fx).await);
    sqlx::raw_sql(
        r#"
        DROP TRIGGER test_refuse_plan_run ON agent_runs;
        DROP FUNCTION test_refuse_plan_run();
        "#,
    )
    .execute(&fx.pool)
    .await
    .expect("drop trigger");

    assert_eq!(picked, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(after_pick.0.as_deref(), Some("Earlier request."));
    assert_eq!(after_pick.1, Some(split.clone()));
    assert!(after_pick.2["chosen"].is_null(), "{}", after_pick.2);
    assert_eq!(after_pick.2["selected"]["id"], "a");
    assert_eq!(plan_runs(&fx).await, runs);

    assert!(asked.is_err());
    assert_eq!(approved, Some(plan_id));
    assert_eq!(after_ask.0.as_deref(), Some("Earlier request."));
    assert_eq!(after_ask.1, Some(split));
}

async fn ticket_split(fx: &TicketHttp) -> Option<Value> {
    sqlx::query_scalar("SELECT pending_split_recommendation FROM tickets WHERE id = $1")
        .bind(fx.ticket_id)
        .fetch_one(&fx.pool)
        .await
        .expect("split")
}

#[tokio::test]
async fn pick_is_refused_when_the_planner_has_another_active_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    // One active run per ticket and agent, so queuing the plan run conflicts,
    // while the active plan run check before it finds nothing.
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            id, ticket_id, agent_id, job_type, status, sandbox_profile_id, context_profile
        )
        SELECT gen_random_uuid(), ticket_id, agent_id, 'work_on_ticket', 'queued',
               sandbox_profile_id, context_profile
        FROM agent_runs WHERE ticket_id = $1 LIMIT 1
        "#,
    )
    .bind(fx.ticket_id)
    .execute(&fx.pool)
    .await
    .expect("other run");
    assert_eq!(set_json(&fx, set_id).await["choice"]["mode"], "update_plan");

    assert_refused(&fx, set_id, "b", PLAN_ALREADY_RUNNING).await;
    assert_eq!(change_request(&fx).await, None);
}

#[tokio::test]
async fn pick_is_refused_with_skip_planning_and_saves_nothing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    sqlx::query("UPDATE tickets SET skip_planning = true WHERE id = $1")
        .bind(fx.ticket_id)
        .execute(&fx.pool)
        .await
        .expect("skip planning");

    assert_refused(&fx, set_id, "b", NO_PM).await;
    assert_eq!(change_request(&fx).await, None);
}

/// Work runs need a repo; the plan-run fixtures have none.
async fn ensure_repo(fx: &TicketHttp, ticket_id: Uuid) {
    let has: Option<Uuid> = sqlx::query_scalar("SELECT repo_id FROM tickets WHERE id = $1")
        .bind(ticket_id)
        .fetch_one(&fx.pool)
        .await
        .expect("repo");
    if has.is_some() {
        return;
    }
    let (dir, path) = common::create_temp_git_checkout();
    let repo_id = match sqlx::query_scalar::<_, Uuid>("SELECT id FROM repos LIMIT 1")
        .fetch_optional(&fx.pool)
        .await
        .expect("repos")
    {
        Some(id) => id.to_string(),
        None => {
            common::register_test_repo(&fx.app, &path.display().to_string(), &fx.cookie, &fx.csrf)
                .await
        }
    };
    std::mem::forget(dir);
    common::set_ticket_repo(
        &fx.app,
        &ticket_id.to_string(),
        &repo_id,
        &fx.cookie,
        &fx.csrf,
    )
    .await;
}

async fn work_set(fx: &TicketHttp) -> Uuid {
    ensure_repo(fx, fx.ticket_id).await;
    // One active run per ticket: finish the earlier work run first.
    sqlx::query(
        "UPDATE agent_runs SET status = 'succeeded', ended_at = now()
         WHERE ticket_id = $1 AND status IN ('queued', 'running')",
    )
    .bind(fx.ticket_id)
    .execute(&fx.pool)
    .await
    .expect("finish earlier runs");
    let run_id = start_ticket_run(fx, fx.agent_id).await;
    publish_html(
        fx,
        run_id,
        fx.agent_id,
        "Work mockup",
        Some(fx.ticket_id),
        None,
        "<!doctype html><title>A</title>",
        "<!doctype html><title>B</title>",
    )
    .await
}

async fn child_ticket(fx: &TicketHttp) -> Uuid {
    let board_id: Uuid = sqlx::query_scalar("SELECT board_id FROM tickets WHERE id = $1")
        .bind(fx.ticket_id)
        .fetch_one(&fx.pool)
        .await
        .expect("board");
    let child: Uuid =
        common::create_unplanned_ticket(&fx.app, &board_id.to_string(), &fx.cookie, &fx.csrf)
            .await
            .parse()
            .unwrap();
    sqlx::query("UPDATE tickets SET parent_ticket_id = $2 WHERE id = $1")
        .bind(child)
        .bind(fx.ticket_id)
        .execute(&fx.pool)
        .await
        .expect("link child");
    child
}

#[tokio::test]
async fn work_run_publish_does_not_supersede_the_plan_set() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, plan_set) = plan_review_with_set().await;
    let first = work_set(&fx).await;
    assert_eq!(set_json(&fx, plan_set).await["superseded"], false);
    assert_eq!(set_json(&fx, first).await["superseded"], false);

    let second = work_set(&fx).await;
    assert_eq!(set_json(&fx, first).await["superseded"], true);
    assert_eq!(set_json(&fx, second).await["superseded"], false);
    assert_eq!(set_json(&fx, plan_set).await["superseded"], false);
}

#[tokio::test]
async fn work_run_pick_becomes_the_effective_design() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, plan_set) = plan_review_with_set().await;
    let work = work_set(&fx).await;
    assert_eq!(pick(&fx, work, "b").await.status(), StatusCode::NO_CONTENT);
    let effective = design_svc(&fx)
        .effective_for_ticket(fx.ticket_id)
        .await
        .expect("effective")
        .expect("some design");
    assert_eq!(effective.set_id, work);
    assert_eq!(effective.selected.id, "b");
    assert_ne!(effective.set_id, plan_set);
}

#[tokio::test]
async fn plan_set_is_effective_when_no_work_run_set_was_picked() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, plan_set) = plan_review_with_set().await;
    let _work = work_set(&fx).await;
    let effective = design_svc(&fx)
        .effective_for_ticket(fx.ticket_id)
        .await
        .expect("effective")
        .expect("some design");
    assert_eq!(effective.set_id, plan_set);
    assert_eq!(effective.selected.id, "a");
}

#[tokio::test]
async fn effective_design_path_is_the_absolute_entry_file() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, _plan_set) = plan_review_with_set().await;
    let effective = design_svc(&fx)
        .effective_for_ticket(fx.ticket_id)
        .await
        .expect("effective")
        .expect("some design");
    let path = effective.entry_path.expect("path");
    assert!(path.is_absolute());
    assert!(path.ends_with("a/index.html"));
    assert!(path.is_file());
}

#[tokio::test]
async fn effective_design_path_uses_the_stored_dir() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    let artifacts = PathBuf::from(&fx.state.config.storage.artifacts_dir);
    let moved = format!("designs/moved-{set_id}");
    std::fs::rename(
        artifacts.join(format!("designs/{set_id}")),
        artifacts.join(&moved),
    )
    .expect("move folder");
    sqlx::query("UPDATE design_sets SET dir = $2 WHERE id = $1")
        .bind(set_id)
        .bind(&moved)
        .execute(&fx.pool)
        .await
        .expect("store dir");

    let path = design_svc(&fx)
        .effective_for_ticket(fx.ticket_id)
        .await
        .expect("effective")
        .expect("some design")
        .entry_path
        .expect("path");
    assert!(path.is_file(), "{path:?}");
    assert!(path.to_string_lossy().contains("moved-"), "{path:?}");
}

#[tokio::test]
async fn a_missing_design_folder_keeps_the_title_with_a_null_path() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, set_id) = plan_review_with_set().await;
    let child = child_ticket(&fx).await;
    ensure_repo(&fx, child).await;
    let run_id = RunService::new(&fx.pool)
        .start_run_for_agent(child, fx.agent_id, "work_on_ticket", Default::default())
        .await
        .expect("child run")
        .id;
    let design_plugin: Uuid =
        sqlx::query_scalar("SELECT id FROM plugins WHERE name = 'design' AND enabled")
            .fetch_one(&fx.pool)
            .await
            .expect("design plugin");
    let token = common::mint_test_token_with_plugins(
        &fx.state,
        run_id,
        ContextProfile::Full,
        vec![design_plugin],
    )
    .await;
    std::fs::remove_dir_all(
        PathBuf::from(&fx.state.config.storage.artifacts_dir).join(format!("designs/{set_id}")),
    )
    .expect("remove folder");

    let url = mcp_url(fx.app.clone()).await;
    for tool in ["ticket_get", "design__list"] {
        let body = tool_json(
            &rpc(
                &url,
                &token,
                "tools/call",
                json!({"name": tool, "arguments": {}}),
            )
            .await,
        );
        let design = &body["selectedDesign"];
        assert_eq!(design["setId"], set_id.to_string(), "{tool}: {body}");
        assert_eq!(design["id"], "a", "{tool}");
        assert_eq!(design["path"], Value::Null, "{tool}");
        assert!(design["optionTitle"].as_str().is_some(), "{tool}");
    }
}

#[tokio::test]
async fn sub_ticket_inherits_the_parent_selected_design() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let (fx, plan_set) = plan_review_with_set().await;
    let child = child_ticket(&fx).await;

    let res = fx
        .app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{child}/designs"),
            "",
            &fx.cookie,
            &fx.csrf,
        ))
        .await
        .expect("list");
    assert_eq!(res.status(), StatusCode::OK);
    let sets = common::json_body(res).await;
    let sets = sets.as_array().expect("array");
    assert_eq!(sets.len(), 1, "{sets:?}");
    assert_eq!(sets[0]["id"], plan_set.to_string());
    assert_eq!(sets[0]["inherited"], true);
    assert_eq!(sets[0]["choice"]["mode"], "read_only");
    // The parent's own view is not inherited.
    assert_eq!(set_json(&fx, plan_set).await["inherited"], false);

    ensure_repo(&fx, child).await;
    let run_id = RunService::new(&fx.pool)
        .start_run_for_agent(child, fx.agent_id, "work_on_ticket", Default::default())
        .await
        .expect("child run")
        .id;
    let design_plugin: Uuid =
        sqlx::query_scalar("SELECT id FROM plugins WHERE name = 'design' AND enabled")
            .fetch_one(&fx.pool)
            .await
            .expect("design plugin");
    let token = common::mint_test_token_with_plugins(
        &fx.state,
        run_id,
        ContextProfile::Full,
        vec![design_plugin],
    )
    .await;
    let url = mcp_url(fx.app.clone()).await;
    let names = tool_names(&rpc(&url, &token, "tools/list", json!({})).await);
    assert!(names.iter().any(|name| name == "design__list"), "{names:?}");
    let body = tool_json(
        &rpc(
            &url,
            &token,
            "tools/call",
            json!({"name":"ticket_get","arguments":{}}),
        )
        .await,
    );
    let design = &body["selectedDesign"];
    assert_eq!(design["setId"], plan_set.to_string());
    assert_eq!(design["id"], "a");
    assert_eq!(design["optionTitle"], "Quiet button after the last card");
    assert!(design["path"].as_str().unwrap().ends_with("a/index.html"));

    let listed = tool_json(
        &rpc(
            &url,
            &token,
            "tools/call",
            json!({"name":"design__list","arguments":{}}),
        )
        .await,
    );
    assert_eq!(listed["selectedDesign"]["setId"], plan_set.to_string());
    assert_eq!(listed["sets"][0]["inherited"], true);
    assert_eq!(listed["sets"][0]["superseded"], false);
}

#[tokio::test]
async fn sub_ticket_of_a_parent_without_designs_has_none() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = ticket_http().await;
    let child = child_ticket(&fx).await;
    let svc = design_svc(&fx);
    assert!(svc.effective_for_ticket(child).await.unwrap().is_none());
    assert!(svc.list_for_ticket(child).await.unwrap().is_empty());
}

#[tokio::test]
async fn slug_ids_and_prefixed_titles_get_labels_and_clean_titles() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    require_db!();

    let fx = unplanned_with_pm().await;
    let plans = planning(&fx);
    plans
        .insert_plan(fx.ticket_id, fx.agent_id, PICK_PLAN, None)
        .await
        .expect("first plan");
    plans.bind_latest_plan(fx.ticket_id).await.expect("bind");
    plans
        .ask_for_changes(fx.ticket_id, "Show me options.")
        .await
        .expect("ask");
    let run_id = active_plan_run(&fx).await;
    let svc = design_svc(&fx);
    let created = svc
        .create(
            "Home",
            CreateScope {
                ticket_id: Some(fx.ticket_id),
                chat_session_id: None,
                run_id,
                agent_id: fx.agent_id,
            },
        )
        .await
        .expect("create");
    for entry in ["option-a/index.html", "option-b/index.html"] {
        svc.write(created.id, run_id, entry, b"<!doctype html><title>x</title>")
            .await
            .expect("write");
    }
    let options = vec![
        DesignOption {
            id: "option-a".into(),
            title: "A: Quiet button after the last card".into(),
            entry: "option-a/index.html".into(),
        },
        DesignOption {
            id: "option-b".into(),
            title: "B: Full-width button with Show all".into(),
            entry: "option-b/index.html".into(),
        },
    ];
    svc.publish(created.id, run_id, options, "option-a")
        .await
        .expect("publish");
    plans
        .insert_plan(fx.ticket_id, fx.agent_id, PICK_PLAN, Some(run_id))
        .await
        .expect("plan with design");
    set_run_status(&fx, run_id, "succeeded").await;

    let set = set_json(&fx, created.id).await;
    assert_eq!(set["options"][0]["id"], "option-a");
    assert_eq!(set["options"][0]["title"], "A: Quiet button after the last card");
    assert_eq!(set["options"][0]["label"], "A");
    assert_eq!(set["options"][0]["displayTitle"], "Quiet button after the last card");
    assert_eq!(set["options"][1]["label"], "B");
    assert_eq!(set["options"][1]["displayTitle"], "Full-width button with Show all");
    assert_eq!(set["recommended"]["label"], "A");
    assert_eq!(set["recommended"]["displayTitle"], "Quiet button after the last card");

    let res = pick(&fx, created.id, "option-b").await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert_eq!(change_request(&fx).await.as_deref(), Some(B_NOTE));
    let set = set_json(&fx, created.id).await;
    assert_eq!(set["chosen"]["id"], "option-b");
    assert_eq!(set["chosen"]["label"], "B");
    assert_eq!(set["chosen"]["displayTitle"], "Full-width button with Show all");
    assert_eq!(set["selected"]["label"], "B");
}
