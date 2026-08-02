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

    let pm_id =
        common::create_agent_with_preset_key(&app, "pm", "PM Agent", &cookie, &csrf).await;
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
                    == Some("backend_engineer")
        },
    )
    .await;
    assert!(pm_ready["pendingAssignRecommendation"].is_object());

    common::assign_agent_to_ticket(&app, &ticket_id, &engineer_id, &cookie, &csrf).await;

    let after_assign = common::get_ticket(&app, &ticket_id, &cookie, &csrf).await;
    assert!(after_assign["pendingAssignRecommendation"].is_null());

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
