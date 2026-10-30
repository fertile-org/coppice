mod common;

use axum::http::StatusCode;
use axum::Router;
use coppice_server::domain::context_profile::ContextProfile;
use coppice_server::mcp::token::{NewRunToolScope, TokenService};
use coppice_server::services::design_service::{
    CreateScope, DesignError, DesignOption, DesignService,
};
use coppice_server::services::run_service::RunService;
use coppice_server::AppState;
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
