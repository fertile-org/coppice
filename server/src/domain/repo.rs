use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Repo {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub remote_url: Option<String>,
    pub default_branch: String,
    pub created_at: OffsetDateTime,
}
