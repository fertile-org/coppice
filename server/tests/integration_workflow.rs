mod common;

use axum::http::StatusCode;
use std::time::Duration;
use tower::ServiceExt;

#[tokio::test]
async fn human_mention_does_not_change_ticket_status() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, app, cookie, csrf) = common::bootstrap_and_login_with_state().await;
    let pool = state.db.as_ref().expect("db pool");

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    let _agent_id =
        common::create_agent_with_preset_key(&app, "pm", "PM Agent", &cookie, &csrf).await;

    let before = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(before.status(), StatusCode::OK);
    let before_body: serde_json::Value = common::json_body(before).await;
    assert_eq!(before_body["status"], "backlog");

    // Agent mode requires a repo; chat mode creates mention without changing status.
    let comment_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@pm please review the approach","mentionMode":"chat"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(comment_res.status(), StatusCode::CREATED);

    let agent_without_repo = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@pm run this","mentionMode":"agent"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(agent_without_repo.status(), StatusCode::BAD_REQUEST);

    let after = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::OK);
    let after_body: serde_json::Value = common::json_body(after).await;
    assert_eq!(after_body["status"], "backlog");
    assert!(after_body["substatus"].is_null());

    let mention_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ticket_mentions WHERE ticket_id = $1 AND status = 'pending'",
    )
    .bind(uuid::Uuid::parse_str(&ticket_id).unwrap())
    .fetch_one(pool)
    .await
    .expect("count mentions");
    assert_eq!(mention_count, 1);
}

#[tokio::test]
async fn final_approve_requires_wait_for_final_review() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (app, cookie, csrf) = common::bootstrap_and_login().await;
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;

    let reject = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/final-approve"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(reject.status(), StatusCode::BAD_REQUEST);

    let patch = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"wait_for_final_review"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let approve = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/final-approve"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);

    let approved: serde_json::Value = common::json_body(approve).await;
    assert_eq!(approved["status"], "done");
    assert!(approved["substatus"].is_null());

    let comments_res = app
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
    assert_eq!(comments_res.status(), StatusCode::OK);
    let comments: serde_json::Value = common::json_body(comments_res).await;
    let final_comment = comments
        .as_array()
        .and_then(|list| list.last())
        .expect("final approve comment");
    assert!(
        final_comment["body"]
            .as_str()
            .unwrap_or("")
            .contains("Final approval")
    );
}

#[tokio::test]
async fn assign_on_ready_moves_ticket_to_in_progress() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;

    let engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;

    let patch = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let assign_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/assign"),
            &format!(r#"{{"agentId":"{engineer_id}"}}"#),
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(assign_res.status(), StatusCode::OK);
    let assigned: serde_json::Value = common::json_body(assign_res).await;
    assert_eq!(assigned["status"], "in_progress");
}

#[tokio::test]
async fn ready_tech_lead_human_agent_run_keeps_agent_mode_contract_and_git_behavior() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let tech_lead_id = common::create_agent_with_preset_key(
        &app,
        "tech_lead",
        "Human Ready Tech Lead",
        &cookie,
        &csrf,
    )
    .await;
    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let comment = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/comments"),
            r#"{"body":"@human-ready-tech-lead inspect this in Agent mode","mentionMode":"agent"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(comment.status(), StatusCode::CREATED);

    let runs = common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "Ready Tech Lead Human Agent run",
        Duration::from_secs(30),
        |runs| {
            runs.iter().any(|run| {
                run["agentId"].as_str() == Some(tech_lead_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
                    && run["status"].as_str() == Some("succeeded")
            })
        },
    )
    .await;
    assert_eq!(runs.len(), 1);

    let context_profile = sqlx::query_scalar::<_, String>(
        "SELECT context_profile FROM agent_runs WHERE ticket_id = $1 AND agent_id = $2",
    )
    .bind(uuid::Uuid::parse_str(&ticket_id).expect("ticket UUID"))
    .bind(uuid::Uuid::parse_str(&tech_lead_id).expect("Tech Lead UUID"))
    .fetch_one(pool)
    .await
    .expect("Human Agent context profile");
    assert_eq!(context_profile, "human_agent");

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "ready");
    assert!(ticket["assigneeAgentId"].is_null());
    assert_eq!(ticket["description"], "details");

    let agent_comment = sqlx::query_scalar::<_, String>(
        r#"
        SELECT body FROM ticket_comments
        WHERE ticket_id = $1 AND author_type = 'agent' AND author_id = $2
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(uuid::Uuid::parse_str(&ticket_id).expect("ticket UUID"))
    .bind(uuid::Uuid::parse_str(&tech_lead_id).expect("Tech Lead UUID"))
    .fetch_one(pool)
    .await
    .expect("Human Agent Tech Lead comment");
    assert!(agent_comment.contains("## Verdict"));
    assert!(agent_comment.contains("**Git:**"));
    assert!(!agent_comment.contains("Recorded the technical approach"));
}

