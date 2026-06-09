#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentHealthStatus {
    Unknown,
    Healthy,
    MissingConfig,
    Unhealthy,
}

pub fn health_status_to_str(s: AgentHealthStatus) -> &'static str {
    match s {
        AgentHealthStatus::Unknown => "unknown",
        AgentHealthStatus::Healthy => "healthy",
        AgentHealthStatus::MissingConfig => "missing_config",
        AgentHealthStatus::Unhealthy => "unhealthy",
    }
}
