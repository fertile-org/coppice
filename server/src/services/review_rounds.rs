//! Review-round cap.
//!
//! A reviewer may send work back a limited number of times. From round 2 the
//! prompt is only the diff since the previous round plus earlier findings.
//! At the cap the reviewer posts one comment and the ticket waits for a person.

use std::path::Path;

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::copy::review::{
    review_didnt_pass, round_cap_comment, REVIEW_ROUND_EARLIER_FINDINGS, REVIEW_ROUND_INTRO,
    REVIEW_ROUND_NEW_DIFF, REVIEW_ROUND_NO_DIFF, REVIEW_ROUND_NO_FINDINGS,
};
use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::substatus::TicketStatus;
use crate::domain::ticket::status_to_str;
use crate::domain::workflow::{is_in_review_review_task, TransitionAction};
use crate::providers::AgentRunResult;
use crate::services::comment_service::{CommentError, CommentService};
use crate::services::review_capacity::ReviewCapacityService;
use crate::services::ticket_service::TicketService;

/// How many review rounds may ask for another fix. The round that reaches
/// this number stops the loop.
pub const DEFAULT_REVIEW_ROUND_CAP: i32 = 3;

const WORK_ON_TICKET: &str = "work_on_ticket";

#[derive(Debug, thiserror::Error)]
pub enum ReviewRoundError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Message(String),
}

impl From<CommentError> for ReviewRoundError {
    fn from(err: CommentError) -> Self {
        match err {
            CommentError::Database(err) => ReviewRoundError::Database(err),
            other => ReviewRoundError::Message(other.to_string()),
        }
    }
}

impl From<crate::services::ticket_service::TicketError> for ReviewRoundError {
    fn from(err: crate::services::ticket_service::TicketError) -> Self {
        use crate::services::ticket_service::TicketError;
        match err {
            TicketError::Database(err) => ReviewRoundError::Database(err),
            other => ReviewRoundError::Message(other.to_string()),
        }
    }
}

impl From<ReviewRoundError> for crate::services::run_service::RunError {
    fn from(err: ReviewRoundError) -> Self {
        use crate::services::run_service::RunError;
        match err {
            ReviewRoundError::Database(err) => RunError::Database(err),
            ReviewRoundError::Message(message) => RunError::Validation(message),
        }
    }
}

