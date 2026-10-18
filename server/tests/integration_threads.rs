mod common;

use axum::http::StatusCode;
use tower::ServiceExt;
use uuid::Uuid;

async fn get(app: &axum::Router, path: &str, cookie: &str, csrf: &str) -> axum::response::Response {
    app.clone()
        .oneshot(common::json_request("GET", path, "", cookie, csrf))
        .await
        .unwrap()
}

#[tokio::test]
async fn ticket_comments_page_newest_first() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.clone().unwrap();
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    sqlx::query(
        r#"
        INSERT INTO ticket_comments (id, ticket_id, author_type, body, intent, created_at)
        SELECT gen_random_uuid(), $1, 'human', 'comment ' || n, 'progress_update',
               now() - make_interval(secs => 1000 - n)
        FROM generate_series(1, 450) AS n
        "#,
    )
    .bind(Uuid::parse_str(&ticket_id).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let mut seen = Vec::new();
    let mut path = format!("/api/tickets/{ticket_id}/comments");
    loop {
        let res = get(&app, &path, &cookie, &csrf).await;
        assert_eq!(res.status(), StatusCode::OK);
        let next = res
            .headers()
            .get("x-next-before")
            .map(|value| value.to_str().unwrap().to_string());
        let page = common::json_body(res).await;
        let page = page.as_array().unwrap();
        assert!(page.len() <= 200);
        seen.extend(page.iter().map(|c| c["body"].as_str().unwrap().to_string()));
        match next {
            Some(before) => path = format!("/api/tickets/{ticket_id}/comments?before={before}"),
            None => break,
        }
    }
    let expected: Vec<String> = (1..=450).rev().map(|n| format!("comment {n}")).collect();
    assert_eq!(seen, expected);

    let small = get(&app, &format!("/api/tickets/{ticket_id}/comments?limit=5"), &cookie, &csrf).await;
    assert!(small.headers().contains_key("x-next-before"));
    assert_eq!(common::json_body(small).await.as_array().unwrap().len(), 5);
    let bad = get(
        &app,
        &format!("/api/tickets/{ticket_id}/comments?before={}", Uuid::new_v4()),
        &cookie,
        &csrf,
    )
    .await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
}

async fn chat_with_messages(count: i64, body: &str) -> (std::sync::Arc<coppice_server::AppState>, axum::Router, String, String, Uuid) {
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let agent_id =
        common::create_agent_with_preset_key(&app, "backend_engineer", "BE", &cookie, &csrf).await;
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
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let session = Uuid::parse_str(common::json_body(created).await["id"].as_str().unwrap()).unwrap();
    sqlx::query(
        r#"
        INSERT INTO chat_messages (id, session_id, seq, role, body)
        SELECT gen_random_uuid(), $1, n, CASE WHEN n % 2 = 0 THEN 'human' ELSE 'agent' END,
               $3 || ' ' || n
        FROM generate_series(0, $2 - 1) AS n
        "#,
    )
    .bind(session)
    .bind(count)
    .bind(body)
    .execute(state.db.as_ref().unwrap())
    .await
    .unwrap();
    (state, app, cookie, csrf, session)
}

#[tokio::test]
async fn opening_a_long_chat_loads_one_page() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    let (_state, app, cookie, csrf, session) = chat_with_messages(5000, "message").await;

    let res = get(&app, &format!("/api/chat/sessions/{session}/messages"), &cookie, &csrf).await;
    assert_eq!(res.status(), StatusCode::OK);
    let page = common::json_body(res).await;
    let messages = page["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 100);
    assert_eq!(messages[0]["seq"], 4900);
    assert_eq!(messages[99]["seq"], 4999);
    assert_eq!(page["olderBeforeSeq"], 4900);

    let older = get(
        &app,
        &format!("/api/chat/sessions/{session}/messages?beforeSeq=4900&limit=50"),
        &cookie,
        &csrf,
    )
    .await;
    let older = common::json_body(older).await;
    let messages = older["messages"].as_array().unwrap();
    assert_eq!(messages[0]["seq"], 4850);
    assert_eq!(messages[49]["seq"], 4899);

    let first = get(
        &app,
        &format!("/api/chat/sessions/{session}/messages?beforeSeq=10"),
        &cookie,
        &csrf,
    )
    .await;
    let first = common::json_body(first).await;
    assert_eq!(first["messages"].as_array().unwrap().len(), 10);
    assert!(first.get("olderBeforeSeq").is_none());
}

#[tokio::test]
async fn the_model_transcript_matches_reading_the_whole_chat() {
    use coppice_server::services::chat_service::ChatService;
    use coppice_server::services::chat_transcript::bound_rendered_transcript;
    let _guard = common::DB_TEST_LOCK.lock().await;
    let body = "word ".repeat(40);
    let (state, _app, _cookie, _csrf, session) = chat_with_messages(3000, body.trim()).await;
    let blocks: Vec<String> = (0..3000)
        .map(|n| {
            let role = if n % 2 == 0 { "Human" } else { "Agent" };
            format!("### {role}\n\n{} {n}\n\n", body.trim())
        })
        .collect();
    let service = ChatService::new(state.db.as_ref().unwrap());
    for budget in [50, 300, 5_000, 40_000] {
        let paged = service.format_model_transcript(session, budget).await.unwrap();
        assert_eq!(paged, bound_rendered_transcript(&blocks, budget), "budget {budget}");
    }
}
