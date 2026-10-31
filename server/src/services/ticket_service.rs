use crate::domain::substatus::{
    build_substatus_display, validate_status_substatus_combo, Substatus, SubstatusDisplay,
    TicketStatus,
};
use crate::domain::workflow::{PendingRecommendation, PendingSplitRecommendation};
use crate::services::human_review_service::{HumanReview, HumanReviewService};
use crate::services::review_capacity::ReviewCapacityService;
use crate::services::workflow_service::{PlanEntry, WorkflowService};
use crate::domain::ticket::{
    priority_from_str, priority_to_str, status_from_str, status_to_str, substatus_from_str,
    substatus_to_str, Ticket, TicketPriority,
};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::Row;
use std::collections::HashSet;
use time::OffsetDateTime;
use uuid::Uuid;

pub struct TicketService<'a> {
    pool: &'a PgPool,
}

enum StatusWrite {
    Direct,
    Accept,
    /// Continue a column a crash moved to Blocked. Skips the plan gate:
    /// the ticket was already in that column.
    RestartResume,
}

#[derive(Debug, Default)]
pub struct TicketFilters {
    pub status: Option<TicketStatus>,
    pub assignee_agent_id: Option<Uuid>,
    pub include_archived: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum TicketError {
    #[error("ticket not found")]
    TicketNotFound,
    #[error("board not found")]
    BoardNotFound,
    #[error("invalid status")]
    InvalidStatus,
    #[error("invalid substatus")]
    InvalidSubstatus,
    #[error("invalid priority")]
    InvalidPriority,
    #[error("an active run already exists for this ticket")]
    ActiveRunExists,
    #[error("ticket is archived; unarchive before continuing")]
    Archived,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// The approved plan is the newest ticket_plans row, written for the ticket's
/// current content. Same rule as [`TicketService::approved_plan_is_current`].
fn plan_is_current(ticket: &Ticket, latest_plan_id: Option<Uuid>, latest_plan_version: Option<i32>) -> bool {
    match (ticket.approved_plan_id, ticket.approved_plan_content_version) {
        (Some(approved_id), Some(approved_version)) => {
            approved_version == ticket.content_version
                && latest_plan_id == Some(approved_id)
                && latest_plan_version == Some(ticket.content_version)
        }
        _ => false,
    }
}

fn plan_error(err: crate::services::planning_service::PlanError) -> TicketError {
    match err {
        crate::services::planning_service::PlanError::Database(err) => TicketError::Database(err),
        crate::services::planning_service::PlanError::Validation(message) => {
            TicketError::Validation(message)
        }
    }
}

/// Columns returned by aliased ticket list queries (keep in sync with `row_to_ticket`).
const TICKET_COLUMNS_ALIASED: &str = r#"
                t.id, t.board_id, t.repo_id, t.title, t.description,
                t.status, t.substatus, t.substatus_metadata, t.priority,
                t.assignee_agent_id, t.owner_user_id, t.branch_name,
                t.pending_assign_recommendation, t.parent_ticket_id,
                t.pending_split_recommendation, t.clarification_round,
                t.skip_planning, t.content_version,
                t.approved_plan_id, t.approved_plan_content_version,
                t.created_by, t.created_by_id, t.created_at, t.updated_at, t.archived_at,
                t.attachment_ids
"#;

pub struct TicketWithDisplay {
    pub ticket: Ticket,
    pub substatus_display: Option<SubstatusDisplay>,
    pub last_activity_at: OffsetDateTime,
    pub has_active_run: bool,
    pub human_review: Option<HumanReview>,
    /// True when the current content version has an approved plan.
    pub plan_approved: bool,
    /// Status of the newest `plan_ticket` run, when one exists.
    pub latest_plan_run_status: Option<String>,
    /// The planner's question round, when it asked one.
    pub plan_questions: Option<crate::services::planning_service::PlanQuestionsState>,
    /// In Plan Review: a plan comment exists to approve. False elsewhere.
    pub has_plan: bool,
    /// Split into sub-tickets at Plan Review; gets no implementation run.
    pub split_parent: bool,
    /// Every sub-ticket is Done and merged, so Verify parent ticket can run.
    pub verify_available: bool,
}

impl<'a> TicketService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_by_board(
        &self,
        board_id: Uuid,
        filters: &TicketFilters,
    ) -> Result<Vec<TicketWithDisplay>, TicketError> {
        self.ensure_board_exists(board_id).await?;

        let mut query = format!(
            r#"
            SELECT
                {TICKET_COLUMNS_ALIASED}
            FROM tickets t
            WHERE t.board_id = $1
            "#,
        );
        let mut bind_index = 2;

        if !filters.include_archived {
            query.push_str(" AND t.archived_at IS NULL");
        }
        if filters.status.is_some() {
            query.push_str(&format!(" AND t.status = ${bind_index}"));
            bind_index += 1;
        }
        if filters.assignee_agent_id.is_some() {
            query.push_str(&format!(" AND t.assignee_agent_id = ${bind_index}"));
        }
        query.push_str(" ORDER BY t.created_at ASC");

        let mut q = sqlx::query(&query).bind(board_id);
        if let Some(status) = filters.status {
            q = q.bind(status_to_str(status));
        }
        if let Some(agent_id) = filters.assignee_agent_id {
            q = q.bind(agent_id);
        }

        let rows = q.fetch_all(self.pool).await?;
        let active_ticket_ids = self.active_ticket_ids_for_board(board_id).await?;
        let active: std::collections::HashSet<Uuid> = active_ticket_ids.into_iter().collect();
        self.enrich_many(rows.iter().map(row_to_ticket).collect(), &active)
            .await
    }

