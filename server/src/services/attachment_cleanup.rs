//! Removes attachment rows and files that nothing references (#165).
//!
//! An attachment is live while a ticket description, a ticket comment, or a
//! chat message lists its id. Removing it from a ticket deletes it right away
//! once unreferenced; an hourly sweep catches uploads that were never
//! attached (a cancelled composer, for example).

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use crate::storage::attachment_store::AttachmentStore;

const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// An upload gets this long to be attached before the sweep removes it.
const UNATTACHED_GRACE_HOURS: i32 = 24;

const UNREFERENCED: &str = r#"
    NOT EXISTS (SELECT 1 FROM tickets t WHERE a.id = ANY(t.attachment_ids))
    AND NOT EXISTS (SELECT 1 FROM ticket_comments c WHERE a.id = ANY(c.attachment_ids))
    AND NOT EXISTS (SELECT 1 FROM chat_messages m WHERE a.id = ANY(m.attachment_ids))
"#;

/// Delete the given attachments that are no longer referenced. Returns how many.
pub async fn delete_if_unreferenced(
    pool: &PgPool,
    store: &AttachmentStore,
    ids: &[Uuid],
) -> Result<usize, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let deleted: Vec<Uuid> = sqlx::query_scalar(&format!(
        "DELETE FROM attachments a WHERE a.id = ANY($1) AND {UNREFERENCED} RETURNING a.id"
    ))
    .bind(ids)
    .fetch_all(pool)
    .await?;
    remove_files(store, &deleted);
    Ok(deleted.len())
}

/// Delete attachments older than the grace period that nothing references.
pub async fn sweep(pool: &PgPool, store: &AttachmentStore) -> Result<usize, sqlx::Error> {
    let deleted: Vec<Uuid> = sqlx::query_scalar(&format!(
        "DELETE FROM attachments a
         WHERE a.created_at < now() - make_interval(hours => $1) AND {UNREFERENCED}
         RETURNING a.id"
    ))
    .bind(UNATTACHED_GRACE_HOURS)
    .fetch_all(pool)
    .await?;
    remove_files(store, &deleted);
    Ok(deleted.len())
}

fn remove_files(store: &AttachmentStore, ids: &[Uuid]) {
    for id in ids {
        if let Err(err) = store.delete(*id) {
            tracing::warn!(attachment_id = %id, error = %err, "could not remove attachment file");
        }
    }
}

pub fn spawn_sweep(state: Arc<crate::AppState>) {
    let Some(pool) = state.db.clone() else {
        return;
    };
    tokio::spawn(async move {
        loop {
            match sweep(&pool, &state.attachments).await {
                Ok(0) => {}
                Ok(removed) => tracing::info!(removed, "removed unattached uploads"),
                Err(err) => tracing::warn!(error = %err, "attachment sweep failed"),
            }
            tokio::time::sleep(SWEEP_INTERVAL).await;
        }
    });
}
