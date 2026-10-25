use std::cmp::Ordering;
use time::OffsetDateTime;
use uuid::Uuid;

// Board order for preset roles. A new preset seed must be added here, or it sorts as custom.
const PRESET_ORDER: [&str; 10] = [
    "pm",
    "tech_lead",
    "backend_engineer",
    "frontend_engineer",
    "qc",
    "reviewer",
    "security",
    "dba",
    "devops",
    "research",
];
const CUSTOM_PRESET_RANK: u8 = PRESET_ORDER.len() as u8;

pub fn preset_rank(preset_source: Option<&str>) -> u8 {
    preset_source
        .and_then(|key| PRESET_ORDER.iter().position(|preset| *preset == key))
        .map_or(CUSTOM_PRESET_RANK, |index| index as u8)
}

pub fn compare_agents_for_board(a: &Agent, b: &Agent) -> Ordering {
    let rank_a = preset_rank(a.preset_source.as_deref());
    let rank_b = preset_rank(b.preset_source.as_deref());
    rank_a
        .cmp(&rank_b)
        .then_with(|| {
            if rank_a == CUSTOM_PRESET_RANK {
                a.role.to_lowercase().cmp(&b.role.to_lowercase())
            } else {
                Ordering::Equal
            }
        })
        .then_with(|| a.created_at.cmp(&b.created_at))
        .then_with(|| a.id.cmp(&b.id))
}

#[derive(Debug, Clone)]
pub struct AgentPreset {
    pub id: Uuid,
    pub key: String,
    pub role: String,
    pub responsibilities: Vec<String>,
    pub default_plugins: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub role: String,
    pub responsibilities: Vec<String>,
    pub system_prompt: String,
    pub connector: String,
    pub model_provider: Option<String>,
    pub model: Option<String>,
    pub enabled: bool,
    pub preset_source: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: u128, role: &str, preset: Option<&str>, created_at: i64) -> Agent {
        Agent {
            id: Uuid::from_u128(id),
            name: role.to_string(),
            role: role.to_string(),
            responsibilities: vec![],
            system_prompt: String::new(),
            connector: "mock".into(),
            model_provider: None,
            model: None,
            enabled: true,
            preset_source: preset.map(str::to_string),
            created_at: OffsetDateTime::from_unix_timestamp(created_at).unwrap(),
            updated_at: OffsetDateTime::from_unix_timestamp(created_at).unwrap(),
        }
    }

    #[test]
    fn preset_rank_follows_board_order() {
        assert_eq!(preset_rank(Some("pm")), 0);
        assert_eq!(preset_rank(Some("tech_lead")), 1);
        assert_eq!(preset_rank(Some("backend_engineer")), 2);
        assert_eq!(preset_rank(Some("research")), 9);
    }

    #[test]
    fn unknown_and_missing_presets_rank_last() {
        assert_eq!(preset_rank(Some("renamed_engineer")), CUSTOM_PRESET_RANK);
        assert_eq!(preset_rank(None), CUSTOM_PRESET_RANK);
    }

    #[test]
    fn compare_orders_by_role_then_creation_then_id() {
        let mut agents = vec![
            agent(1, "Custom Zed", None, 10),
            agent(2, "Research", Some("research"), 10),
            agent(3, "Backend Engineer", Some("backend_engineer"), 30),
            agent(4, "Backend Engineer", Some("backend_engineer"), 20),
            agent(5, "PM", Some("pm"), 10),
            agent(6, "custom alpha", Some("old_key"), 10),
        ];
        agents.sort_by(compare_agents_for_board);
        let ids: Vec<u128> = agents.iter().map(|a| a.id.as_u128()).collect();
        assert_eq!(ids, vec![5, 4, 3, 2, 6, 1]);
    }

    #[test]
    fn equal_created_at_falls_back_to_id() {
        let mut agents = vec![
            agent(9, "QC", Some("qc"), 10),
            agent(7, "QC", Some("qc"), 10),
        ];
        agents.sort_by(compare_agents_for_board);
        let ids: Vec<u128> = agents.iter().map(|a| a.id.as_u128()).collect();
        assert_eq!(ids, vec![7, 9]);
    }
}
