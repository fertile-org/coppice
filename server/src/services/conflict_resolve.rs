//! Ask an eligible agent to resolve a merge or rebase conflict.
//!
//! Eligible means enabled and either a Technical Lead or an implementer
//! (`is_implementer && !is_qc && !tech lead`). The run is a normal
//! `work_on_ticket` run for the chosen agent. A merge conflict on a Done
//! ticket changes nothing until a person asks; see [`start_resolve`].

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::copy::conflict::{
    ask_to_resolve_label, conflict_message, connector_not_ready_reason,
    merge_conflict_reopen_comment, other_agent_active_reason, resolve_prompt, AGENT_NOT_ELIGIBLE,
    INVALID_FILE_LIST, MERGE_ACTION, NO_ELIGIBLE_AGENT, REBASE_ACTION, REREVIEW_NOTE,
};
use crate::domain::agent::Agent;
use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::context_profile::ContextProfile;
use crate::domain::run::AgentRun;
use crate::domain::slug::slugify;
use crate::domain::substatus::TicketStatus;
use crate::domain::workflow::{is_qc, is_tech_lead_identity};
use crate::services::agent_service::{AgentError, AgentService};
use crate::services::comment_service::{CommentError, CommentService};
use crate::services::run_service::{RunError, RunService, StartRunOptions};
use crate::services::ticket_git_service::{validate_branch_name, GitConflictKind};
use crate::services::ticket_service::{TicketError, TicketService};
use crate::services::workflow_service::is_implementer;

