use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Board {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: OffsetDateTime,
    pub review_cap: Option<i32>,
    pub count_in_review: bool,
    pub diff_budget: Option<i32>,
}
