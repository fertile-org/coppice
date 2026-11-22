use crate::domain::Id;
use time::OffsetDateTime;
#[derive(Debug, Clone)]
pub struct Session {
    pub id: Id,
    pub user_id: Id,
    pub csrf_token: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct SessionBundle {
    pub session: Session,
    pub session_token: String,
    pub user: crate::domain::user::User,
}
