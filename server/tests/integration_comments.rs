mod common;

use axum::http::StatusCode;
use tower::ServiceExt;

async fn setup_ticket_with_repo_and_agents() -> (
    tempfile::TempDir,
    sqlx::PgPool,
    axum::Router,
    String,
    String,
    String,
) {
    let (git_dir, local_path) = common::create_temp_git_checkout();
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool").clone();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::create_agent_with_preset_key(&app, "pm", "PM Agent", &cookie, &csrf).await;
    common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;
    (git_dir, pool, app, cookie, csrf, ticket_id)
}

#[tokio::test]
async fn upload_attachment_and_create_comment() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;

    let upload = app
        .clone()
        .oneshot(common::multipart_request(
            "/api/attachments",
            "notes.txt",
            "text/plain",
            "hello attachment",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::CREATED);

    let attachment: serde_json::Value = common::json_body(upload).await;
    let attachment_id = attachment["id"].as_str().unwrap();
    assert_eq!(attachment["filename"], "notes.txt");
    assert_eq!(attachment["contentType"], "text/plain");
    assert_eq!(attachment["sizeBytes"], 16);

    let create_comment = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            &format!(r#"{{"body":"See attached","attachmentIds":["{attachment_id}"]}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(create_comment.status(), StatusCode::CREATED);

    let list = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/comments"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);

    let comments: serde_json::Value = common::json_body(list).await;
    assert_eq!(comments.as_array().unwrap().len(), 1);
    assert_eq!(comments[0]["body"], "See attached");
    assert_eq!(comments[0]["authorType"], "human");
    assert_eq!(
        comments[0]["attachmentIds"][0].as_str().unwrap(),
        attachment_id
    );
    assert_eq!(comments[0]["attachments"][0]["filename"], "notes.txt");
    assert_eq!(comments[0]["attachments"][0]["contentType"], "text/plain");
}

#[tokio::test]
async fn mention_chat_mode_starts_respond_to_mention_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, pool, app, cookie, csrf, ticket_id) = setup_ticket_with_repo_and_agents().await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@pm hello","mentionMode":"chat"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let body: serde_json::Value = common::json_body(res).await;
    let started_runs = body["startedRuns"].as_array().unwrap();
    assert_eq!(started_runs.len(), 1);
    assert_eq!(started_runs[0]["agentKey"], "pm-agent");

    let run_id = uuid::Uuid::parse_str(started_runs[0]["runId"].as_str().unwrap()).unwrap();
    let row: (String, String) =
        sqlx::query_as("SELECT job_type, context_profile FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .expect("run row");
    assert_eq!(row.0, "respond_to_mention");
    assert_eq!(row.1, "human_chat");
}

#[tokio::test]
async fn mention_agent_mode_starts_work_on_ticket_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, pool, app, cookie, csrf, ticket_id) = setup_ticket_with_repo_and_agents().await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@backend_engineer fix","mentionMode":"agent"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let body: serde_json::Value = common::json_body(res).await;
    let started_runs = body["startedRuns"].as_array().unwrap();
    assert_eq!(started_runs.len(), 1);
    assert_eq!(started_runs[0]["agentKey"], "backend-engineer");

    let run_id = uuid::Uuid::parse_str(started_runs[0]["runId"].as_str().unwrap()).unwrap();
    let row: (String, String) =
        sqlx::query_as("SELECT job_type, context_profile FROM agent_runs WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .expect("run row");
    assert_eq!(row.0, "work_on_ticket");
    assert_eq!(row.1, "human_agent");
}

#[tokio::test]
async fn full_name_mention_selects_agent_when_preset_matches_another_agent() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, pool, app, cookie, csrf, ticket_id) = setup_ticket_with_repo_and_agents().await;
    common::create_agent_with_preset_key(&app, "pm", "PM Opencode", &cookie, &csrf).await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@pm-opencode please inspect","mentionMode":"chat"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let body: serde_json::Value = common::json_body(res).await;
    let started_runs = body["startedRuns"].as_array().unwrap();
    assert_eq!(started_runs.len(), 1);
    assert_eq!(started_runs[0]["agentKey"], "pm-opencode");

    let mentioned_name: String = sqlx::query_scalar(
        r#"
        SELECT agents.name
        FROM ticket_mentions
        JOIN agents ON agents.id = ticket_mentions.mentioned_agent_id
        WHERE ticket_mentions.ticket_id = $1
        "#,
    )
    .bind(uuid::Uuid::parse_str(&ticket_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("mentioned agent name");
    assert_eq!(mentioned_name, "PM Opencode");
}

#[tokio::test]
async fn mention_multiple_agents_returns_bad_request() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_git_dir, _pool, app, cookie, csrf, ticket_id) = setup_ticket_with_repo_and_agents().await;

    let res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@pm @backend_engineer please help"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
