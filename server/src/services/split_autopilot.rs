//! Ordered auto-start for a ticket split at Plan Review.
//!
//! The parent keeps `child_order` and `auto_start` on `ticket_plan_splits`.
//! Approving the plan starts child 1. A recorded merge starts the next queued
//! child, and the last merge starts parent verification. Pause, a blocked
//! child, or an archived child change that: pause and blocked stop the
//! automatic step, archive is skipped, and Resume or Start next continues.

use std::collections::HashMap;

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::domain::substatus::TicketStatus;
use crate::domain::ticket::{status_from_str, status_to_str};
use crate::domain::workflow::JOB_TYPE_VERIFY_PARENT;
use crate::services::human_review_service::{HumanReviewError, HumanReviewService};
use crate::services::planning_service::{PlanError, PlanningService};
use crate::services::ticket_service::TicketService;

pub const NOT_A_SPLIT: &str = "Only a ticket split at Plan Review has these controls.";
pub const NOTHING_TO_START: &str = "No queued sub-ticket to start.";
pub const NOTHING_TO_MARK_MERGED: &str = "No accepted sub-ticket is waiting to be marked merged.";

pub const ACTOR_YOU: &str = "you";
pub const ACTOR_AGENT: &str = "agent";
pub const ACTOR_NONE: &str = "none";

/// One split, shared by the parent card and every sub-ticket card.
/// Board lanes group on `parent_ticket_id`. `position` is this card's place
/// in the order; the parent card leaves it empty.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitProgress {
    pub parent_ticket_id: Uuid,
    pub parent_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i32>,
    pub count: i32,
    pub merged_count: i32,
    pub auto_start: bool,
    pub active_child_id: Option<Uuid>,
    pub next_actor: String,
    pub children: Vec<SplitChildProgress>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitChildProgress {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub position: i32,
    pub archived: bool,
    pub merged: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AdvanceMode {
    /// Merge, resume, and plan approval. Honors pause and a blocked child.
    Auto,
    /// Start next. Skips a blocked child and starts the following queued one.
    Manual,
}

struct ChildSnap {
    id: Uuid,
    title: String,
    status: TicketStatus,
    archived: bool,
    merged: bool,
    accepted: bool,
}

struct Family {
    parent_id: Uuid,
    parent_title: String,
    parent_status: TicketStatus,
    auto_start: bool,
    verifying: bool,
    children: Vec<ChildSnap>,
}

pub struct SplitAutopilot<'a> {
    pool: &'a PgPool,
}

