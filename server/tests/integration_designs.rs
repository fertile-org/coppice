mod common;

use coppice_server::services::design_service::{CreateScope, DesignOption, DesignService};
use coppice_server::services::run_service::RunService;
use std::fs;
use std::path::{Path, PathBuf};
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

    let html_a = b"<!doctype html><title>A</title>design-preview-marker-a";
    let html_b = b"<!doctype html><title>B</title>design-preview-marker-b";
    svc.write(created.id, "a/index.html", html_a)
        .await
        .expect("write a");
    svc.write(created.id, "b/index.html", html_b)
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