#[tokio::test]
async fn scope_b_mock_pipeline_reaches_final_review() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;

    let pm_id = common::create_agent_with_preset_key(&app, "pm", "PM Agent", &cookie, &csrf).await;
    let tech_lead_id =
        common::create_agent_with_preset_key(&app, "tech_lead", "Tech Lead Agent", &cookie, &csrf)
            .await;
    let engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &pm_id, &cookie, &csrf).await;

    let pm_ready = common::poll_ticket_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "PM run → ready + recommendation",
        Duration::from_secs(30),
        |ticket| {
            ticket["status"].as_str() == Some("ready")
                && ticket["pendingAssignRecommendation"]
                    .as_object()
                    .and_then(|rec| rec.get("recommendedAgentKey"))
                    .and_then(|key| key.as_str())
                    == Some("tech_lead")
        },
    )
    .await;
    assert!(pm_ready["pendingAssignRecommendation"].is_object());

    common::assign_agent_to_ticket(&app, &ticket_id, &tech_lead_id, &cookie, &csrf).await;

    let after_assign = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert!(after_assign["pendingAssignRecommendation"].is_null());

    common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "Tech Lead refinement succeeded and handed off to engineering",
        Duration::from_secs(30),
        |runs| {
            runs.iter().any(|run| {
                run["agentId"].as_str() == Some(tech_lead_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
                    && run["status"].as_str() == Some("succeeded")
            }) && runs.iter().any(|run| {
                run["agentId"].as_str() == Some(engineer_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
            })
        },
    )
    .await;

    common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "engineer blocked run",
        Duration::from_secs(30),
        |runs| {
            runs.iter().any(|run| {
                run["agentId"].as_str() == Some(engineer_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
                    && run["status"].as_str() == Some("blocked")
            })
        },
    )
    .await;

    common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "PM respond_to_mention succeeded",
        Duration::from_secs(30),
        |runs| {
            runs.iter().any(|run| {
                run["agentId"].as_str() == Some(pm_id.as_str())
                    && run["jobType"].as_str() == Some("respond_to_mention")
                    && run["status"].as_str() == Some("succeeded")
            })
        },
    )
    .await;

    common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "engineer resume succeeded",
        Duration::from_secs(30),
        |runs| {
            let engineer_work_runs: Vec<_> = runs
                .iter()
                .filter(|run| {
                    run["agentId"].as_str() == Some(engineer_id.as_str())
                        && run["jobType"].as_str() == Some("work_on_ticket")
                })
                .collect();
            engineer_work_runs.len() >= 2
                && engineer_work_runs
                    .iter()
                    .any(|run| run["status"].as_str() == Some("succeeded"))
        },
    )
    .await;

    let final_ticket = common::poll_ticket_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "wait_for_final_review",
        Duration::from_secs(120),
        |ticket| ticket["status"].as_str() == Some("wait_for_final_review"),
    )
    .await;
    assert_eq!(final_ticket["status"], "wait_for_final_review");

    let approve = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/final-approve"),
            "{}",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);

    let approved: serde_json::Value = common::json_body(approve).await;
    assert_eq!(approved["status"], "done");
    assert!(approved["substatus"].is_null());
}