    /// Case-insensitive title/description search over non-archived tickets, newest
    /// activity first. `board_id: None` searches every board.
    pub async fn search(
        &self,
        board_id: Option<Uuid>,
        query: Option<&str>,
        status: Option<TicketStatus>,
        limit: i64,
    ) -> Result<Vec<Ticket>, TicketError> {
        let pattern = query
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(|q| {
                let escaped = q
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                format!("%{escaped}%")
            });
        let sql = format!(
            r#"
            SELECT {TICKET_COLUMNS_ALIASED}
            FROM tickets t
            WHERE t.archived_at IS NULL
              AND ($1::uuid IS NULL OR t.board_id = $1)
              AND ($2::text IS NULL OR t.title ILIKE $2 OR t.description ILIKE $2)
              AND ($3::text IS NULL OR t.status = $3)
            ORDER BY t.updated_at DESC, t.id
            LIMIT $4
            "#,
        );
        let rows = sqlx::query(&sql)
            .bind(board_id)
            .bind(pattern)
            .bind(status.map(status_to_str))
            .bind(limit)
            .fetch_all(self.pool)
            .await?;
        Ok(rows.iter().map(row_to_ticket).collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        board_id: Uuid,
        title: &str,
        description: &str,
        repo_id: Option<Uuid>,
        priority: Option<TicketPriority>,
        created_by: &str,
        created_by_id: Uuid,
    ) -> Result<TicketWithDisplay, TicketError> {
        self.create_with_source(
            board_id,
            title,
            description,
            repo_id,
            priority,
            created_by,
            created_by_id,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_with_source(
        &self,
        board_id: Uuid,
        title: &str,
        description: &str,
        repo_id: Option<Uuid>,
        priority: Option<TicketPriority>,
        created_by: &str,
        created_by_id: Uuid,
        source_chat_session_id: Option<Uuid>,
    ) -> Result<TicketWithDisplay, TicketError> {
        self.ensure_board_exists(board_id).await?;

        let repo_id = match repo_id {
            Some(id) => Some(id),
            None => self.default_repo_id().await?,
        };

        let id = Uuid::new_v4();
        let status = TicketStatus::Backlog;
        let priority_str = priority_to_str(priority_for_create(priority));

        let row = sqlx::query(
            r#"
            INSERT INTO tickets (
                id, board_id, repo_id, title, description, status, priority,
                created_by, created_by_id, source_chat_session_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(id)
        .bind(board_id)
        .bind(repo_id)
        .bind(title)
        .bind(description)
        .bind(status_to_str(status))
        .bind(priority_str)
        .bind(created_by)
        .bind(created_by_id)
        .bind(source_chat_session_id)
        .fetch_one(self.pool)
        .await?;

        self.enrich_row(&row).await
    }

    pub async fn create_child(
        &self,
        parent: &Ticket,
        title: &str,
        description: &str,
        created_by_id: Uuid,
    ) -> Result<TicketWithDisplay, TicketError> {
        self.ensure_board_exists(parent.board_id).await?;

        let id = Uuid::new_v4();
        let status = TicketStatus::Backlog;
        let priority_str = priority_to_str(priority_for_create(None));

        let row = sqlx::query(
            r#"
            INSERT INTO tickets (
                id, board_id, repo_id, title, description, status, priority,
                parent_ticket_id, created_by, created_by_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(id)
        .bind(parent.board_id)
        .bind(parent.repo_id)
        .bind(title)
        .bind(description)
        .bind(status_to_str(status))
        .bind(priority_str)
        .bind(parent.id)
        .bind("agent")
        .bind(created_by_id)
        .fetch_one(self.pool)
        .await?;

        self.enrich_row(&row).await
    }

    pub async fn set_pending_split_recommendation(
        &self,
        ticket_id: Uuid,
        recommendation: PendingSplitRecommendation,
    ) -> Result<TicketWithDisplay, TicketError> {
        let value = serde_json::to_value(recommendation).map_err(|e| {
            TicketError::Validation(format!("pending split recommendation: {e}"))
        })?;

        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET pending_split_recommendation = $2, updated_at = now()
            WHERE id = $1
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(ticket_id)
        .bind(&value)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        let ticket = row_to_ticket(&row);
        let has_active_run = self.ticket_has_active_run(ticket.id).await?;
        self.enrich_ticket(ticket, has_active_run).await
    }

    pub async fn get(&self, ticket_id: Uuid) -> Result<TicketWithDisplay, TicketError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            FROM tickets
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        let ticket = row_to_ticket(&row);
        let has_active_run = self.ticket_has_active_run(ticket.id).await?;
        self.enrich_ticket(ticket, has_active_run).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_fields(
        &self,
        ticket_id: Uuid,
        title: Option<&str>,
        description: Option<&str>,
        repo_id: Option<Option<Uuid>>,
        priority: Option<Option<TicketPriority>>,
        branch_name: Option<Option<&str>>,
        owner_user_id: Option<Option<Uuid>>,
        skip_planning: Option<bool>,
        human_edit: bool,
    ) -> Result<TicketWithDisplay, TicketError> {
        let current = self.get(ticket_id).await?;
        Self::ensure_not_archived(&current.ticket)?;
        let title = title.unwrap_or(&current.ticket.title);
        let description = description.unwrap_or(&current.ticket.description);
        let repo_id = match repo_id {
            Some(value) => value,
            None => current.ticket.repo_id,
        };
        let priority = match priority {
            Some(value) => value,
            None => current.ticket.priority,
        };
        let branch_name = match branch_name {
            Some(value) => value.map(str::to_string),
            None => current.ticket.branch_name.clone(),
        };
        let owner_user_id = match owner_user_id {
            Some(value) => value,
            None => current.ticket.owner_user_id,
        };
        let priority_str = priority.map(priority_to_str);
        let content_changed = human_edit
            && (*title != current.ticket.title || description != current.ticket.description);
        let version_delta: i32 = i32::from(content_changed);
        let skip_planning = skip_planning.unwrap_or(current.ticket.skip_planning);

        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET
                title = $2,
                description = $3,
                repo_id = $4,
                priority = $5,
                branch_name = $6,
                owner_user_id = $7,
                content_version = content_version + $8,
                skip_planning = $9,
                updated_at = now()
            WHERE id = $1
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(ticket_id)
        .bind(title)
        .bind(description)
        .bind(repo_id)
        .bind(priority_str)
        .bind(&branch_name)
        .bind(owner_user_id)
        .bind(version_delta)
        .bind(skip_planning)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        self.enrich_row(&row).await
    }

    pub async fn set_attachment_ids(
        &self,
        ticket_id: Uuid,
        attachment_ids: &[Uuid],
    ) -> Result<TicketWithDisplay, TicketError> {
        let current = self.get(ticket_id).await?;
        Self::ensure_not_archived(&current.ticket)?;
        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET attachment_ids = $2, updated_at = now()
            WHERE id = $1
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(ticket_id)
        .bind(attachment_ids)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;
        self.enrich_row(&row).await
    }

    pub async fn update_status(
        &self,
        ticket_id: Uuid,
        status: TicketStatus,
        substatus: Option<Option<Substatus>>,
        substatus_metadata: Option<Option<Value>>,
    ) -> Result<TicketWithDisplay, TicketError> {
        self.write_status(
            ticket_id,
            status,
            substatus,
            substatus_metadata,
            StatusWrite::Direct,
        )
        .await
    }

    /// Put a ticket back in the column it occupied when Coppice restarted.
    pub async fn restore_interrupted_column(
        &self,
        ticket_id: Uuid,
        status: TicketStatus,
    ) -> Result<TicketWithDisplay, TicketError> {
        self.write_status(
            ticket_id,
            status,
            Some(None),
            Some(None),
            StatusWrite::RestartResume,
        )
        .await
    }

    /// Accept is the only status write that may enter Done.
    pub async fn accept_to_done(
        &self,
        ticket_id: Uuid,
    ) -> Result<TicketWithDisplay, TicketError> {
        self.write_status(
            ticket_id,
            TicketStatus::Done,
            Some(None),
            Some(None),
            StatusWrite::Accept,
        )
        .await
    }

    async fn write_status(
        &self,
        ticket_id: Uuid,
        status: TicketStatus,
        substatus: Option<Option<Substatus>>,
        substatus_metadata: Option<Option<Value>>,
        write: StatusWrite,
    ) -> Result<TicketWithDisplay, TicketError> {
        let current = self.get(ticket_id).await?;
        Self::ensure_not_archived(&current.ticket)?;
        match write {
            StatusWrite::Accept => {
                let approved = WorkflowService::final_approve(current.ticket.status)
                    .map_err(|msg| TicketError::Validation(msg.to_string()))?;
                if status != approved {
                    return Err(TicketError::Validation(
                        "final approve requires wait_for_final_review".into(),
                    ));
                }
            }
            StatusWrite::Direct => {
                let plan = self.plan_entry(&current.ticket).await?;
                if let Some(msg) =
                    WorkflowService::direct_status_change_error(current.ticket.status, status, plan)
                {
                    return Err(TicketError::Validation(msg.to_string()));
                }
                if let Some(message) = ReviewCapacityService::new(self.pool)
                    .direct_status_block(ticket_id, current.ticket.status, status)
                    .await?
                {
                    return Err(TicketError::Validation(message));
                }
            }
            StatusWrite::RestartResume => {
                if current.ticket.status != TicketStatus::Blocked {
                    return Err(TicketError::Validation(
                        "ticket is not waiting after a restart".into(),
                    ));
                }
                if status == TicketStatus::Done {
                    return Err(TicketError::Validation(
                        "accepted work is not resumed".into(),
                    ));
                }
            }
        }
        let substatus = match substatus {
            Some(value) => value,
            None => current.ticket.substatus,
        };
        let substatus_metadata = match substatus_metadata {
            Some(value) => value,
            None => current.ticket.substatus_metadata.clone(),
        };

        if let Some(msg) =
            validate_status_substatus_combo(status, substatus, &substatus_metadata)
        {
            return Err(TicketError::Validation(msg.to_string()));
        }

        let substatus_str = substatus.map(substatus_to_str);

        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET
                status = $2,
                substatus = $3,
                substatus_metadata = $4,
                updated_at = now()
            WHERE id = $1
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(ticket_id)
        .bind(status_to_str(status))
        .bind(substatus_str)
        .bind(&substatus_metadata)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        ReviewCapacityService::new(self.pool)
            .after_status_write(
                current.ticket.board_id,
                ticket_id,
                current.ticket.status,
                status,
            )
            .await?;

        self.enrich_row(&row).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn apply_workflow_update(
        &self,
        ticket_id: Uuid,
        status: Option<TicketStatus>,
        substatus: Option<Option<Substatus>>,
        substatus_metadata: Option<Option<Value>>,
        assignee: Option<Option<Uuid>>,
        pending_recommendation: Option<Option<PendingRecommendation>>,
        clarification_round_delta: i32,
    ) -> Result<TicketWithDisplay, TicketError> {
        let current = self.get(ticket_id).await?;
        Self::ensure_not_archived(&current.ticket)?;

        let status = status.unwrap_or(current.ticket.status);
        let substatus = match substatus {
            Some(value) => value,
            None => current.ticket.substatus,
        };
        let substatus_metadata = match substatus_metadata {
            Some(value) => value,
            None => current.ticket.substatus_metadata.clone(),
        };
        let assignee_agent_id = match assignee {
            Some(value) => value,
            None => current.ticket.assignee_agent_id,
        };
        let pending_assign_recommendation = match pending_recommendation {
            Some(None) => None,
            Some(Some(rec)) => Some(serde_json::to_value(rec).map_err(|e| {
                TicketError::Validation(format!("pending recommendation: {e}"))
            })?),
            None => current.ticket.pending_assign_recommendation.clone(),
        };
        let clarification_round =
            current.ticket.clarification_round.saturating_add(clarification_round_delta);

        let plan = self.plan_entry(&current.ticket).await?;
        if let Some(msg) =
            WorkflowService::direct_status_change_error(current.ticket.status, status, plan)
        {
            return Err(TicketError::Validation(msg.to_string()));
        }

        if let Some(msg) =
            validate_status_substatus_combo(status, substatus, &substatus_metadata)
        {
            return Err(TicketError::Validation(msg.to_string()));
        }

        let substatus_str = substatus.map(substatus_to_str);

        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET
                status = $2,
                substatus = $3,
                substatus_metadata = $4,
                assignee_agent_id = $5,
                pending_assign_recommendation = $6,
                clarification_round = $7,
                updated_at = now()
            WHERE id = $1
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(ticket_id)
        .bind(status_to_str(status))
        .bind(substatus_str)
        .bind(&substatus_metadata)
        .bind(assignee_agent_id)
        .bind(&pending_assign_recommendation)
        .bind(clarification_round)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        ReviewCapacityService::new(self.pool)
            .after_status_write(
                current.ticket.board_id,
                ticket_id,
                current.ticket.status,
                status,
            )
            .await?;

        self.enrich_row(&row).await
    }

    pub async fn clear_pending_recommendation(
        &self,
        ticket_id: Uuid,
    ) -> Result<TicketWithDisplay, TicketError> {
        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET pending_assign_recommendation = NULL, updated_at = now()
            WHERE id = $1
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        self.enrich_row(&row).await
    }

    pub async fn clear_pending_split_recommendation(
        &self,
        ticket_id: Uuid,
    ) -> Result<TicketWithDisplay, TicketError> {
        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET pending_split_recommendation = NULL, updated_at = now()
            WHERE id = $1
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        self.enrich_row(&row).await
    }

    pub async fn list_children(
        &self,
        parent_ticket_id: Uuid,
    ) -> Result<Vec<TicketWithDisplay>, TicketError> {
        self.get(parent_ticket_id).await?;

        let rows = sqlx::query(
            r#"
            SELECT
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            FROM tickets
            WHERE parent_ticket_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(parent_ticket_id)
        .fetch_all(self.pool)
        .await?;

        let tickets: Vec<Ticket> = rows.iter().map(row_to_ticket).collect();
        let ids: Vec<Uuid> = tickets.iter().map(|ticket| ticket.id).collect();
        let active: std::collections::HashSet<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT ticket_id FROM agent_runs WHERE ticket_id = ANY($1) AND status IN ('queued', 'running')",
        )
        .bind(&ids)
        .fetch_all(self.pool)
        .await?
        .into_iter()
        .collect();
        self.enrich_many(tickets, &active).await
    }

    pub async fn assign_agent(
        &self,
        ticket_id: Uuid,
        agent_id: Option<Uuid>,
    ) -> Result<TicketWithDisplay, TicketError> {
        let current = self.get(ticket_id).await?;
        Self::ensure_not_archived(&current.ticket)?;

        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET assignee_agent_id = $2, updated_at = now()
            WHERE id = $1
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(ticket_id)
        .bind(agent_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        self.enrich_row(&row).await
    }

    pub async fn archive(&self, ticket_id: Uuid) -> Result<TicketWithDisplay, TicketError> {
        let current = self.get(ticket_id).await?;
        if self.ticket_has_active_run(ticket_id).await? {
            return Err(TicketError::ActiveRunExists);
        }
        if current.ticket.archived_at.is_some() {
            return Ok(current);
        }

        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET archived_at = now(), updated_at = now()
            WHERE id = $1 AND archived_at IS NULL
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        self.enrich_row(&row).await
    }

    pub async fn unarchive(&self, ticket_id: Uuid) -> Result<TicketWithDisplay, TicketError> {
        let current = self.get(ticket_id).await?;
        if current.ticket.archived_at.is_none() {
            return Ok(current);
        }

        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET archived_at = NULL, updated_at = now()
            WHERE id = $1 AND archived_at IS NOT NULL
            RETURNING
                id, board_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                pending_assign_recommendation, parent_ticket_id,
                pending_split_recommendation, clarification_round,
                skip_planning, content_version,
                approved_plan_id, approved_plan_content_version,
                created_by, created_by_id, created_at, updated_at, archived_at, attachment_ids
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        self.enrich_row(&row).await
    }

    pub fn ensure_not_archived(ticket: &Ticket) -> Result<(), TicketError> {
        if ticket.archived_at.is_some() {
            Err(TicketError::Archived)
        } else {
            Ok(())
        }
    }

    pub async fn compute_last_activity_at(
        &self,
        ticket_id: Uuid,
    ) -> Result<OffsetDateTime, TicketError> {
        let row = sqlx::query(
            r#"
            SELECT GREATEST(
                t.updated_at,
                COALESCE(
                    (SELECT MAX(c.created_at) FROM ticket_comments c WHERE c.ticket_id = t.id),
                    t.updated_at
                )
            ) AS last_activity_at
            FROM tickets t
            WHERE t.id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        Ok(row.get("last_activity_at"))
    }

    async fn ensure_board_exists(&self, board_id: Uuid) -> Result<(), TicketError> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM boards WHERE id = $1)",
        )
        .bind(board_id)
        .fetch_one(self.pool)
        .await?;

        if !exists {
            return Err(TicketError::BoardNotFound);
        }
        Ok(())
    }

    async fn default_repo_id(&self) -> Result<Option<Uuid>, TicketError> {
        let ids = sqlx::query_scalar::<_, Uuid>("SELECT id FROM repos LIMIT 2")
            .fetch_all(self.pool)
            .await?;

        Ok(match ids.as_slice() {
            [only] => Some(*only),
            _ => None,
        })
    }

    async fn active_ticket_ids_for_board(
        &self,
        board_id: Uuid,
    ) -> Result<HashSet<Uuid>, TicketError> {
        let rows = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT DISTINCT ar.ticket_id
            FROM agent_runs ar
            INNER JOIN tickets t ON t.id = ar.ticket_id
            WHERE t.board_id = $1
              AND ar.status IN ('queued', 'running')
            "#,
        )
        .bind(board_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    pub(crate) async fn ticket_has_active_run(&self, ticket_id: Uuid) -> Result<bool, TicketError> {
        sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM agent_runs
                WHERE ticket_id = $1
                  AND status IN ('queued', 'running')
            )
            "#,
        )
        .bind(ticket_id)
        .fetch_one(self.pool)
        .await
        .map_err(TicketError::from)
    }

    async fn enrich_row(
        &self,
        row: &sqlx::postgres::PgRow,
    ) -> Result<TicketWithDisplay, TicketError> {
        let ticket = row_to_ticket(row);
        let has_active_run = self.ticket_has_active_run(ticket.id).await?;
        self.enrich_ticket(ticket, has_active_run).await
    }

    async fn enrich_ticket(
        &self,
        ticket: Ticket,
        has_active_run: bool,
    ) -> Result<TicketWithDisplay, TicketError> {
        let active = if has_active_run {
            std::collections::HashSet::from([ticket.id])
        } else {
            std::collections::HashSet::new()
        };
        let mut enriched = self.enrich_many(vec![ticket], &active).await?;
        Ok(enriched.remove(0))
    }

    /// Display fields for many tickets in a fixed number of queries, however
    /// many tickets there are. Order is kept.
    async fn enrich_many(
        &self,
        tickets: Vec<Ticket>,
        active: &std::collections::HashSet<Uuid>,
    ) -> Result<Vec<TicketWithDisplay>, TicketError> {
        if tickets.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = tickets.iter().map(|ticket| ticket.id).collect();
        let rows = sqlx::query(
            r#"
            SELECT
                t.id,
                agent.name AS agent_name,
                GREATEST(t.updated_at, COALESCE(last_comment.at, t.updated_at)) AS last_activity_at,
                latest_plan.id AS latest_plan_id,
                latest_plan.content_version AS latest_plan_version,
                plan_run.status AS latest_plan_run_status,
                questions.questions AS plan_questions,
                questions.answers AS plan_answers,
                questions.answered_at IS NULL AS plan_questions_waiting,
                questions.ticket_id IS NOT NULL AS has_plan_questions,
                split.parent_ticket_id IS NOT NULL AS split_parent
            FROM tickets t
            LEFT JOIN agents agent ON agent.id = t.assignee_agent_id
            LEFT JOIN LATERAL (
                SELECT max(c.created_at) AS at FROM ticket_comments c WHERE c.ticket_id = t.id
            ) last_comment ON true
            LEFT JOIN LATERAL (
                SELECT p.id, p.content_version FROM ticket_plans p
                WHERE p.ticket_id = t.id
                ORDER BY p.revision DESC
                LIMIT 1
            ) latest_plan ON true
            LEFT JOIN LATERAL (
                SELECT r.status FROM agent_runs r
                WHERE r.ticket_id = t.id AND r.job_type = $2
                ORDER BY r.created_at DESC
                LIMIT 1
            ) plan_run ON true
            LEFT JOIN ticket_plan_questions questions ON questions.ticket_id = t.id
            LEFT JOIN ticket_plan_splits split ON split.parent_ticket_id = t.id
            WHERE t.id = ANY($1)
            "#,
        )
        .bind(&ids)
        .bind(crate::domain::workflow::JOB_TYPE_PLAN_TICKET)
        .fetch_all(self.pool)
        .await?;
        let mut by_id: std::collections::HashMap<Uuid, sqlx::postgres::PgRow> = rows
            .into_iter()
            .map(|row| (row.get::<Uuid, _>("id"), row))
            .collect();
        let mut reviews = HumanReviewService::new(self.pool).get_many(&ids).await?;
        let planning = crate::services::planning_service::PlanningService::new(self.pool);

        let mut out = Vec::with_capacity(tickets.len());
        for ticket in tickets {
            let row = by_id.remove(&ticket.id).ok_or(TicketError::TicketNotFound)?;
            let agent_name: Option<String> = row.get("agent_name");
            let substatus_display = build_substatus_display(
                ticket.substatus,
                &ticket.substatus_metadata,
                agent_name.as_deref(),
            );
            let latest_plan_id: Option<Uuid> = row.get("latest_plan_id");
            let latest_plan_version: Option<i32> = row.get("latest_plan_version");
            let plan_approved = plan_is_current(&ticket, latest_plan_id, latest_plan_version);
            let plan_questions = if row.get::<bool, _>("has_plan_questions") {
                let questions: serde_json::Value = row.get("plan_questions");
                let answers: Option<serde_json::Value> = row.get("plan_answers");
                Some(crate::services::planning_service::PlanQuestionsState {
                    questions: serde_json::from_value(questions).unwrap_or_default(),
                    answers: answers
                        .and_then(|answers| serde_json::from_value(answers).ok())
                        .unwrap_or_default(),
                    waiting: row.get("plan_questions_waiting"),
                })
            } else {
                None
            };
            let split_parent: bool = row.get("split_parent");
            // Rare: only split parents need the sub-ticket check.
            let verify_available = split_parent
                && planning
                    .verify_available(ticket.id)
                    .await
                    .map_err(plan_error)?;
            out.push(TicketWithDisplay {
                has_active_run: active.contains(&ticket.id),
                has_plan: ticket.status == TicketStatus::PlanReview && latest_plan_id.is_some(),
                human_review: reviews.remove(&ticket.id),
                last_activity_at: row.get("last_activity_at"),
                latest_plan_run_status: row.get("latest_plan_run_status"),
                substatus_display,
                plan_approved,
                plan_questions,
                split_parent,
                verify_available,
                ticket,
            });
        }
        Ok(out)
    }
}

impl<'a> TicketService<'a> {
    pub async fn plan_entry(&self, ticket: &Ticket) -> Result<PlanEntry, TicketError> {
        if ticket.skip_planning {
            return Ok(PlanEntry {
                skip_planning: true,
                approved_for_current_version: false,
            });
        }
        Ok(PlanEntry {
            skip_planning: false,
            approved_for_current_version: self.approved_plan_is_current(ticket).await?,
        })
    }

    pub async fn approved_plan_is_current(&self, ticket: &Ticket) -> Result<bool, TicketError> {
        let (Some(approved_id), Some(approved_version)) = (
            ticket.approved_plan_id,
            ticket.approved_plan_content_version,
        ) else {
            return Ok(false);
        };
        if approved_version != ticket.content_version {
            return Ok(false);
        }
        let row = sqlx::query(
            r#"
            SELECT id, content_version
            FROM ticket_plans
            WHERE ticket_id = $1
            ORDER BY revision DESC
            LIMIT 1
            "#,
        )
        .bind(ticket.id)
        .fetch_optional(self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let id: Uuid = row.get("id");
        let version: i32 = row.get("content_version");
        Ok(id == approved_id && version == ticket.content_version)
    }
}

fn priority_for_create(priority: Option<TicketPriority>) -> TicketPriority {
    priority.unwrap_or(TicketPriority::Medium)
}

fn row_to_ticket(row: &sqlx::postgres::PgRow) -> Ticket {
    let status_str: String = row.get("status");
    let status = status_from_str(&status_str).unwrap_or(TicketStatus::Backlog);

    let substatus: Option<String> = row.get("substatus");
    let substatus = substatus
        .as_deref()
        .and_then(substatus_from_str);

    let priority: Option<String> = row.get("priority");
    let priority = priority.as_deref().and_then(priority_from_str);

    Ticket {
        id: row.get("id"),
        board_id: row.get("board_id"),
        repo_id: row.get("repo_id"),
        title: row.get("title"),
        description: row.get("description"),
        status,
        substatus,
        substatus_metadata: row.get("substatus_metadata"),
        priority,
        assignee_agent_id: row.get("assignee_agent_id"),
        owner_user_id: row.get("owner_user_id"),
        branch_name: row.get("branch_name"),
        pending_assign_recommendation: row.get("pending_assign_recommendation"),
        parent_ticket_id: row.get("parent_ticket_id"),
        pending_split_recommendation: row.get("pending_split_recommendation"),
        clarification_round: row.get("clarification_round"),
        skip_planning: row.get("skip_planning"),
        content_version: row.get("content_version"),
        approved_plan_id: row.get("approved_plan_id"),
        approved_plan_content_version: row.get("approved_plan_content_version"),
        created_by: row.get("created_by"),
        created_by_id: row.get("created_by_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        archived_at: row.get("archived_at"),
        attachment_ids: row.get("attachment_ids"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::comment_service::CommentService;

    #[tokio::test]
    async fn description_attachment_ids_round_trip_and_are_readable() {
        let pool = crate::db::shared_test_pool().await.expect("pool");
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role) VALUES ($1, $2, 'x', 'member')",
        )
        .bind(user_id)
        .bind(format!("attach-{user_id}@example.com"))
        .execute(&pool)
        .await
        .expect("user");

        let board_id = Uuid::new_v4();
        sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
            .bind(board_id)
            .bind("attach board")
            .bind(format!("attach-{board_id}"))
            .execute(&pool)
            .await
            .expect("board");

        let tickets = TicketService::new(&pool);
        let created = tickets
            .create(
                board_id,
                "Shot",
                "see file",
                None,
                None,
                "ada",
                user_id,
            )
            .await
            .expect("ticket");
        assert!(created.ticket.attachment_ids.is_empty());

        let attachment_id = Uuid::new_v4();
        CommentService::new(&pool)
            .create_attachment(
                attachment_id,
                "shot.png",
                "image/png",
                4,
                "unused",
                user_id,
            )
            .await
            .expect("attachment");

        let other = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role) VALUES ($1, $2, 'x', 'member')",
        )
        .bind(other)
        .bind(format!("other-{other}@example.com"))
        .execute(&pool)
        .await
        .expect("other user");

        let updated = tickets
            .set_attachment_ids(created.ticket.id, &[attachment_id])
            .await
            .expect("set");
        assert_eq!(updated.ticket.attachment_ids, vec![attachment_id]);

        let loaded = tickets.get(created.ticket.id).await.expect("get");
        assert_eq!(loaded.ticket.attachment_ids, vec![attachment_id]);

        assert!(CommentService::new(&pool)
            .user_can_access_attachment(attachment_id, other)
            .await
            .expect("access"));
    }

    #[tokio::test]
    async fn create_child_defaults_priority_to_medium() {
        let pool = crate::db::shared_test_pool().await.expect("pool");
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role) VALUES ($1, $2, 'x', 'member')",
        )
        .bind(user_id)
        .bind(format!("child-pri-{user_id}@example.com"))
        .execute(&pool)
        .await
        .expect("user");

        let board_id = Uuid::new_v4();
        sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, $2, $3)")
            .bind(board_id)
            .bind("child priority board")
            .bind(format!("child-pri-{board_id}"))
            .execute(&pool)
            .await
            .expect("board");

        let tickets = TicketService::new(&pool);
        let parent = tickets
            .create(
                board_id,
                "Parent",
                "",
                None,
                Some(TicketPriority::High),
                "ada",
                user_id,
            )
            .await
            .expect("parent");

        let child = tickets
            .create_child(&parent.ticket, "Child", "split work", user_id)
            .await
            .expect("child");
        assert_eq!(child.ticket.priority, Some(TicketPriority::Medium));
        assert_eq!(child.ticket.parent_ticket_id, Some(parent.ticket.id));
    }
}
