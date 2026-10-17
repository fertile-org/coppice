-- Ticket comments are read newest first, one page at a time (#160).
CREATE INDEX ticket_comments_ticket_created_idx
    ON ticket_comments (ticket_id, created_at DESC, id DESC);
