use sqlx::PgPool;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::domain::run::{run_status_to_str, RunStatus};
use crate::domain::ticket::{status_to_str, substatus_to_str};
use crate::events::bus::{AppEvent, EventBus};
use crate::services::notification_service::NotificationService;
use crate::services::ticket_service::TicketWithDisplay;
use crate::AppState;

pub fn publish_ticket_updated(bus: &EventBus, ticket: &TicketWithDisplay) {
    bus.publish(AppEvent::TicketUpdated {
        ticket_id: ticket.ticket.id,
        status: status_to_str(ticket.ticket.status).into(),
        substatus: ticket
            .ticket
            .substatus
            .map(|s| substatus_to_str(s).into()),
        updated_at: ticket
            .ticket
            .updated_at
            .format(&Rfc3339)
            .unwrap_or_default(),
    });
}

pub async fn publish_run_finished(
    state: &AppState,
    pool: &PgPool,
    run_id: Uuid,
    ticket_id: Uuid,
    agent_id: Uuid,
    status: RunStatus,
    error_message: Option<String>,
) {
    state.event_bus.publish(AppEvent::AgentRunFinished {
        run_id,
        ticket_id,
        agent_id,
        status: run_status_to_str(status).into(),
        error_message,
    });

    // Persist durable in-app notifications for the four terminal statuses.
    // Failures are non-fatal: a missing notification row is preferable to a
    // dropped run-completion signal.
    let status_str = run_status_to_str(status);
    if let Err(err) = NotificationService::new(pool)
        .create_for_run_finished(run_id, ticket_id, agent_id, status_str)
        .await
    {
        tracing::warn!(error = %err, %run_id, "failed to create run-finished notification");
    } else {
        state.event_bus.publish(AppEvent::NotificationChanged {
            recipient_user_id: None,
        });
    }
}