impl<'a> SplitAutopilot<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// The plan was approved and the children exist, still in Backlog.
    /// The parent becomes the running tracker and child 1 leaves Backlog.
    pub async fn on_split_approved(&self, parent_id: Uuid) -> Result<(), PlanError> {
        self.move_parent_to_running(parent_id).await?;
        self.advance(parent_id, AdvanceMode::Auto).await
    }

    /// A merge was just recorded on `ticket_id`. No-op unless it is a split child.
    pub async fn on_child_merged(&self, ticket_id: Uuid) -> Result<(), PlanError> {
        let parent_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT split.parent_ticket_id
            FROM tickets child
            JOIN ticket_plan_splits split ON split.parent_ticket_id = child.parent_ticket_id
            WHERE child.id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;
        let Some(parent_id) = parent_id else {
            return Ok(());
        };
        self.advance(parent_id, AdvanceMode::Auto).await
    }

    pub async fn pause(&self, parent_id: Uuid) -> Result<(), PlanError> {
        self.set_auto_start(parent_id, false).await
    }

    pub async fn resume(&self, parent_id: Uuid) -> Result<(), PlanError> {
        self.set_auto_start(parent_id, true).await?;
        self.advance(parent_id, AdvanceMode::Auto).await
    }

    pub async fn start_next(&self, parent_id: Uuid) -> Result<(), PlanError> {
        self.advance(parent_id, AdvanceMode::Manual).await
    }

    /// The active accepted child merged on a forge the sweep cannot see.
    /// Records that merge, which starts the next child when auto-start is on.
    pub async fn merged_elsewhere(&self, parent_id: Uuid) -> Result<(), PlanError> {
        let family = self
            .load_family(parent_id)
            .await?
            .ok_or_else(|| PlanError::Validation(NOT_A_SPLIT.into()))?;
        // Only the active child: a later accepted child must not be marked
        // merged while an earlier one is still open.
        let Some(child) = active_child(&family.children).filter(|child| child.accepted) else {
            return Err(PlanError::Validation(NOTHING_TO_MARK_MERGED.into()));
        };
        let review = HumanReviewService::new(self.pool)
            .get(child.id)
            .await?
            .ok_or_else(|| PlanError::Validation(NOTHING_TO_MARK_MERGED.into()))?;
        HumanReviewService::new(self.pool)
            .record_merge(child.id, &review.head_sha)
            .await
            .map_err(review_error)?;
        Ok(())
    }

    /// Progress for each requested ticket that belongs to a split, keyed by
    /// that ticket. Parent and children share one family; `position` differs.
    pub async fn progress_for(
        &self,
        ticket_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, SplitProgress>, PlanError> {
        if ticket_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let families = self.load_families(ticket_ids).await?;
        let mut out = HashMap::new();
        for family in families {
            let progress = progress_from(&family);
            out.insert(family.parent_id, progress.clone());
            for child in &progress.children {
                let mut card = progress.clone();
                card.position = Some(child.position);
                out.insert(child.id, card);
            }
        }
        Ok(out)
    }

    async fn advance(&self, parent_id: Uuid, mode: AdvanceMode) -> Result<(), PlanError> {
        let Some(family) = self.load_family(parent_id).await? else {
            return Err(PlanError::Validation(NOT_A_SPLIT.into()));
        };
        if mode == AdvanceMode::Auto && (!family.auto_start || any_blocked(&family.children)) {
            return Ok(());
        }
        let next = match mode {
            AdvanceMode::Auto => next_in_order(&family.children),
            AdvanceMode::Manual => next_manual(&family.children),
        };
        if let Some(child) = next {
            return self.start_child(child.id).await;
        }
        if ready_to_verify(&family.children) {
            return self.start_parent_verification(parent_id, mode).await;
        }
        if mode == AdvanceMode::Manual {
            return Err(PlanError::Validation(NOTHING_TO_START.into()));
        }
        Ok(())
    }

    async fn move_parent_to_running(&self, parent_id: Uuid) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(parent_id).await?;
        if ticket.ticket.status != TicketStatus::Ready {
            return Ok(());
        }
        // In Progress is the running lane. Ready would look like the parent
        // is waiting for its own implementation run, which it never gets.
        TicketService::new(self.pool)
            .update_status(parent_id, TicketStatus::InProgress, Some(None), Some(None))
            .await?;
        Ok(())
    }

    async fn start_child(&self, child_id: Uuid) -> Result<(), PlanError> {
        let ticket = TicketService::new(self.pool).get(child_id).await?;
        if ticket.ticket.archived_at.is_some() || ticket.ticket.status != TicketStatus::Backlog {
            return Ok(());
        }
        TicketService::new(self.pool)
            .update_status(child_id, TicketStatus::Ready, Some(None), Some(None))
            .await?;
        PlanningService::new(self.pool)
            .on_entered_ready(child_id)
            .await?;
        Ok(())
    }

    async fn start_parent_verification(
        &self,
        parent_id: Uuid,
        mode: AdvanceMode,
    ) -> Result<(), PlanError> {
        match PlanningService::new(self.pool)
            .start_verification(parent_id, None)
            .await
        {
            Ok(_) => Ok(()),
            Err(PlanError::Validation(message)) if mode == AdvanceMode::Auto => {
                tracing::info!(%parent_id, %message, "split parent verification did not start");
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    async fn set_auto_start(&self, parent_id: Uuid, auto_start: bool) -> Result<(), PlanError> {
        let updated = sqlx::query(
            "UPDATE ticket_plan_splits SET auto_start = $2 WHERE parent_ticket_id = $1",
        )
        .bind(parent_id)
        .bind(auto_start)
        .execute(self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(PlanError::Validation(NOT_A_SPLIT.into()));
        }
        Ok(())
    }

    async fn load_family(&self, parent_id: Uuid) -> Result<Option<Family>, PlanError> {
        let mut families = self.load_families(&[parent_id]).await?;
        Ok(families.pop())
    }

    /// Every family that contains a requested id. A match on one child still
    /// returns its siblings; dropping them would renumber that child as 1 of 1.
    async fn load_families(&self, ticket_ids: &[Uuid]) -> Result<Vec<Family>, PlanError> {
        let rows = sqlx::query(
            r#"
            SELECT
                parent.id AS parent_id,
                parent.title AS parent_title,
                parent.status AS parent_status,
                split.auto_start,
                EXISTS (
                    SELECT 1 FROM agent_runs run
                    WHERE run.ticket_id = parent.id
                      AND run.job_type = $2
                      AND run.status IN ('queued', 'running')
                ) AS verifying,
                child.id AS child_id,
                child.title AS child_title,
                child.status AS child_status,
                child.archived_at IS NOT NULL AS archived,
                review.merged_at IS NOT NULL AS merged,
                review.ticket_id IS NOT NULL AS accepted
            FROM ticket_plan_splits split
            JOIN tickets parent ON parent.id = split.parent_ticket_id
            JOIN tickets child ON child.parent_ticket_id = parent.id
            LEFT JOIN ticket_human_reviews review ON review.ticket_id = child.id
            WHERE split.parent_ticket_id = ANY($1)
               OR split.parent_ticket_id IN (
                    SELECT requested.parent_ticket_id
                    FROM tickets requested
                    WHERE requested.id = ANY($1)
                      AND requested.parent_ticket_id IS NOT NULL
               )
            ORDER BY
                parent.id,
                array_position(split.child_order, child.id) NULLS LAST,
                child.created_at,
                child.id
            "#,
        )
        .bind(ticket_ids)
        .bind(JOB_TYPE_VERIFY_PARENT)
        .fetch_all(self.pool)
        .await?;

        let mut families: Vec<Family> = Vec::new();
        for row in rows {
            let parent_id: Uuid = row.get("parent_id");
            if families.last().map(|family| family.parent_id) != Some(parent_id) {
                let status: String = row.get("parent_status");
                families.push(Family {
                    parent_id,
                    parent_title: row.get("parent_title"),
                    parent_status: status_from_str(&status).unwrap_or(TicketStatus::Backlog),
                    auto_start: row.get("auto_start"),
                    verifying: row.get("verifying"),
                    children: Vec::new(),
                });
            }
            let status: String = row.get("child_status");
            let family = families.last_mut().expect("family just pushed");
            family.children.push(ChildSnap {
                id: row.get("child_id"),
                title: row.get("child_title"),
                status: status_from_str(&status).unwrap_or(TicketStatus::Backlog),
                archived: row.get("archived"),
                merged: row.get("merged"),
                accepted: row.get("accepted"),
            });
        }
        Ok(families)
    }
}

fn progress_from(family: &Family) -> SplitProgress {
    let children: Vec<SplitChildProgress> = family
        .children
        .iter()
        .enumerate()
        .map(|(index, child)| SplitChildProgress {
            id: child.id,
            title: child.title.clone(),
            status: status_to_str(child.status).to_string(),
            position: (index + 1) as i32,
            archived: child.archived,
            merged: child.merged,
        })
        .collect();
    let merged_count = family
        .children
        .iter()
        .filter(|child| !child.archived && child.merged)
        .count() as i32;
    SplitProgress {
        parent_ticket_id: family.parent_id,
        parent_title: family.parent_title.clone(),
        position: None,
        count: children.len() as i32,
        merged_count,
        auto_start: family.auto_start,
        active_child_id: active_child(&family.children).map(|child| child.id),
        next_actor: next_actor(family).to_string(),
        children,
    }
}

fn finished(child: &ChildSnap) -> bool {
    child.archived || (child.status == TicketStatus::Done && child.merged)
}

fn active_child(children: &[ChildSnap]) -> Option<&ChildSnap> {
    children.iter().find(|child| !finished(child))
}

fn any_blocked(children: &[ChildSnap]) -> bool {
    children
        .iter()
        .any(|child| !child.archived && child.status == TicketStatus::Blocked)
}

/// The next Backlog child, only when every child before it is finished.
fn next_in_order(children: &[ChildSnap]) -> Option<&ChildSnap> {
    for child in children {
        if finished(child) {
            continue;
        }
        if child.status == TicketStatus::Backlog {
            return Some(child);
        }
        return None;
    }
    None
}

/// Like [`next_in_order`], but a blocked child is skipped so Start next can
/// continue past it. An in-flight child still holds the line.
fn next_manual(children: &[ChildSnap]) -> Option<&ChildSnap> {
    for child in children {
        if finished(child) || child.status == TicketStatus::Blocked {
            continue;
        }
        if child.status == TicketStatus::Backlog {
            return Some(child);
        }
        return None;
    }
    None
}

fn ready_to_verify(children: &[ChildSnap]) -> bool {
    let mut live = 0;
    for child in children {
        if child.archived {
            continue;
        }
        live += 1;
        if child.status != TicketStatus::Done || !child.merged {
            return false;
        }
    }
    live > 0
}

fn next_actor(family: &Family) -> &'static str {
    if family.verifying {
        return ACTOR_AGENT;
    }
    let Some(active) = active_child(&family.children) else {
        return match family.parent_status {
            TicketStatus::WaitForFinalReview => ACTOR_YOU,
            _ => ACTOR_NONE,
        };
    };
    match active.status {
        TicketStatus::Blocked | TicketStatus::WaitForFinalReview | TicketStatus::Done => ACTOR_YOU,
        TicketStatus::Backlog if !family.auto_start => ACTOR_YOU,
        _ => ACTOR_AGENT,
    }
}

fn review_error(err: HumanReviewError) -> PlanError {
    match err {
        HumanReviewError::Database(err) => PlanError::Database(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child(status: TicketStatus, archived: bool, merged: bool) -> ChildSnap {
        ChildSnap {
            id: Uuid::nil(),
            title: String::new(),
            status,
            archived,
            merged,
            accepted: false,
        }
    }

    #[test]
    fn auto_stops_at_an_open_child_and_skips_archived() {
        let children = vec![
            child(TicketStatus::Done, false, true),
            child(TicketStatus::Backlog, true, false),
            child(TicketStatus::Backlog, false, false),
        ];
        let next = next_in_order(&children).expect("queued child");
        assert!(!next.archived);
        assert_eq!(next.status, TicketStatus::Backlog);
        assert!(!any_blocked(&children));

        let held = vec![
            child(TicketStatus::Done, false, true),
            child(TicketStatus::Blocked, false, false),
            child(TicketStatus::Backlog, false, false),
        ];
        assert!(next_in_order(&held).is_none());
        assert_eq!(
            next_manual(&held).map(|c| c.status),
            Some(TicketStatus::Backlog)
        );
        assert!(any_blocked(&held));
    }

    #[test]
    fn manual_does_not_pass_an_in_flight_child() {
        let children = vec![
            child(TicketStatus::Ready, false, false),
            child(TicketStatus::Backlog, false, false),
        ];
        assert!(next_manual(&children).is_none());
    }

    #[test]
    fn verify_waits_until_every_live_child_is_merged() {
        let open = vec![
            child(TicketStatus::Done, false, true),
            child(TicketStatus::Done, false, false),
        ];
        assert!(!ready_to_verify(&open));
        let done = vec![
            child(TicketStatus::Done, false, true),
            child(TicketStatus::Backlog, true, false),
        ];
        assert!(ready_to_verify(&done));
    }

    #[test]
    fn paused_queue_is_the_humans_turn() {
        let family = Family {
            parent_id: Uuid::nil(),
            parent_title: String::new(),
            parent_status: TicketStatus::InProgress,
            auto_start: false,
            verifying: false,
            children: vec![
                child(TicketStatus::Done, false, true),
                child(TicketStatus::Backlog, false, false),
            ],
        };
        assert_eq!(next_actor(&family), ACTOR_YOU);
    }
}
