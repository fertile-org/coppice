mod common;

use coppice_server::domain::git_state::GitOutcome;
use coppice_server::services::human_review_service::HumanReviewService;
use uuid::Uuid;

const SHA: &str = "abc1234abc1234abc1234abc1234abc1234abc12";

async fn setup_ticket() -> (
    std::sync::Arc<coppice_server::AppState>,
    Uuid,
    Uuid,
    common::AgentTestEnv,
) {
    let (state, app, cookie, csrf, env) =
        common::bootstrap_and_login_with_state_and_workers("", |_| {}).await;
    let board_id = common::create_test_board(&app, &cookie, &csrf).await;
    let ticket_id = common::create_test_ticket(&app, &board_id, &cookie, &csrf).await;
    let pool = state.db.clone().expect("pool");
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("user");
    (state, Uuid::parse_str(&ticket_id).unwrap(), user_id, env)
}

#[tokio::test]
async fn reacceptance_clears_git_outcome() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, ticket_id, user_id, _env) = setup_ticket().await;
    let pool = state.db.clone().expect("pool");
    let svc = HumanReviewService::new(&pool);
    svc.record_acceptance(ticket_id, Some(user_id), SHA)
        .await
        .unwrap();
    assert!(svc
        .set_git_outcome(
            ticket_id,
            Some(GitOutcome::Conflict),
            Some("conflict in a.txt")
        )
        .await
        .unwrap());
    let review = svc.get(ticket_id).await.unwrap().unwrap();
    assert_eq!(review.git_outcome, Some(GitOutcome::Conflict));
    assert_eq!(
        review.git_outcome_reason.as_deref(),
        Some("conflict in a.txt")
    );
    assert!(!review.accepted_by_coppice);

    let again = svc.record_acceptance(ticket_id, None, SHA).await.unwrap();
    assert_eq!(again.git_outcome, None);
    assert_eq!(again.git_outcome_reason, None);
    assert_eq!(again.merged_base_branch, None);
    assert!(again.accepted_by_coppice);
    assert_eq!(svc.get(ticket_id).await.unwrap().unwrap().git_outcome, None);
}

#[tokio::test]
async fn set_git_outcome_reports_change() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let (state, ticket_id, user_id, _env) = setup_ticket().await;
    let pool = state.db.clone().expect("pool");
    let svc = HumanReviewService::new(&pool);
    svc.record_acceptance(ticket_id, Some(user_id), SHA)
        .await
        .unwrap();
    assert!(svc
        .set_git_outcome(ticket_id, Some(GitOutcome::MainDirty), None)
        .await
        .unwrap());
    assert!(!svc
        .set_git_outcome(ticket_id, Some(GitOutcome::MainDirty), None)
        .await
        .unwrap());
    assert!(svc.set_git_outcome(ticket_id, None, None).await.unwrap());
}
