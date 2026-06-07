use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub created_at: OffsetDateTime,
}
