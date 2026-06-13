use time::format_description::well_known::Rfc3339;

use crate::domain::ticket::{status_to_str, substatus_to_str};
use crate::events::bus::{AppEvent, EventBus};
use crate::services::ticket_service::TicketWithDisplay;

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
