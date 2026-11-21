use crate::domain::Id;
use time::OffsetDateTime;
#[derive(Debug, Clone)]
pub struct Attachment {
    pub id: Id,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
    pub uploaded_by: Id,
    pub created_at: OffsetDateTime,
}