#[tokio::test]
async fn ready_tech_lead_auto_handoff_queues_exactly_one_implementer_run() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let tech_lead_id =
        common::create_agent_with_preset_key(&app, "tech_lead", "Ready Tech Lead", &cookie, &csrf)
            .await;
    let engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Ready Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    common::assign_agent_to_ticket(&app, &ticket_id, &tech_lead_id, &cookie, &csrf).await;

    let runs = common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "Ready Tech Lead handoff and implementer run start",
        Duration::from_secs(30),
        |runs| {
            runs.iter().any(|run| {
                run["agentId"].as_str() == Some(tech_lead_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
                    && run["status"].as_str() == Some("succeeded")
            }) && runs.iter().any(|run| {
                run["agentId"].as_str() == Some(engineer_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
            })
        },
    )
    .await;

    assert_eq!(
        runs.iter()
            .filter(|run| {
                run["agentId"].as_str() == Some(engineer_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
            })
            .count(),
        1,
        "technical refinement must queue exactly one implementer work run"
    );
    assert!(!runs.iter().any(|run| {
        run["agentId"].as_str() == Some(engineer_id.as_str())
            && run["jobType"].as_str() == Some("respond_to_mention")
    }));

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "in_progress");
    assert_eq!(ticket["assigneeAgentId"], engineer_id);

    let tech_lead_comment = sqlx::query_scalar::<_, String>(
        r#"
        SELECT body FROM ticket_comments
        WHERE ticket_id = $1 AND author_type = 'agent' AND author_id = $2
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(uuid::Uuid::parse_str(&ticket_id).expect("ticket UUID"))
    .bind(uuid::Uuid::parse_str(&tech_lead_id).expect("Tech Lead UUID"))
    .fetch_one(pool)
    .await
    .expect("Tech Lead refinement comment");
    assert!(tech_lead_comment.contains("technical approach"));
    assert!(!tech_lead_comment.contains("**Git:**"));
}

#[tokio::test]
async fn ready_tech_lead_manual_handoff_persists_recommendation_and_starts_nobody() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_worker_config(1, |config| {
            config.workflow.auto_assign.ready = Some(false);
        })
        .await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let tech_lead_id = common::create_agent_with_preset_key(
        &app,
        "tech_lead",
        "Manual Handoff Tech Lead",
        &cookie,
        &csrf,
    )
    .await;
    let engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Manual Handoff Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    common::assign_agent_to_ticket(&app, &ticket_id, &tech_lead_id, &cookie, &csrf).await;

    common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "manual Ready Tech Lead handoff completion",
        Duration::from_secs(30),
        |runs| {
            runs.iter().any(|run| {
                run["agentId"].as_str() == Some(tech_lead_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
                    && run["status"].as_str() == Some("succeeded")
            })
        },
    )
    .await;

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "ready");
    assert_eq!(ticket["assigneeAgentId"], tech_lead_id);
    assert_eq!(
        ticket["pendingAssignRecommendation"]["recommendedAgentKey"],
        "backend_engineer"
    );

    let engineer_run_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM agent_runs WHERE ticket_id = $1 AND agent_id = $2",
    )
    .bind(uuid::Uuid::parse_str(&ticket_id).expect("ticket UUID"))
    .bind(uuid::Uuid::parse_str(&engineer_id).expect("engineer UUID"))
    .fetch_one(pool)
    .await
    .expect("count manual handoff engineer runs");
    assert_eq!(engineer_run_count, 0);
}

