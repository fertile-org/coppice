//! What picking a design option does, depending on the ticket and its plan.

use crate::copy::plan::{
    plan_design_change_note, DESIGN_SUPERSEDED, NO_PM, PLAN_ALREADY_RUNNING, PLAN_DESIGN_FINAL,
};
use crate::domain::substatus::TicketStatus;
use crate::services::design_service::{DesignError, DesignService, DesignSetSummary};
use crate::services::planning_service::{PlanError, PlanningService};
use crate::services::ticket_service::{TicketError, TicketService};
use sqlx::PgPool;
use std::path::Path;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceMode {
    /// Picking a different option rewrites the plan.
    UpdatePlan,
    /// A pick is stored and nothing else happens.
    SaveOnly,
    PlanRunning,
    PlanFinal,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceState {
    pub mode: ChoiceMode,
    /// The selected option is not the one the current plan was written around.
    pub plan_out_of_date: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum DesignChoiceError {
    #[error(transparent)]
    Design(#[from] DesignError),
    #[error(transparent)]
    Plan(#[from] PlanError),
}

fn refusal(message: &str) -> DesignChoiceError {
    DesignChoiceError::Plan(PlanError::Validation(message.to_string()))
}

pub async fn choice_state(
    pool: &PgPool,
    set: &DesignSetSummary,
    ticket_id: Option<Uuid>,
    inherited: bool,
) -> Result<ChoiceState, DesignChoiceError> {
    let ticket = match ticket_id {
        Some(ticket_id) => Some(
            TicketService::new(pool)
                .get(ticket_id)
                .await
                .map_err(PlanError::from)?
                .ticket,
        ),
        None => None,
    };
    let mode_only = |mode| ChoiceState {
        mode,
        plan_out_of_date: false,
    };

    let archived = ticket
        .as_ref()
        .is_some_and(|ticket| ticket.archived_at.is_some());
    if inherited || set.superseded || archived {
        return Ok(mode_only(ChoiceMode::ReadOnly));
    }
    let Some(ticket) = ticket.filter(|_| set.from_plan_run) else {
        return Ok(mode_only(ChoiceMode::SaveOnly));
    };
    let past_ready = matches!(
        ticket.status,
        TicketStatus::InProgress
            | TicketStatus::InReview
            | TicketStatus::InQa
            | TicketStatus::WaitForFinalReview
            | TicketStatus::Done
    );
    if ticket.approved_plan_id.is_some() || past_ready {
        return Ok(mode_only(ChoiceMode::PlanFinal));
    }
    let planning = PlanningService::new(pool);
    if planning.active_plan_run(ticket.id).await?.is_some() {
        return Ok(mode_only(ChoiceMode::PlanRunning));
    }
    let plan = planning.latest_plan(ticket.id).await?;
    let Some(plan) = plan.filter(|_| ticket.status == TicketStatus::PlanReview) else {
        return Ok(mode_only(ChoiceMode::SaveOnly));
    };

    // A plan with no recorded design counts as up to date.
    let plan_out_of_date = match (&set.selected, plan.design_set_id) {
        (Some(selected), Some(plan_set_id)) => {
            plan_set_id != set.id || plan.design_option_id.as_deref() != Some(selected.id.as_str())
        }
        _ => false,
    };
    Ok(ChoiceState {
        mode: ChoiceMode::UpdatePlan,
        plan_out_of_date,
    })
}

/// Save a pick and, when the plan has to follow it, ask for the rewrite.
///
/// `ask_for_changes` uses the pool and queues a run, so this is not one
/// transaction. The run must see the saved pick, so the save comes first and
/// is undone if the request fails.
pub async fn choose_design(
    pool: &PgPool,
    artifacts: &Path,
    set_id: Uuid,
    option_id: &str,
    user_id: Uuid,
) -> Result<(), DesignChoiceError> {
    let svc = DesignService::new(pool, artifacts);
    let record = svc.get(set_id).await?;
    let set = &record.summary;
    let Some(option) = set.options.iter().find(|option| option.id == option_id) else {
        return Err(DesignError::Invalid("unknown option".into()).into());
    };

    let state = choice_state(pool, set, record.ticket_id, false).await?;
    match state.mode {
        ChoiceMode::ReadOnly if set.superseded => return Err(refusal(DESIGN_SUPERSEDED)),
        ChoiceMode::ReadOnly => return Err(PlanError::from(TicketError::Archived).into()),
        ChoiceMode::PlanRunning => return Err(refusal(PLAN_ALREADY_RUNNING)),
        ChoiceMode::PlanFinal => return Err(refusal(PLAN_DESIGN_FINAL)),
        ChoiceMode::SaveOnly | ChoiceMode::UpdatePlan => {}
    }

    let rewrite_for = match (state.mode, record.ticket_id, &set.selected) {
        (ChoiceMode::UpdatePlan, Some(ticket_id), Some(selected))
            if selected.id != option_id || state.plan_out_of_date =>
        {
            Some(ticket_id)
        }
        _ => None,
    };
    let Some(ticket_id) = rewrite_for else {
        svc.choose(set_id, option_id, user_id).await?;
        return Ok(());
    };

    let planning = PlanningService::new(pool);
    if !planning.planner_available(ticket_id).await? {
        return Err(refusal(NO_PM));
    }
    let earlier = read_chosen(pool, set_id).await?;
    svc.choose(set_id, option_id, user_id).await?;
    if let Err(err) = planning
        .ask_for_changes(ticket_id, &plan_design_change_note(&option.title))
        .await
    {
        if let Err(restore_err) = restore_chosen(pool, set_id, &earlier).await {
            tracing::warn!(error = %restore_err, %set_id, "could not restore the earlier design choice");
        }
        return Err(err.into());
    }
    Ok(())
}

type ChosenColumns = (Option<String>, Option<Uuid>, Option<OffsetDateTime>);

async fn read_chosen(pool: &PgPool, set_id: Uuid) -> Result<ChosenColumns, DesignError> {
    let chosen = sqlx::query_as(
        "SELECT chosen_option_id, chosen_by_user_id, chosen_at FROM design_sets WHERE id = $1",
    )
    .bind(set_id)
    .fetch_one(pool)
    .await?;
    Ok(chosen)
}

async fn restore_chosen(
    pool: &PgPool,
    set_id: Uuid,
    (option_id, user_id, at): &ChosenColumns,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE design_sets
        SET chosen_option_id = $2, chosen_by_user_id = $3, chosen_at = $4
        WHERE id = $1
        "#,
    )
    .bind(set_id)
    .bind(option_id)
    .bind(user_id)
    .bind(at)
    .execute(pool)
    .await?;
    Ok(())
}
