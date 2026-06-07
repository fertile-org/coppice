use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub csrf_token: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct SessionBundle {
    pub session: Session,
    pub session_token: String,
    pub user: crate::domain::user::User,
}