#[tokio::test]
async fn ready_tech_lead_invalid_handoffs_stay_ready_and_start_nobody() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let tech_lead_id = common::create_agent_with_preset_key(
        &app,
        "tech_lead",
        "Invalid Handoff Tech Lead",
        &cookie,
        &csrf,
    )
    .await;
    let engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Invalid Handoff Engineer",
        &cookie,
        &csrf,
    )
    .await;
    let tech_lead_uuid = uuid::Uuid::parse_str(&tech_lead_id).expect("Tech Lead UUID");
    let engineer_uuid = uuid::Uuid::parse_str(&engineer_id).expect("engineer UUID");

    for (fixture_key, disable_engineer, expected_reason) in [
        ("tech_lead_missing", false, "did not return `assignTo`"),
        ("tech_lead_unknown", false, "unknown or disabled"),
        ("tech_lead_disabled", true, "unknown or disabled"),
    ] {
        sqlx::query("UPDATE agents SET preset_source = $2 WHERE id = $1")
            .bind(tech_lead_uuid)
            .bind(fixture_key)
            .execute(pool)
            .await
            .expect("select invalid Tech Lead fixture");
        sqlx::query("UPDATE agents SET enabled = $2 WHERE id = $1")
            .bind(engineer_uuid)
            .bind(!disable_engineer)
            .execute(pool)
            .await
            .expect("configure implementer availability");

        let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
        common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
        let ready = app
            .clone()
            .oneshot(common::json_request(
                "PATCH",
                &format!("/api/tickets/{ticket_id}/status"),
                r#"{"status":"ready"}"#,
                &cookie,
                &csrf,
            ))
            .await
            .unwrap();
        assert_eq!(ready.status(), StatusCode::OK);
        common::assign_agent_to_ticket(&app, &ticket_id, &tech_lead_id, &cookie, &csrf).await;

        common::poll_runs_until_count(
            &app,
            &ticket_id,
            &cookie,
            &csrf,
            "invalid Ready Tech Lead handoff completed",
            Duration::from_secs(30),
            |runs| {
                runs.iter().any(|run| {
                    run["agentId"].as_str() == Some(tech_lead_id.as_str())
                        && run["jobType"].as_str() == Some("work_on_ticket")
                        && run["status"].as_str() == Some("succeeded")
                })
            },
        )
        .await;

        let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
        assert_eq!(ticket["status"], "ready", "fixture {fixture_key}");
        assert_eq!(
            ticket["assigneeAgentId"], tech_lead_id,
            "fixture {fixture_key}"
        );
        assert!(ticket["pendingAssignRecommendation"].is_null());

        let target_run_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_runs WHERE ticket_id = $1 AND agent_id != $2",
        )
        .bind(uuid::Uuid::parse_str(&ticket_id).expect("ticket UUID"))
        .bind(tech_lead_uuid)
        .fetch_one(pool)
        .await
        .expect("count invalid handoff target runs");
        assert_eq!(target_run_count, 0, "fixture {fixture_key}");

        let system_comment = sqlx::query_scalar::<_, String>(
            r#"
            SELECT body FROM ticket_comments
            WHERE ticket_id = $1 AND author_type = 'system'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(uuid::Uuid::parse_str(&ticket_id).expect("ticket UUID"))
        .fetch_one(pool)
        .await
        .expect("invalid handoff system comment");
        assert!(system_comment.contains("Technical refinement handoff is incomplete"));
        assert!(system_comment.contains(expected_reason));
        assert!(system_comment.contains("remains in Ready"));
        assert!(system_comment.contains("enabled implementer"));
    }
}

#[tokio::test]
async fn ready_tech_lead_clarification_resumes_same_refinement_contract() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;
    let pool = state.db.as_ref().expect("db pool");
    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let pm_id =
        common::create_agent_with_preset_key(&app, "pm", "Clarification PM", &cookie, &csrf).await;
    let tech_lead_id = common::create_agent_with_preset_key(
        &app,
        "tech_lead",
        "Clarification Tech Lead",
        &cookie,
        &csrf,
    )
    .await;
    let engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Clarification Engineer",
        &cookie,
        &csrf,
    )
    .await;
    sqlx::query("UPDATE agents SET preset_source = 'tech_lead_clarification' WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&tech_lead_id).expect("Tech Lead UUID"))
        .execute(pool)
        .await
        .expect("select Tech Lead clarification fixtures");

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    let ready = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"ready"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    common::assign_agent_to_ticket(&app, &ticket_id, &tech_lead_id, &cookie, &csrf).await;

    let runs = common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "Tech Lead clarification response and resumed refinement",
        Duration::from_secs(45),
        |runs| {
            let tech_lead_work = runs
                .iter()
                .filter(|run| {
                    run["agentId"].as_str() == Some(tech_lead_id.as_str())
                        && run["jobType"].as_str() == Some("work_on_ticket")
                })
                .collect::<Vec<_>>();
            tech_lead_work.len() == 2
                && tech_lead_work
                    .iter()
                    .any(|run| run["status"].as_str() == Some("blocked"))
                && tech_lead_work
                    .iter()
                    .any(|run| run["status"].as_str() == Some("succeeded"))
                && runs.iter().any(|run| {
                    run["agentId"].as_str() == Some(pm_id.as_str())
                        && run["jobType"].as_str() == Some("respond_to_mention")
                        && run["status"].as_str() == Some("succeeded")
                })
                && runs.iter().any(|run| {
                    run["agentId"].as_str() == Some(engineer_id.as_str())
                        && run["jobType"].as_str() == Some("work_on_ticket")
                })
        },
    )
    .await;

    assert_eq!(
        runs.iter()
            .filter(|run| {
                run["agentId"].as_str() == Some(tech_lead_id.as_str())
                    && run["jobType"].as_str() == Some("work_on_ticket")
            })
            .count(),
        2
    );
    let clarification_round =
        sqlx::query_scalar::<_, i32>("SELECT clarification_round FROM tickets WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&ticket_id).expect("ticket UUID"))
            .fetch_one(pool)
            .await
            .expect("clarification round");
    assert_eq!(clarification_round, 1);

    let tech_lead_comments = sqlx::query_scalar::<_, String>(
        r#"
        SELECT body FROM ticket_comments
        WHERE ticket_id = $1 AND author_type = 'agent' AND author_id = $2
        ORDER BY created_at ASC
        "#,
    )
    .bind(uuid::Uuid::parse_str(&ticket_id).expect("ticket UUID"))
    .bind(uuid::Uuid::parse_str(&tech_lead_id).expect("Tech Lead UUID"))
    .fetch_all(pool)
    .await
    .expect("Tech Lead comments");
    assert_eq!(tech_lead_comments.len(), 2);
    assert!(tech_lead_comments
        .iter()
        .all(|comment| !comment.contains("**Git:**")));
    assert!(tech_lead_comments
        .iter()
        .any(|comment| comment.contains("clarified technical approach")));
}

