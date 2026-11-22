use crate::domain::Id;
use time::OffsetDateTime;
#[derive(Debug, Clone)]
pub struct Board {
    pub id: Id,
    pub name: String,
    pub slug: String,
    pub created_at: OffsetDateTime,
    pub review_cap: Option<i32>,
    pub count_in_review: bool,
}
