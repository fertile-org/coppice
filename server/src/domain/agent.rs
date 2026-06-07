use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AgentPreset {
    pub id: Uuid,
    pub key: String,
    pub role: String,
    pub skills: Vec<String>,
    pub responsibilities: Vec<String>,
    pub system_prompt_template: String,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub role: String,
    pub skills: Vec<String>,
    pub responsibilities: Vec<String>,
    pub system_prompt: String,
    pub provider_id: String,
    pub enabled: bool,
    pub preset_source: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