#[tokio::test]
async fn scope_continued_run_keeps_in_progress() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("backend_engineer/continued", |_| {})
            .await;

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &engineer_id, &cookie, &csrf).await;

    let patch = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"in_progress"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let run_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/run-agent"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(run_res.status(), StatusCode::CREATED);

    common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "continued run succeeded",
        Duration::from_secs(15),
        |runs| {
            runs.iter().any(|run| {
                run["jobType"].as_str() == Some("work_on_ticket")
                    && run["status"].as_str() == Some("succeeded")
            })
        },
    )
    .await;

    let ticket = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert_eq!(ticket["status"], "in_progress");
    assert!(ticket["substatus"].is_null());

    let comments_res = app
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
    assert_eq!(comments_res.status(), StatusCode::OK);
    let comments: serde_json::Value = common::json_body(comments_res).await;
    let progress_comment = comments
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["authorType"] == "agent" && c["intent"] == "progress_update")
        .expect("agent progress_update comment");
    assert!(
        progress_comment["body"]
            .as_str()
            .unwrap()
            .contains("TmuxStream")
    );
}

#[tokio::test]
async fn scope_pm_split_pending() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_state_and_workers("pm/split_pending", |_| {}).await;

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;
    let pm_id =
        common::create_agent_with_preset_key(&app, "pm", "PM Agent", &cookie, &csrf).await;

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;
    common::assign_agent_to_ticket(&app, &ticket_id, &pm_id, &cookie, &csrf).await;

    let run_res = app
        .clone()
        .oneshot(common::json_request(
            "POST",
            &format!("/api/tickets/{ticket_id}/run-agent"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(run_res.status(), StatusCode::CREATED);

    let ticket = common::poll_ticket_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "pending split recommendation",
        Duration::from_secs(15),
        |t| t["pendingSplitRecommendation"].is_object(),
    )
    .await;

    let splits = ticket["pendingSplitRecommendation"]["splits"]
        .as_array()
        .expect("splits array");
    assert_eq!(splits.len(), 2);
    assert_eq!(splits[0]["title"], "Add retry logic to API client");
    assert_eq!(splits[1]["title"], "Add circuit breaker dashboard");

    let children_res = app
        .clone()
        .oneshot(common::json_request(
            "GET",
            &format!("/api/tickets/{ticket_id}/children"),
            "",
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(children_res.status(), StatusCode::OK);
    let children: serde_json::Value = common::json_body(children_res).await;
    assert!(children.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn qc_defect_handoff_returns_to_engineer_without_committing() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }

    let (_state, app, cookie, csrf, _env) =
        common::bootstrap_and_login_with_auto_start_workers().await;

    let project_id = common::create_test_project(&app, &cookie, &csrf).await;
    let (_git_dir, local_path) = common::create_temp_git_checkout();
    let repo_id =
        common::register_test_repo(&app, &local_path.display().to_string(), &cookie, &csrf).await;

    let engineer_id = common::create_agent_with_preset_key(
        &app,
        "backend_engineer",
        "Backend Engineer",
        &cookie,
        &csrf,
    )
    .await;
    let qc_id =
        common::create_agent_with_preset_key(&app, "qc", "QC Agent", &cookie, &csrf).await;

    let ticket_id = common::create_test_ticket(&app, &project_id, &cookie, &csrf).await;
    common::set_ticket_repo(&app, &ticket_id, &repo_id, &cookie, &csrf).await;

    // Place the ticket in QA and hand it to QC.
    let patch = app
        .clone()
        .oneshot(common::json_request(
            "PATCH",
            &format!("/api/tickets/{ticket_id}/status"),
            r#"{"status":"in_qa"}"#,
            &cookie,
            &csrf,
        ))
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);
    common::assign_agent_to_ticket(&app, &ticket_id, &qc_id, &cookie, &csrf).await;

    // QC run fires (autostart), returns a defect with mentionAgents: ["backend_engineer"].
    // The workflow handoff must return the ticket to In Progress, assign the engineer,
    // and enqueue a work_on_ticket fix run.
    let runs = common::poll_runs_until_count(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "QC defect → engineer work_on_ticket enqueued",
        Duration::from_secs(30),
        |runs| {
            runs.iter().any(|r| {
                r["agentId"].as_str() == Some(qc_id.as_str())
                    && r["jobType"].as_str() == Some("work_on_ticket")
                    && r["status"].as_str() == Some("succeeded")
            }) && runs.iter().any(|r| {
                r["agentId"].as_str() == Some(engineer_id.as_str())
                    && r["jobType"].as_str() == Some("work_on_ticket")
            })
        },
    )
    .await;

    // QC run completed; engineer fix run was enqueued by the handoff.
    assert!(runs.iter().any(|r| {
        r["agentId"].as_str() == Some(qc_id.as_str())
            && r["jobType"].as_str() == Some("work_on_ticket")
            && r["status"].as_str() == Some("succeeded")
    }));
    assert_eq!(
        runs.iter()
            .filter(|r| {
                r["agentId"].as_str() == Some(engineer_id.as_str())
                    && r["jobType"].as_str() == Some("work_on_ticket")
            })
            .count(),
        1,
        "QC handoff must enqueue exactly one engineer work run"
    );
    assert!(
        !runs.iter().any(|r| {
            r["agentId"].as_str() == Some(engineer_id.as_str())
                && r["jobType"].as_str() == Some("respond_to_mention")
        }),
        "QC handoff must not also enqueue an engineer response run"
    );

    // Ticket left in_qa and is now in_progress owned by the implementing engineer.
    let ticket = common::poll_ticket_until(
        &app,
        &ticket_id,
        &cookie,
        &csrf,
        "in_progress owned by backend_engineer",
        Duration::from_secs(30),
        |t| {
            t["status"].as_str() == Some("in_progress")
                && t["assigneeAgentId"].as_str() == Some(engineer_id.as_str())
        },
    )
    .await;
    assert_eq!(ticket["status"], "in_progress");
    assert_eq!(ticket["assigneeAgentId"], engineer_id);

    // QC-authored comment must not present a git commit footer: QC is verification-only
    // and its (potential) edits must never be committed as the implementation.
    let comments_res = app
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
    assert_eq!(comments_res.status(), StatusCode::OK);
    let comments: serde_json::Value = common::json_body(comments_res).await;
    let qc_comment = comments
        .as_array()
        .expect("comments array")
        .iter()
        .find(|c| c["authorId"].as_str() == Some(qc_id.as_str()))
        .expect("QC-authored comment");
    let body = qc_comment["body"].as_str().expect("qc comment body");
    assert!(body.contains("Defects found"), "QC defect verdict present: {body}");
    assert!(
        !body.contains("**Git:**"),
        "QC comment must not carry a git footer: {body}"
    );
    assert!(
        !body.contains("committed"),
        "QC comment must not claim a commit: {body}"
    );
}