impl From<crate::services::review_capacity::ReviewCapacityError> for ReviewRoundError {
    fn from(err: crate::services::review_capacity::ReviewCapacityError) -> Self {
        use crate::services::review_capacity::ReviewCapacityError;
        match err {
            ReviewCapacityError::Database(err) => ReviewRoundError::Database(err),
            ReviewCapacityError::Message(message) => ReviewRoundError::Message(message),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRoundState {
    pub round: i32,
    pub anchor_sha: Option<String>,
    pub findings: String,
    pub escalated: bool,
}

/// `true` when the next change-request round would pass the cap.
pub fn hits_round_cap(stored_round: i32, cap: i32) -> bool {
    stored_round.saturating_add(1) >= cap
}

/// `true` when another reviewer run must not start.
pub fn must_stop_before_run(stored_round: i32, cap: i32) -> bool {
    stored_round >= cap
}

/// A verification handoff that would start another fix.
pub fn is_review_change_request(action: &TransitionAction) -> bool {
    action.new_status == Some(TicketStatus::InProgress)
        && action
            .enqueue_jobs
            .iter()
            .any(|job| job.job_type == WORK_ON_TICKET)
}

/// Validation signal for a start that the round cap already stopped.
pub fn round_cap_signal() -> String {
    review_didnt_pass(DEFAULT_REVIEW_ROUND_CAP)
}

pub fn is_round_cap_signal(message: &str) -> bool {
    message == round_cap_signal()
}

/// Open findings recorded on the latest `Round N:` block.
pub fn last_round_open_findings(stored: &str) -> Vec<String> {
    let mut last_block = "";
    for block in stored.split("\n\n") {
        let block = block.trim();
        if block.starts_with("Round ") {
            last_block = block;
        }
    }
    last_block
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("- ")
                .map(str::trim)
                .filter(|finding| !finding.is_empty())
                .map(str::to_string)
        })
        .collect()
}

/// Replace a send-back with Wait for Human Review and the one comment.
pub fn apply_round_cap(
    action: &mut TransitionAction,
    comment_body: &mut String,
    findings: &[String],
) {
    action.new_status = Some(TicketStatus::WaitForFinalReview);
    action.new_assignee_id = Some(None);
    action.pending_recommendation = Some(None);
    action.substatus = Some(None);
    action.substatus_metadata = Some(None);
    action.enqueue_jobs.clear();
    action.system_comments.clear();
    *comment_body = round_cap_comment(DEFAULT_REVIEW_ROUND_CAP, findings);
}

/// Prompt section for round 2 and later. Round 1 returns `None`.
pub fn reviewer_round_section(round: i32, diff: &str, findings: &str) -> Option<String> {
    if round < 2 {
        return None;
    }
    let diff = if diff.trim().is_empty() {
        REVIEW_ROUND_NO_DIFF
    } else {
        diff.trim()
    };
    let findings = if findings.trim().is_empty() {
        REVIEW_ROUND_NO_FINDINGS
    } else {
        findings.trim()
    };
    Some(format!(
        "# Review round {round}\n\n{REVIEW_ROUND_INTRO}\n\n## {REVIEW_ROUND_NEW_DIFF}\n\n```\n{diff}\n```\n\n## {REVIEW_ROUND_EARLIER_FINDINGS}\n\n{findings}\n"
    ))
}

pub fn append_findings(existing: &str, round: i32, summary: &str, blockers: &[String]) -> String {
    let mut block = format!("Round {round}: {}", summary.trim());
    for blocker in blockers {
        let blocker = blocker.trim();
        if !blocker.is_empty() {
            block.push_str("\n- ");
            block.push_str(blocker);
        }
    }
    let existing = existing.trim();
    if existing.is_empty() {
        block
    } else {
        format!("{existing}\n\n{block}")
    }
}

pub fn findings_parts(result: &AgentRunResult) -> (String, Vec<String>) {
    match result {
        AgentRunResult::Done {
            summary, blockers, ..
        } => (summary.clone(), blockers.clone()),
        AgentRunResult::Blocked { summary, .. } | AgentRunResult::Continued { summary, .. } => {
            (summary.clone(), Vec::new())
        }
    }
}

/// `git diff` range. Anything that is not a commit sha is refused.
pub fn diff_range_arg(anchor: &str) -> Option<String> {
    let anchor = anchor.trim();
    if anchor.len() < 7 || !anchor.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("{anchor}..HEAD"))
}

pub fn silence_collaboration(contract: &mut AgentRunResult) {
    match contract {
        AgentRunResult::Done {
            mention_agents,
            agent_requests,
            blockers,
            split_tickets,
            ..
        } => {
            mention_agents.clear();
            agent_requests.clear();
            blockers.clear();
            split_tickets.clear();
        }
        AgentRunResult::Blocked { mention_agents, .. } => {
            mention_agents.clear();
        }
        AgentRunResult::Continued { .. } => {}
    }
}

pub struct FinishedReview<'a> {
    pub ticket_id: Uuid,
    pub status: TicketStatus,
    pub agent_key: &'a str,
    pub agent_role: &'a str,
    pub result: &'a AgentRunResult,
    pub worktree: Option<&'a str>,
}

pub struct ReviewRoundService<'a> {
    pool: &'a PgPool,
}