const JOB_TYPE_WORK_ON_TICKET: &str = "work_on_ticket";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAgentJson {
    pub id: Uuid,
    pub name: String,
    pub ask_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictOfferJson {
    pub operation: &'static str,
    pub base_branch: String,
    pub files: Vec<String>,
    pub can_ask_assignee: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent_id: Option<Uuid>,
    pub agents: Vec<ConflictAgentJson>,
    pub rereview_note: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitConflictHttp {
    pub message: String,
    pub conflict: ConflictOfferJson,
}

pub struct ResolveConflictOutcome {
    pub run: AgentRun,
    pub new_comment_id: Option<Uuid>,
    /// The ticket moved from Done to In Progress.
    pub reopened: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConflictResolveError {
    #[error("no eligible agent")]
    NoEligibleAgent,
    #[error("agent not eligible")]
    AgentNotEligible,
    #[error("connector not ready")]
    ConnectorNotReady { assignee: String },
    #[error("other agent active")]
    OtherAgentActive { name: String },
    #[error("invalid branch")]
    InvalidBranch,
    #[error("invalid files")]
    InvalidFiles,
    #[error(transparent)]
    Ticket(#[from] TicketError),
    #[error(transparent)]
    Agent(#[from] AgentError),
    #[error(transparent)]
    Run(#[from] RunError),
    #[error(transparent)]
    Comment(#[from] CommentError),
}

pub fn invalid_file_list_message() -> &'static str {
    INVALID_FILE_LIST
}

pub fn no_eligible_agent_message() -> &'static str {
    NO_ELIGIBLE_AGENT
}

pub fn agent_not_eligible_message() -> &'static str {
    AGENT_NOT_ELIGIBLE
}

struct ResolverOffer {
    agents: Vec<Agent>,
    default_id: Option<Uuid>,
}

impl ResolverOffer {
    fn default_agent(&self) -> Option<&Agent> {
        self.default_id
            .and_then(|id| self.agents.iter().find(|agent| agent.id == id))
    }

    fn get(&self, id: Uuid) -> Option<&Agent> {
        self.agents.iter().find(|agent| agent.id == id)
    }
}

fn agent_key(agent: &Agent) -> String {
    agent
        .preset_source
        .clone()
        .unwrap_or_else(|| slugify(&agent.name))
}

fn is_conflict_tech_lead(agent: &Agent) -> bool {
    is_tech_lead_identity(&agent_key(agent), &agent.role)
}

fn is_conflict_implementer(agent: &Agent) -> bool {
    is_implementer(&agent.role) && !is_qc(&agent.role) && !is_conflict_tech_lead(agent)
}

fn is_eligible_resolver(agent: &Agent) -> bool {
    agent.enabled && (is_conflict_tech_lead(agent) || is_conflict_implementer(agent))
}

/// Eligible assignee, then the newest enabled implementer with a
/// `work_on_ticket` + `full` run, then the oldest enabled Technical Lead,
/// then the first eligible agent.
fn preselect_agent_id(
    eligible: &[&Agent],
    assignee_id: Option<Uuid>,
    work_run_agent_ids_newest_first: &[Uuid],
) -> Option<Uuid> {
    if let Some(id) = assignee_id {
        if eligible.iter().any(|agent| agent.id == id) {
            return Some(id);
        }
    }
    for agent_id in work_run_agent_ids_newest_first {
        if eligible
            .iter()
            .any(|agent| agent.id == *agent_id && is_conflict_implementer(agent))
        {
            return Some(*agent_id);
        }
    }
    if let Some(tech_lead) = eligible.iter().find(|agent| is_conflict_tech_lead(agent)) {
        return Some(tech_lead.id);
    }
    eligible.first().map(|agent| agent.id)
}

fn conflict_agent_json(agent: &Agent, connector_ready: bool) -> ConflictAgentJson {
    ConflictAgentJson {
        id: agent.id,
        name: agent.name.clone(),
        ask_label: ask_to_resolve_label(&agent.name),
        unavailable_reason: if connector_ready {
            None
        } else {
            Some(connector_not_ready_reason(&agent.name))
        },
    }
}

fn offer_json(
    kind: GitConflictKind,
    base_branch: String,
    files: Vec<String>,
    offer: &ResolverOffer,
    connector_ready: &(dyn Fn(&str) -> bool + Sync),
) -> ConflictOfferJson {
    let agents: Vec<ConflictAgentJson> = offer
        .agents
        .iter()
        .map(|agent| conflict_agent_json(agent, connector_ready(&agent.connector)))
        .collect();
    let default_agent = offer.default_agent();
    let default_json =
        default_agent.and_then(|agent| agents.iter().find(|entry| entry.id == agent.id).cloned());
    let can_ask_assignee = agents
        .iter()
        .any(|agent| agent.unavailable_reason.is_none());
    let (ask_label, unavailable_reason) = if agents.is_empty() {
        (None, Some(NO_ELIGIBLE_AGENT.to_string()))
    } else {
        (
            default_json.as_ref().map(|agent| agent.ask_label.clone()),
            default_json
                .as_ref()
                .and_then(|agent| agent.unavailable_reason.clone()),
        )
    };
    ConflictOfferJson {
        operation: kind.as_str(),
        base_branch,
        files,
        can_ask_assignee,
        ask_label,
        unavailable_reason,
        default_agent_id: offer.default_id,
        agents,
        rereview_note: REREVIEW_NOTE,
    }
}

async fn implementer_work_run_agent_ids(
    pool: &PgPool,
    ticket_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT agent_id
        FROM agent_runs
        WHERE ticket_id = $1
          AND job_type = 'work_on_ticket'
          AND context_profile = 'full'
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|row| row.0).collect())
}

async fn load_resolver_offer(
    pool: &PgPool,
    ticket_id: Uuid,
) -> Result<ResolverOffer, ConflictResolveError> {
    let ticket = TicketService::new(pool).get(ticket_id).await?;
    let agents = AgentService::new(pool).list_agents().await?;
    let eligible: Vec<Agent> = agents.into_iter().filter(is_eligible_resolver).collect();
    let work_runs = implementer_work_run_agent_ids(pool, ticket_id)
        .await
        .map_err(RunError::from)?;
    let eligible_refs: Vec<&Agent> = eligible.iter().collect();
    let default_id =
        preselect_agent_id(&eligible_refs, ticket.ticket.assignee_agent_id, &work_runs);
    Ok(ResolverOffer {
        agents: eligible,
        default_id,
    })
}

pub async fn build_git_conflict_response(
    pool: &PgPool,
    ticket_id: Uuid,
    kind: GitConflictKind,
    base_branch: String,
    files: Vec<String>,
    connector_ready: &(dyn Fn(&str) -> bool + Sync),
) -> GitConflictHttp {
    let action = match kind {
        GitConflictKind::Merge => MERGE_ACTION,
        GitConflictKind::Rebase => REBASE_ACTION,
    };
    let message = conflict_message(action, &base_branch, &files);
    let offer = match load_resolver_offer(pool, ticket_id).await {
        Ok(offer) => offer,
        Err(err) => {
            tracing::warn!(error = %err, "conflict offer could not load resolvers");
            ResolverOffer {
                agents: vec![],
                default_id: None,
            }
        }
    };
    GitConflictHttp {
        message,
        conflict: offer_json(kind, base_branch, files, &offer, connector_ready),
    }
}

/// Ask an eligible agent to resolve a conflict.
///
/// `requested_agent_id` is the picker selection. When it is missing, the
/// preselection rule is used. A merge conflict on a Done ticket moves it to
/// In Progress in the same transaction that queues that agent's run, so the
/// ticket never leaves Done without a run. Any other conflict starts a
/// resolve run and leaves the status and assignee.
pub async fn start_resolve(
    pool: &PgPool,
    ticket_id: Uuid,
    kind: GitConflictKind,
    base_branch: &str,
    files: &[String],
    requested_agent_id: Option<Uuid>,
    connector_ready: &(dyn Fn(&str) -> bool + Sync),
) -> Result<ResolveConflictOutcome, ConflictResolveError> {
    let base_branch = base_branch.trim();
    validate_branch_name(base_branch).map_err(|_| ConflictResolveError::InvalidBranch)?;
    let files = clean_conflict_files(files)?;

    let ticket = TicketService::new(pool).get(ticket_id).await?;
    TicketService::ensure_not_archived(&ticket.ticket)?;
    let offer = load_resolver_offer(pool, ticket_id).await?;
    let agent = match requested_agent_id {
        Some(id) => offer
            .get(id)
            .cloned()
            .ok_or(ConflictResolveError::AgentNotEligible)?,
        None => offer
            .default_agent()
            .cloned()
            .ok_or(ConflictResolveError::NoEligibleAgent)?,
    };
    if !connector_ready(&agent.connector) {
        return Err(ConflictResolveError::ConnectorNotReady {
            assignee: agent.name,
        });
    }

    let run_svc = RunService::new(pool);
    let active = run_svc.active_for_ticket(ticket_id).await?;
    if let Some(existing) = active.iter().find(|run| run.agent_id == agent.id) {
        return Ok(ResolveConflictOutcome {
            run: existing.clone(),
            new_comment_id: None,
            reopened: false,
        });
    }
    if let Some(other) = active.first() {
        let other_agent = AgentService::new(pool).get(other.agent_id).await?;
        return Err(ConflictResolveError::OtherAgentActive {
            name: other_agent.name,
        });
    }

    let reopen =
        matches!(kind, GitConflictKind::Merge) && ticket.ticket.status == TicketStatus::Done;
    let (body, options) = if reopen {
        (
            merge_conflict_reopen_comment(base_branch, &files, &agent.name),
            StartRunOptions {
                context_profile: ContextProfile::Full,
                trigger_comment_id: None,
                reopen_done_after_merge_conflict: true,
                reopen_assignee_id: Some(agent.id),
            },
        )
    } else {
        (
            resolve_prompt(base_branch, &files),
            StartRunOptions {
                context_profile: ContextProfile::HumanAgent,
                ..StartRunOptions::default()
            },
        )
    };
    let comments = CommentService::new(pool);
    let comment = comments
        .create(
            ticket_id,
            AuthorType::System,
            None,
            &body,
            CommentIntent::SystemEvent,
            &[],
            &[],
        )
        .await?;

    match run_svc
        .start_run_for_agent(
            ticket_id,
            agent.id,
            JOB_TYPE_WORK_ON_TICKET,
            StartRunOptions {
                trigger_comment_id: Some(comment.id),
                ..options
            },
        )
        .await
    {
        Ok(run) => Ok(ResolveConflictOutcome {
            run,
            new_comment_id: Some(comment.id),
            reopened: reopen,
        }),
        Err(RunError::ActiveRunExists) => {
            discard_comment(&comments, comment.id).await;
            let active = run_svc.active_for_ticket(ticket_id).await?;
            if let Some(existing) = active.iter().find(|run| run.agent_id == agent.id) {
                return Ok(ResolveConflictOutcome {
                    run: existing.clone(),
                    new_comment_id: None,
                    reopened: false,
                });
            }
            if let Some(other) = active.first() {
                let other_agent = AgentService::new(pool).get(other.agent_id).await?;
                return Err(ConflictResolveError::OtherAgentActive {
                    name: other_agent.name,
                });
            }
            Err(RunError::ActiveRunExists.into())
        }
        Err(err) => {
            discard_comment(&comments, comment.id).await;
            Err(err.into())
        }
    }
}

/// The run did not start, so its trigger comment would describe something
/// that did not happen.
async fn discard_comment(comments: &CommentService<'_>, comment_id: Uuid) {
    if let Err(err) = comments.delete(comment_id).await {
        tracing::error!(error = %err, %comment_id, "could not remove the resolve comment");
    }
}

fn clean_conflict_files(files: &[String]) -> Result<Vec<String>, ConflictResolveError> {
    let mut cleaned = Vec::new();
    for file in files {
        let file = file.trim();
        if file.is_empty() {
            continue;
        }
        if file.len() > 500 || file.chars().any(char::is_control) {
            return Err(ConflictResolveError::InvalidFiles);
        }
        cleaned.push(file.to_string());
    }
    if cleaned.len() > 100 {
        return Err(ConflictResolveError::InvalidFiles);
    }
    cleaned.sort();
    cleaned.dedup();
    Ok(cleaned)
}

pub fn other_agent_active_message(name: &str) -> String {
    other_agent_active_reason(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn agent(
        id: u128,
        name: &str,
        role: &str,
        preset: Option<&str>,
        enabled: bool,
        created: i64,
    ) -> Agent {
        Agent {
            id: Uuid::from_u128(id),
            name: name.to_string(),
            role: role.to_string(),
            responsibilities: vec![],
            system_prompt: String::new(),
            connector: "mock".into(),
            model_provider: None,
            model: None,
            enabled,
            preset_source: preset.map(str::to_string),
            created_at: OffsetDateTime::from_unix_timestamp(created).unwrap(),
            updated_at: OffsetDateTime::from_unix_timestamp(created).unwrap(),
        }
    }

    fn frontend() -> Agent {
        agent(
            1,
            "Frontend Engineer",
            "Frontend Engineer",
            Some("frontend_engineer"),
            true,
            10,
        )
    }

    fn backend() -> Agent {
        agent(
            2,
            "Backend Engineer",
            "Backend Engineer",
            Some("backend_engineer"),
            true,
            20,
        )
    }

    fn tech_lead() -> Agent {
        agent(
            3,
            "Technical Lead",
            "Technical Lead",
            Some("tech_lead"),
            true,
            5,
        )
    }

    fn later_tech_lead() -> Agent {
        agent(
            4,
            "Second Lead",
            "Technical Lead",
            Some("tech_lead"),
            true,
            50,
        )
    }

    fn qc() -> Agent {
        agent(5, "QC Engineer", "QC", Some("qc"), true, 15)
    }

    fn quality_engineer() -> Agent {
        agent(
            6,
            "Quality Engineer",
            "Quality Engineer",
            Some("qc"),
            true,
            16,
        )
    }

    fn pm() -> Agent {
        agent(7, "PM", "PM", Some("pm"), true, 1)
    }

    #[test]
    fn eligibility_excludes_qc_pm_and_disabled() {
        assert!(is_eligible_resolver(&frontend()));
        assert!(is_eligible_resolver(&backend()));
        assert!(is_eligible_resolver(&tech_lead()));
        assert!(is_conflict_implementer(&frontend()));
        assert!(!is_conflict_implementer(&tech_lead()));
        assert!(!is_eligible_resolver(&qc()));
        assert!(!is_eligible_resolver(&quality_engineer()));
        assert!(!is_eligible_resolver(&pm()));
        let mut disabled = frontend();
        disabled.enabled = false;
        assert!(!is_eligible_resolver(&disabled));
    }

    #[test]
    fn preselect_prefers_eligible_assignee() {
        let fe = frontend();
        let be = backend();
        let tl = tech_lead();
        let eligible = vec![&tl, &be, &fe];
        assert_eq!(
            preselect_agent_id(&eligible, Some(fe.id), &[be.id]),
            Some(fe.id)
        );
    }

    #[test]
    fn preselect_skips_pm_or_disabled_assignee() {
        let fe = frontend();
        let tl = tech_lead();
        let pm = pm();
        let eligible = vec![&tl, &fe];
        assert_eq!(
            preselect_agent_id(&eligible, Some(pm.id), &[fe.id]),
            Some(fe.id)
        );
        let mut disabled_fe = fe.clone();
        disabled_fe.enabled = false;
        let eligible = vec![&tl];
        assert_eq!(
            preselect_agent_id(&eligible, Some(disabled_fe.id), &[disabled_fe.id]),
            Some(tl.id)
        );
    }

    #[test]
    fn preselect_uses_newest_implementer_work_run() {
        let fe = frontend();
        let be = backend();
        let tl = tech_lead();
        let qc = qc();
        let eligible = vec![&tl, &be, &fe];
        assert_eq!(
            preselect_agent_id(&eligible, None, &[qc.id, be.id, fe.id]),
            Some(be.id)
        );
    }

    #[test]
    fn preselect_ignores_non_implementer_run_ids() {
        let fe = frontend();
        let tl = tech_lead();
        let qc = qc();
        let eligible = vec![&tl, &fe];
        assert_eq!(
            preselect_agent_id(&eligible, None, &[qc.id, tl.id, fe.id]),
            Some(fe.id)
        );
        assert_eq!(
            preselect_agent_id(&eligible, None, &[qc.id, tl.id]),
            Some(tl.id)
        );
    }

    #[test]
    fn preselect_falls_back_to_oldest_tech_lead() {
        let fe = frontend();
        let tl = tech_lead();
        let later = later_tech_lead();
        let eligible = vec![&tl, &later, &fe];
        assert_eq!(preselect_agent_id(&eligible, None, &[]), Some(tl.id));
    }

    #[test]
    fn preselect_falls_back_to_first_eligible() {
        let fe = frontend();
        let be = backend();
        let eligible = vec![&be, &fe];
        assert_eq!(preselect_agent_id(&eligible, None, &[]), Some(be.id));
    }
}
