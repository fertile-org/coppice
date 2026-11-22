use crate::domain::Id;
use time::OffsetDateTime;
#[derive(Debug, Clone)]
pub struct User {
    pub id: Id,
    pub email: String,
    pub role: String,
    pub created_at: OffsetDateTime,
}
