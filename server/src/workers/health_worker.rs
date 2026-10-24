use std::sync::Arc;
use std::time::Duration;

use crate::services::agent_health::evaluate_agent_health;
use crate::services::agent_service::AgentService;
use crate::AppState;

pub fn spawn_health_worker(state: Arc<AppState>) {
    let interval_secs = state.config.agent.health_check_interval_secs.max(10);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        // Before the startup connector check, Automatic connectors look off.
        state
            .connectors
            .wait_readiness_checked(crate::connectors_runtime::READINESS_WAIT)
            .await;
        let mut changes = state.connectors.subscribe_changes();
        changes.mark_unchanged();
        run_health_pass_once(&state).await;

        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs as u64));
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                changed = changes.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
            }
            run_health_pass_once(&state).await;
        }
    });
}

pub async fn run_health_pass_once(state: &AppState) {
    let Some(pool) = state.db.as_ref() else {
        return;
    };
    let service = AgentService::new(pool);
    let Ok(agents) = service.list_agents().await else {
        return;
    };

    for agent in agents {
        state.agent_health.ensure_agent(agent.id);
        let registry = state.connectors.registry();
        let (status, detail) = evaluate_agent_health(&agent, registry.as_ref()).await;
        state.agent_health.set(agent.id, status, detail);
    }
}
