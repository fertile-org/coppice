//! Owners for In Review and In QA when a person moves a card there.
//!
//! An agent handoff picks the owner in `WorkflowService::resolve_transition`.
//! A drag has no recommending agent, so this keeps an eligible assignee or
//! picks the oldest enabled Tech Lead/reviewer (In Review) or QC (In QA), then
//! starts its run. With no eligible agent the ticket stays unassigned and a
//! comment names the missing role.

use crate::config::WorkflowConfig;
use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::ticket::status_to_str;
use crate::domain::workflow::{
    is_owned_stage, is_stage_eligible, stage_owner_for, stage_owner_label,
};
use crate::services::agent_service::AgentService;
use crate::services::comment_service::CommentService;
use crate::services::mention_service::resolve_agent_keys;
use crate::services::planning_service::{PlanError, PlanningService};
use crate::services::run_orchestrator::board_agent_infos;
use crate::services::run_service::{RunError, RunService, StartRunOptions};
use crate::services::ticket_service::TicketService;
use sqlx::PgPool;
use uuid::Uuid;

pub struct StageOwnerService<'a> {
    pool: &'a PgPool,
    workflow: &'a WorkflowConfig,
}

impl<'a> StageOwnerService<'a> {
    pub fn new(pool: &'a PgPool, workflow: &'a WorkflowConfig) -> Self {
        Self { pool, workflow }
    }

    /// A person moved the ticket into In Review or In QA. A ticket with a
    /// queued or running run is left alone: that run finishes first and the
    /// workflow gate hands off from there.
    pub async fn on_entered_review_stage(&self, ticket_id: Uuid) -> Result<(), PlanError> {
        let tickets = TicketService::new(self.pool);
        let ticket = tickets.get(ticket_id).await?.ticket;
        let status = ticket.status;
        if !is_owned_stage(status)
            || ticket.repo_id.is_none()
            || !self.workflow.auto_start_runs
            || !self.workflow.auto_assign.effective(status_to_str(status))
            || tickets.ticket_has_active_run(ticket_id).await?
            || PlanningService::new(self.pool)
                .is_split_parent(ticket_id)
                .await?
        {
            return Ok(());
        }

        let agents = AgentService::new(self.pool)
            .list_agents()
            .await
            .map_err(|err| PlanError::Validation(err.to_string()))?;
        let board = board_agent_infos(&agents, &resolve_agent_keys(&agents));
        let current = ticket
            .assignee_agent_id
            .and_then(|id| board.iter().find(|agent| agent.id == id))
            .filter(|agent| is_stage_eligible(status, &agent.key, &agent.role));
        let Some(owner) = current.or_else(|| stage_owner_for(status, &board, None)) else {
            if ticket.assignee_agent_id.is_some() {
                TicketService::new(self.pool)
                    .assign_agent(ticket_id, None)
                    .await?;
            }
            let label = stage_owner_label(status);
            return self
                .note(
                    ticket_id,
                    &format!(
                        "No enabled {label} agent is on this board, so this ticket is unassigned and no run was started. Add or enable a {label} agent, then assign it."
                    ),
                )
                .await;
        };
        let owner_id = owner.id;
        if ticket.assignee_agent_id != Some(owner_id) {
            TicketService::new(self.pool)
                .assign_agent(ticket_id, Some(owner_id))
                .await?;
        }

        match RunService::new(self.pool)
            .start_run_for_agent(
                ticket_id,
                owner_id,
                "work_on_ticket",
                StartRunOptions::default(),
            )
            .await
        {
            Ok(_) | Err(RunError::ActiveRunExists) | Err(RunError::NotFound) => Ok(()),
            Err(RunError::Validation(message)) => {
                tracing::info!(%ticket_id, %message, "stage owner run did not start");
                Ok(())
            }
            Err(RunError::Database(err)) => Err(PlanError::Database(err)),
        }
    }

    async fn note(&self, ticket_id: Uuid, body: &str) -> Result<(), PlanError> {
        CommentService::new(self.pool)
            .create(
                ticket_id,
                AuthorType::System,
                None,
                body,
                CommentIntent::SystemEvent,
                &[],
                &[],
            )
            .await?;
        Ok(())
    }
}