impl<'a> ReviewRoundService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn load(&self, ticket_id: Uuid) -> Result<ReviewRoundState, ReviewRoundError> {
        let row = sqlx::query(
            r#"
            SELECT review_round, review_anchor_sha, review_findings, review_escalated
            FROM tickets
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or_else(|| ReviewRoundError::Message("ticket not found".into()))?;
        Ok(ReviewRoundState {
            round: row.get("review_round"),
            anchor_sha: row.get("review_anchor_sha"),
            findings: row.get("review_findings"),
            escalated: row.get("review_escalated"),
        })
    }

    /// Extra context for a reviewer on round 2 or later.
    pub async fn context_section(
        &self,
        ticket_id: Uuid,
        status: TicketStatus,
        job_type: &str,
        agent_key: &str,
        agent_role: &str,
        worktree: &Path,
    ) -> Result<Option<String>, ReviewRoundError> {
        if !is_review_run(status, job_type, agent_key, agent_role) {
            return Ok(None);
        }
        let state = self.load(ticket_id).await?;
        let next = state.round.saturating_add(1);
        if next < 2 {
            return Ok(None);
        }
        let diff = match state.anchor_sha.as_deref() {
            Some(anchor) => diff_since(worktree, anchor).await.unwrap_or_default(),
            None => String::new(),
        };
        Ok(reviewer_round_section(next, &diff, &state.findings))
    }

    /// Record a finished review. `true` when the cap replaced the send-back.
    pub async fn constrain_finished_review(
        &self,
        review: &FinishedReview<'_>,
        action: &mut TransitionAction,
        comment_body: &mut String,
    ) -> Result<bool, ReviewRoundError> {
        if !is_review_run(
            review.status,
            WORK_ON_TICKET,
            review.agent_key,
            review.agent_role,
        ) {
            return Ok(false);
        }
        if !is_review_change_request(action) {
            return Ok(false);
        }
        let state = self.load(review.ticket_id).await?;
        let next = state.round.saturating_add(1);
        if state.escalated || hits_round_cap(state.round, DEFAULT_REVIEW_ROUND_CAP) {
            self.save(
                review.ticket_id,
                ReviewRoundState {
                    round: next.max(state.round),
                    anchor_sha: state.anchor_sha,
                    findings: state.findings,
                    escalated: true,
                },
            )
            .await?;
            ReviewCapacityService::new(self.pool)
                .mark_merge_conflict_exempt(review.ticket_id)
                .await?;
            let (_, blockers) = findings_parts(review.result);
            apply_round_cap(action, comment_body, &blockers);
            return Ok(true);
        }
        let (summary, blockers) = findings_parts(review.result);
        let findings = append_findings(&state.findings, next, &summary, &blockers);
        let anchor_sha = match review.worktree {
            Some(path) => head_sha(Path::new(path)).await,
            None => None,
        };
        self.save(
            review.ticket_id,
            ReviewRoundState {
                round: next,
                anchor_sha,
                findings,
                escalated: false,
            },
        )
        .await?;
        Ok(false)
    }

    /// Stop a reviewer run that would start after the cap.
    /// Posts the reconsider comment once, then moves the ticket.
    pub async fn stop_capped_review(
        &self,
        ticket_id: Uuid,
        status: TicketStatus,
        agent_id: Uuid,
        agent_key: &str,
        agent_role: &str,
        job_type: &str,
    ) -> Result<bool, ReviewRoundError> {
        if !is_review_run(status, job_type, agent_key, agent_role) {
            return Ok(false);
        }
        let state = self.load(ticket_id).await?;
        if !state.escalated && !must_stop_before_run(state.round, DEFAULT_REVIEW_ROUND_CAP) {
            return Ok(false);
        }
        if !state.escalated {
            let findings = last_round_open_findings(&state.findings);
            self.post_reconsider(ticket_id, agent_id, &findings).await?;
            self.save(
                ticket_id,
                ReviewRoundState {
                    round: state.round.max(DEFAULT_REVIEW_ROUND_CAP),
                    anchor_sha: state.anchor_sha,
                    findings: state.findings,
                    escalated: true,
                },
            )
            .await?;
        }
        ReviewCapacityService::new(self.pool)
            .mark_merge_conflict_exempt(ticket_id)
            .await?;
        // Boxed: this status write can release a Ready ticket, which starts a run.
        Box::pin(TicketService::new(self.pool).apply_workflow_update(
            ticket_id,
            Some(TicketStatus::WaitForFinalReview),
            Some(None),
            Some(None),
            Some(None),
            Some(None),
            0,
        ))
        .await?;
        Ok(true)
    }

    pub async fn on_status_change(
        &self,
        ticket_id: Uuid,
        from: TicketStatus,
        to: TicketStatus,
    ) -> Result<(), ReviewRoundError> {
        let left_human_review =
            from == TicketStatus::WaitForFinalReview && to != TicketStatus::WaitForFinalReview;
        let approved = to == TicketStatus::InQa;
        if left_human_review || approved {
            self.clear(ticket_id).await?;
        }
        Ok(())
    }

    async fn save(&self, ticket_id: Uuid, state: ReviewRoundState) -> Result<(), ReviewRoundError> {
        sqlx::query(
            r#"
            UPDATE tickets
            SET review_round = $2,
                review_anchor_sha = $3,
                review_findings = $4,
                review_escalated = $5
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .bind(state.round)
        .bind(state.anchor_sha)
        .bind(state.findings)
        .bind(state.escalated)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    async fn clear(&self, ticket_id: Uuid) -> Result<(), ReviewRoundError> {
        self.save(
            ticket_id,
            ReviewRoundState {
                round: 0,
                anchor_sha: None,
                findings: String::new(),
                escalated: false,
            },
        )
        .await
    }

    async fn post_reconsider(
        &self,
        ticket_id: Uuid,
        agent_id: Uuid,
        findings: &[String],
    ) -> Result<(), ReviewRoundError> {
        let body = round_cap_comment(DEFAULT_REVIEW_ROUND_CAP, findings);
        CommentService::new(self.pool)
            .create(
                ticket_id,
                AuthorType::Agent,
                Some(agent_id),
                &body,
                CommentIntent::ReviewFeedback,
                &[],
                &[],
            )
            .await?;
        Ok(())
    }
}

fn is_review_run(status: TicketStatus, job_type: &str, agent_key: &str, agent_role: &str) -> bool {
    job_type == WORK_ON_TICKET
        && is_in_review_review_task(status_to_str(status), agent_key, agent_role)
}

async fn head_sha(worktree: &Path) -> Option<String> {
    crate::services::git_ops::git_head_sha(worktree)
        .await
        .ok()
        .filter(|sha| diff_range_arg(sha).is_some())
}

async fn diff_since(worktree: &Path, anchor: &str) -> Result<String, String> {
    let range = diff_range_arg(anchor).ok_or_else(|| "anchor".to_string())?;
    let output = crate::services::git_cmd::output(
        Some(worktree),
        &["diff", "--find-renames", "--unified=3", range.as_str()],
    )
    .await
    .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::workflow::JobRequest;

    fn action_with_fix() -> TransitionAction {
        TransitionAction {
            new_status: Some(TicketStatus::InProgress),
            enqueue_jobs: vec![JobRequest {
                job_type: WORK_ON_TICKET.into(),
                agent_id: Uuid::nil(),
                resume_agent_id: None,
            }],
            system_comments: vec!["extra".into()],
            ..TransitionAction::default()
        }
    }

    #[test]
    fn later_rounds_include_only_the_new_diff_and_earlier_findings() {
        assert!(reviewer_round_section(1, "whole branch", "old").is_none());
        let section = reviewer_round_section(
            2,
            "diff --git a/src/new.rs\n+added\n",
            "Round 1: Check the parser.\n- Missing a test",
        )
        .expect("round 2");
        assert!(section.contains("diff --git a/src/new.rs"));
        assert!(section.contains("+added"));
        assert!(section.contains("Round 1: Check the parser."));
        assert!(section.contains("- Missing a test"));
        assert!(section.contains(REVIEW_ROUND_INTRO));
        assert!(!section.contains("whole branch"));
        assert!(!section.contains("ticket description"));
    }

    #[test]
    fn an_empty_later_round_uses_placeholders() {
        let section = reviewer_round_section(3, "  ", "").expect("round 3");
        assert!(section.contains(REVIEW_ROUND_NO_DIFF));
        assert!(section.contains(REVIEW_ROUND_NO_FINDINGS));
        assert!(section.contains("# Review round 3"));
    }

    #[test]
    fn the_third_change_request_hits_the_cap_and_writes_one_comment() {
        assert!(!hits_round_cap(0, DEFAULT_REVIEW_ROUND_CAP));
        assert!(!hits_round_cap(1, DEFAULT_REVIEW_ROUND_CAP));
        assert!(hits_round_cap(2, DEFAULT_REVIEW_ROUND_CAP));
        assert!(!must_stop_before_run(2, DEFAULT_REVIEW_ROUND_CAP));
        assert!(must_stop_before_run(3, DEFAULT_REVIEW_ROUND_CAP));

        let mut action = action_with_fix();
        let mut body = "Please rename this helper and add a test.".into();
        let findings = vec!["Missing a test".into(), "Header is still wrong".into()];
        apply_round_cap(&mut action, &mut body, &findings);
        assert_eq!(action.new_status, Some(TicketStatus::WaitForFinalReview));
        assert_eq!(action.new_assignee_id, Some(None));
        assert!(action.enqueue_jobs.is_empty());
        assert!(action.system_comments.is_empty());
        assert_eq!(
            body,
            format!(
                "Review didn't pass after {DEFAULT_REVIEW_ROUND_CAP} rounds. This may need a different approach. Open findings:\n- Missing a test\n- Header is still wrong"
            )
        );
    }

    #[test]
    fn round_cap_comment_without_findings_has_no_open_findings_line() {
        let mut action = action_with_fix();
        let mut body = "Please rename this helper.".into();
        apply_round_cap(&mut action, &mut body, &[]);
        assert_eq!(
            body,
            format!(
                "Review didn't pass after {DEFAULT_REVIEW_ROUND_CAP} rounds. This may need a different approach."
            )
        );
        assert!(!body.contains("Open findings"));
        assert_eq!(action.new_status, Some(TicketStatus::WaitForFinalReview));

        let stored = "Round 1: Check the parser.\n- Add a test\n\nRound 2: Header is still wrong.";
        assert!(last_round_open_findings(stored).is_empty());
        assert_eq!(
            last_round_open_findings(
                "Round 1: Check the parser.\n- Add a test\n\nRound 2: Header is still wrong.\n- Rename the helper\n- Cover the blank line"
            ),
            vec!["Rename the helper".to_string(), "Cover the blank line".to_string()]
        );
    }

    #[test]
    fn findings_accumulate_and_the_diff_range_is_a_sha() {
        let first = append_findings("", 1, "Parser misses the header.", &["Add a test".into()]);
        let second = append_findings(&first, 2, "Header is still wrong.", &[]);
        assert_eq!(
            second,
            "Round 1: Parser misses the header.\n- Add a test\n\nRound 2: Header is still wrong."
        );
        assert_eq!(
            diff_range_arg("abc1234dead").as_deref(),
            Some("abc1234dead..HEAD")
        );
        assert!(diff_range_arg("HEAD").is_none());
        assert!(diff_range_arg("main").is_none());
        assert!(diff_range_arg("abc").is_none());
    }

    #[test]
    fn change_requests_are_fix_handoffs() {
        assert!(is_review_change_request(&action_with_fix()));
        let approved = TransitionAction {
            new_status: Some(TicketStatus::InQa),
            ..TransitionAction::default()
        };
        assert!(!is_review_change_request(&approved));
    }
}
