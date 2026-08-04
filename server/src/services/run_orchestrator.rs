use std::collections::{HashMap, HashSet};

use crate::config::WorkflowConfig;
use crate::domain::agent::Agent;
use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::context_profile::ContextProfile;
use crate::domain::mention::TicketMention;
use crate::domain::run::{AgentRun, RunStatus};
use crate::domain::slug::slugify;
use crate::domain::substatus::Substatus;
use crate::domain::ticket::status_to_str;
use crate::domain::workflow::{JobRequest, RunOutcome, TransitionAction, TransitionContext};
use crate::events::{AppEvent, EventBus};
use crate::providers::AgentRunResult;
use crate::services::agent_request::{
    normalized_agent_requests, replace_agent_requests_in_comment, ResolvedAgentRequest,
};
use crate::services::agent_service::AgentService;
use crate::services::comment_service::{CommentError, CommentService};
use crate::services::mention_service::{resolve_agent_keys, MentionService};
use crate::services::notification_service::NotificationService;
use crate::services::result_contract::{merge_ticket_description, ApplyResult};
use crate::services::run_service::{RunError, RunService, StartRunOptions};
use crate::services::split_service::SplitService;
use crate::services::ticket_service::TicketService;
use crate::services::ticket_thread;
use crate::services::workflow_service::{
    is_implementer, WorkflowService, MAX_CLARIFICATION_ROUNDS, MAX_MENTIONS_PER_RUN,
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub struct RunOrchestrator<'a> {
    pool: &'a PgPool,
    workflow: &'a WorkflowConfig,
    event_bus: Option<&'a EventBus>,
}

pub async fn load_run_continuation_context(
    pool: &PgPool,
    run: &AgentRun,
) -> Result<Option<String>, CommentError> {
    if run.context_profile != ContextProfile::Full {
        return Ok(None);
    }

    if run.job_type != "work_on_ticket" && run.job_type != "respond_to_mention" {
        return Ok(None);
    }

    let comments = CommentService::new(pool)
        .list_by_ticket(run.ticket_id)
        .await?;

    let agent_names = AgentService::new(pool)
        .list_agents()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|agent| (agent.id, agent.name))
        .collect();

    Ok(ticket_thread::format_ticket_thread(&comments, &agent_names))
}

impl<'a> RunOrchestrator<'a> {
    pub fn new(pool: &'a PgPool, workflow: &'a WorkflowConfig) -> Self {
        Self {
            pool,
            workflow,
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: &'a EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub async fn finish_run(
        &self,
        run: &AgentRun,
        contract: &AgentRunResult,
        mut apply: ApplyResult,
        worktree_path: Option<String>,
        branch_name: Option<String>,
    ) -> Result<AgentRun, RunError> {
        let ticket_svc = TicketService::new(self.pool);
        let ticket = ticket_svc.get(run.ticket_id).await?;
        let current_status = ticket.ticket.status;
        let original_description = ticket.ticket.description.clone();
        let agent = AgentService::new(self.pool).get(run.agent_id).await?;
        let agents = AgentService::new(self.pool).list_agents().await?;
        let (project_agent_keys, project_agent_ids, project_implementer_keys) =
            build_project_agent_maps(&agents);

        let agent_key = agent
            .preset_source
            .clone()
            .unwrap_or_else(|| slugify(&agent.name));
        let technical_refinement_run = run.job_type == "work_on_ticket"
            && current_status == crate::domain::substatus::TicketStatus::Ready
            && (agent_key.eq_ignore_ascii_case("tech_lead") || is_tech_lead_role(&agent.role));

        let run_outcome = match apply.run_status {
            RunStatus::Succeeded => RunOutcome::Succeeded,
            RunStatus::Blocked => RunOutcome::Blocked,
            other => {
                return Err(RunError::Validation(format!(
                    "unexpected run status for orchestrator: {other:?}"
                )));
            }
        };

        let auto_assign_enabled = self
            .workflow
            .auto_assign
            .effective(status_to_str(ticket.ticket.status));

        let ctx = TransitionContext {
            ticket_id: run.ticket_id,
            current_status: ticket.ticket.status,
            assignee_agent_id: ticket.ticket.assignee_agent_id,
            agent_role: agent.role.clone(),
            agent_key,
            job_type: run.job_type.clone(),
            run_outcome,
            contract: contract.clone(),
            project_agent_keys,
            project_agent_ids: project_agent_ids.clone(),
            project_implementer_keys,
            auto_assign_enabled,
            clarification_round: ticket.ticket.clarification_round,
            context_profile: run.context_profile,
        };

        let skip_workflow = ctx.context_profile == ContextProfile::HumanAgent
            && ctx.run_outcome == RunOutcome::Succeeded;
        let consultation_run =
            run.job_type == "respond_to_mention" && run.context_profile == ContextProfile::Full;
        let mut action = if skip_workflow || consultation_run {
            TransitionAction::default()
        } else {
            WorkflowService::resolve_transition(ctx).map_err(RunError::Validation)?
        };

        let (substatus, substatus_metadata) = if consultation_run {
            (None, None)
        } else {
            merge_substatus(&action, &apply)
        };

        let mut ticket = ticket_svc
            .apply_workflow_update(
                run.ticket_id,
                action.new_status,
                substatus,
                substatus_metadata,
                action.new_assignee_id,
                action.pending_recommendation.clone(),
                i32::from(action.increment_clarification_round),
            )
            .await?;

        if !consultation_run {
            if let Some(description) = merge_ticket_description(
                &original_description,
                apply.ticket.updated_description.as_deref(),
                apply.ticket.acceptance_criteria.as_deref(),
            ) {
                ticket = ticket_svc
                    .update_fields(
                        run.ticket_id,
                        None,
                        Some(&description),
                        None,
                        None,
                        None,
                        None,
                    )
                    .await?;
            }
        }

        if !consultation_run {
            if let AgentRunResult::Done { split_tickets, .. } = contract {
                if !split_tickets.is_empty() {
                    let auto_split = self
                        .workflow
                        .auto_split
                        .effective(status_to_str(current_status));
                    SplitService::new(self.pool, self.workflow)
                        .apply_splits(&ticket.ticket, split_tickets, run.agent_id, auto_split)
                        .await?;
                }
            }
        }

        let collaboration_targets = select_collaboration_targets(contract, &agents, run.agent_id);
        if let AgentRunResult::Done { agent_requests, .. } = contract {
            replace_agent_requests_in_comment(
                &mut apply.comment.body,
                agent_requests,
                &collaboration_targets.agent_requests,
            );
        }

        let comment = CommentService::new(self.pool)
            .create(
                run.ticket_id,
                AuthorType::Agent,
                Some(run.agent_id),
                &apply.comment.body,
                apply.comment.intent,
                &[],
                &collaboration_targets.keys,
            )
            .await?;

        for notice in &action.system_comments {
            CommentService::new(self.pool)
                .create(
                    run.ticket_id,
                    AuthorType::System,
                    None,
                    notice,
                    CommentIntent::SystemEvent,
                    &[],
                    &[],
                )
                .await?;
        }

        let mentions = if collaboration_targets.agent_ids.is_empty() {
            Vec::new()
        } else {
            let resume_agent_id =
                if apply.run_status == RunStatus::Blocked && run.job_type == "work_on_ticket" {
                    Some(run.agent_id)
                } else {
                    None
                };
            MentionService::new(self.pool)
                .create_mentions_for_agents(
                    run.ticket_id,
                    comment.id,
                    &collaboration_targets.agent_ids,
                    resume_agent_id,
                )
                .await?
        };

        self.persist_and_publish_mentions(&mentions).await;

        if run.job_type == "respond_to_mention" && apply.run_status == RunStatus::Succeeded {
            if let Some(resume_job) = self.handle_clarification_resume(run, &ticket).await? {
                action.enqueue_jobs.push(resume_job);
            }
        }

        let mention_dispatch = enqueue_successful_consultation_jobs(
            &mut action,
            run,
            apply.run_status,
            &mentions,
            &collaboration_targets.consultation_agent_ids,
            ticket.ticket.assignee_agent_id,
            ticket.ticket.pending_assign_recommendation.as_ref(),
            &project_agent_ids,
            !technical_refinement_run,
        );
        let mention_svc = MentionService::new(self.pool);
        for mention_id in &mention_dispatch.handled_mention_ids {
            mention_svc.mark_handled(*mention_id).await?;
        }
        for mention_id in &mention_dispatch.ignored_mention_ids {
            mention_svc.mark_ignored(*mention_id).await?;
        }

        // Terminalize the source before exposing any follow-up runs. Otherwise a
        // fast responder can mention the source while it is still active and the
        // active-run guard will permanently suppress the chained response.
        let finished_run = RunService::new(self.pool)
            .finish_run(run.id, apply.run_status, worktree_path, branch_name)
            .await?;

        if self.workflow.auto_start_runs {
            let run_svc = RunService::new(self.pool);
            for job_req in action
                .enqueue_jobs
                .iter()
                .filter(|job_req| job_req.agent_id != run.agent_id)
            {
                let options = if job_req.job_type == "respond_to_mention" {
                    StartRunOptions {
                        trigger_comment_id: Some(comment.id),
                        ..StartRunOptions::default()
                    }
                } else {
                    StartRunOptions::default()
                };
                match run_svc
                    .start_run_for_agent(
                        run.ticket_id,
                        job_req.agent_id,
                        &job_req.job_type,
                        options,
                    )
                    .await
                {
                    Ok(_) => {}
                    Err(RunError::ActiveRunExists)
                        if mention_dispatch
                            .response_agent_ids
                            .contains(&job_req.agent_id) =>
                    {
                        tracing::info!(
                            source_run_id = %run.id,
                            target_agent_id = %job_req.agent_id,
                            "mentioned agent is already active; response will start after it finishes"
                        );
                    }
                    Err(error)
                        if mention_dispatch
                            .response_agent_ids
                            .contains(&job_req.agent_id) =>
                    {
                        tracing::warn!(
                            source_run_id = %run.id,
                            target_agent_id = %job_req.agent_id,
                            error = %error,
                            "could not auto-start mentioned agent; mention remains pending"
                        );
                    }
                    Err(error) => return Err(error),
                }
            }

            if let Some(Some(new_assignee)) = action.new_assignee_id {
                if new_assignee != run.agent_id && ticket.ticket.repo_id.is_some() {
                    let already_queued = action
                        .enqueue_jobs
                        .iter()
                        .any(|j| j.agent_id == new_assignee && j.job_type == "work_on_ticket");
                    if !already_queued {
                        run_svc
                            .start_run_for_agent(
                                run.ticket_id,
                                new_assignee,
                                "work_on_ticket",
                                StartRunOptions::default(),
                            )
                            .await?;
                    }
                }
            }
        }

        self.handle_terminal_run(&finished_run).await;

        Ok(finished_run)
    }

    async fn persist_and_publish_mentions(&self, mentions: &[TicketMention]) {
        let notification_svc = NotificationService::new(self.pool);
        let mut notifications_changed = false;

        for mention in mentions {
            if let Some(event_bus) = self.event_bus {
                event_bus.publish(AppEvent::AgentMentioned {
                    mention_id: mention.id,
                    ticket_id: mention.ticket_id,
                    comment_id: mention.comment_id,
                    mentioned_agent_id: mention.mentioned_agent_id,
                });
            }

            match notification_svc
                .create_for_agent_mentioned(
                    mention.id,
                    mention.ticket_id,
                    mention.comment_id,
                    mention.mentioned_agent_id,
                )
                .await
            {
                Ok(created) => notifications_changed |= !created.is_empty(),
                Err(error) => {
                    tracing::warn!(
                        mention_id = %mention.id,
                        error = %error,
                        "failed to create workflow mention notification"
                    );
                }
            }
        }

        if notifications_changed {
            if let Some(event_bus) = self.event_bus {
                event_bus.publish(AppEvent::NotificationChanged {
                    recipient_user_id: None,
                });
            }
        }
    }

    async fn handle_clarification_resume(
        &self,
        run: &AgentRun,
        ticket: &crate::services::ticket_service::TicketWithDisplay,
    ) -> Result<Option<JobRequest>, RunError> {
        let mention_svc = MentionService::new(self.pool);
        let Some(mention) = mention_svc
            .find_pending_for_agent_and_comment(run.ticket_id, run.agent_id, run.trigger_comment_id)
            .await?
        else {
            return Ok(None);
        };

        mention_svc.mark_handled(mention.id).await?;

        let ticket_svc = TicketService::new(self.pool);
        let Some(resume_agent_id) = mention.resume_agent_id else {
            if run.context_profile == ContextProfile::HumanChat {
                ticket_svc
                    .apply_workflow_update(
                        run.ticket_id,
                        None,
                        Some(None),
                        Some(None),
                        None,
                        None,
                        0,
                    )
                    .await?;
            }
            return Ok(None);
        };

        if ticket.ticket.clarification_round < MAX_CLARIFICATION_ROUNDS {
            ticket_svc
                .apply_workflow_update(
                    run.ticket_id,
                    None,
                    Some(None),
                    Some(None),
                    Some(Some(resume_agent_id)),
                    None,
                    1,
                )
                .await?;

            return Ok(Some(JobRequest {
                job_type: "work_on_ticket".into(),
                agent_id: resume_agent_id,
                resume_agent_id: None,
            }));
        } else {
            ticket_svc
                .apply_workflow_update(
                    run.ticket_id,
                    None,
                    Some(Some(Substatus::WaitingForHuman)),
                    Some(None),
                    None,
                    None,
                    0,
                )
                .await?;

            CommentService::new(self.pool)
                .create(
                    run.ticket_id,
                    AuthorType::System,
                    None,
                    "Maximum clarification rounds reached. Waiting for human input.",
                    CommentIntent::SystemEvent,
                    &[],
                    &[],
                )
                .await?;
        }

        Ok(None)
    }

    pub async fn handle_terminal_run(&self, run: &AgentRun) {
        if run.job_type == "respond_to_mention"
            && matches!(run.status, RunStatus::Failed | RunStatus::Cancelled)
            && run.trigger_comment_id.is_some()
        {
            let mention_svc = MentionService::new(self.pool);
            match mention_svc
                .find_pending_for_agent_and_comment(
                    run.ticket_id,
                    run.agent_id,
                    run.trigger_comment_id,
                )
                .await
            {
                Ok(Some(mention)) if mention.resume_agent_id.is_none() => {
                    if let Err(error) = mention_svc.mark_ignored(mention.id).await {
                        tracing::warn!(
                            terminal_run_id = %run.id,
                            mention_id = %mention.id,
                            error = %error,
                            "could not ignore mention after failed response run"
                        );
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(
                        terminal_run_id = %run.id,
                        error = %error,
                        "could not resolve mention after failed response run"
                    );
                }
            }
        }

        if !self.workflow.auto_start_runs {
            return;
        }

        let mention = match MentionService::new(self.pool)
            .find_next_unscheduled_agent_request(run.ticket_id, run.agent_id)
            .await
        {
            Ok(Some(mention)) => mention,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(
                    terminal_run_id = %run.id,
                    ticket_id = %run.ticket_id,
                    agent_id = %run.agent_id,
                    error = %error,
                    "could not inspect deferred agent mentions"
                );
                return;
            }
        };

        let ticket = match TicketService::new(self.pool).get(run.ticket_id).await {
            Ok(ticket) => ticket,
            Err(error) => {
                tracing::warn!(
                    terminal_run_id = %run.id,
                    ticket_id = %run.ticket_id,
                    error = %error,
                    "could not recheck deferred consultation ownership"
                );
                return;
            }
        };
        let agents = match AgentService::new(self.pool).list_agents().await {
            Ok(agents) => agents,
            Err(error) => {
                tracing::warn!(
                    terminal_run_id = %run.id,
                    ticket_id = %run.ticket_id,
                    error = %error,
                    "could not resolve deferred consultation ownership"
                );
                return;
            }
        };
        let agent_ids = resolve_agent_keys(&agents);
        if target_has_ownership(
            run.agent_id,
            ticket.ticket.assignee_agent_id,
            ticket.ticket.pending_assign_recommendation.as_ref(),
            &agent_ids,
        ) {
            if let Err(error) = MentionService::new(self.pool)
                .mark_handled(mention.id)
                .await
            {
                tracing::warn!(
                    terminal_run_id = %run.id,
                    mention_id = %mention.id,
                    error = %error,
                    "could not mark ownership-superseded consultation handled"
                );
            }
            return;
        }

        match RunService::new(self.pool)
            .start_run_for_agent(
                run.ticket_id,
                run.agent_id,
                "respond_to_mention",
                StartRunOptions {
                    trigger_comment_id: Some(mention.comment_id),
                    ..StartRunOptions::default()
                },
            )
            .await
        {
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    terminal_run_id = %run.id,
                    ticket_id = %run.ticket_id,
                    agent_id = %run.agent_id,
                    mention_id = %mention.id,
                    error = %error,
                    "could not start deferred agent mention; mention remains pending"
                );
            }
        }
    }
}

#[derive(Default)]
struct SuccessfulMentionDispatch {
    response_agent_ids: HashSet<Uuid>,
    handled_mention_ids: Vec<Uuid>,
    ignored_mention_ids: Vec<Uuid>,
}

#[derive(Default)]
struct CollaborationTargets {
    keys: Vec<String>,
    agent_ids: Vec<Uuid>,
    consultation_agent_ids: HashSet<Uuid>,
    agent_requests: Vec<ResolvedAgentRequest>,
}

fn select_collaboration_targets(
    contract: &AgentRunResult,
    agents: &[Agent],
    source_agent_id: Uuid,
) -> CollaborationTargets {
    let agent_map = resolve_agent_keys(agents);
    let mut targets = CollaborationTargets::default();
    let mut selected_agent_ids = HashSet::new();

    let mention_agents = match contract {
        AgentRunResult::Done { mention_agents, .. }
        | AgentRunResult::Blocked { mention_agents, .. } => mention_agents.as_slice(),
        AgentRunResult::Continued { .. } => &[],
    };

    for key in mention_agents {
        let _ = select_target(
            key,
            source_agent_id,
            &agent_map,
            &mut selected_agent_ids,
            &mut targets,
        );
    }

    if let AgentRunResult::Done { agent_requests, .. } = contract {
        for request in normalized_agent_requests(agent_requests) {
            let Some(target_id) = select_target(
                &request.agent_key,
                source_agent_id,
                &agent_map,
                &mut selected_agent_ids,
                &mut targets,
            ) else {
                continue;
            };
            if targets.consultation_agent_ids.insert(target_id) {
                targets.agent_requests.push(ResolvedAgentRequest {
                    agent_id: target_id,
                    request,
                });
            }
        }
    }

    targets
}

fn select_target(
    key: &str,
    source_agent_id: Uuid,
    agent_map: &HashMap<String, Uuid>,
    selected_agent_ids: &mut HashSet<Uuid>,
    targets: &mut CollaborationTargets,
) -> Option<Uuid> {
    let key = key.trim();
    let &target_id = agent_map.get(key)?;
    if target_id == source_agent_id {
        return None;
    }

    if selected_agent_ids.contains(&target_id) {
        return Some(target_id);
    }
    if selected_agent_ids.len() >= MAX_MENTIONS_PER_RUN as usize {
        return None;
    }

    selected_agent_ids.insert(target_id);
    targets.keys.push(key.to_string());
    targets.agent_ids.push(target_id);
    Some(target_id)
}

#[allow(clippy::too_many_arguments)]
fn enqueue_successful_consultation_jobs(
    action: &mut TransitionAction,
    run: &AgentRun,
    run_status: RunStatus,
    mentions: &[TicketMention],
    consultation_agent_ids: &HashSet<Uuid>,
    current_assignee_id: Option<Uuid>,
    pending_recommendation: Option<&Value>,
    project_agent_ids: &HashMap<String, Uuid>,
    allow_consultation_dispatch: bool,
) -> SuccessfulMentionDispatch {
    let mut dispatch = SuccessfulMentionDispatch::default();
    if run_status != RunStatus::Succeeded {
        return dispatch;
    }

    if run.context_profile != ContextProfile::Full {
        dispatch
            .ignored_mention_ids
            .extend(mentions.iter().map(|mention| mention.id));
        return dispatch;
    }

    let mut scheduled_agent_ids: HashSet<Uuid> =
        action.enqueue_jobs.iter().map(|job| job.agent_id).collect();
    if let Some(assignee_id) = current_assignee_id {
        scheduled_agent_ids.insert(assignee_id);
    }
    if let Some(Some(assignee_id)) = action.new_assignee_id {
        scheduled_agent_ids.insert(assignee_id);
    }
    if let Some(pending_id) =
        pending_recommendation_target(pending_recommendation, project_agent_ids)
    {
        scheduled_agent_ids.insert(pending_id);
    }

    // Attention does not create a response. When workflow already scheduled or
    // assigned that target, consider the attention handled by the ownership path.
    dispatch.handled_mention_ids.extend(
        mentions
            .iter()
            .filter(|mention| {
                !consultation_agent_ids.contains(&mention.mentioned_agent_id)
                    && scheduled_agent_ids.contains(&mention.mentioned_agent_id)
            })
            .map(|mention| mention.id),
    );

    let request_mentions = mentions
        .iter()
        .filter(|mention| consultation_agent_ids.contains(&mention.mentioned_agent_id))
        .collect::<Vec<_>>();
    if request_mentions.is_empty() {
        return dispatch;
    }

    // Ready-stage Tech Lead work is a formal ownership gate. Keep request
    // mentions durable, but do not let them create parallel consultation runs.
    if !allow_consultation_dispatch {
        dispatch
            .handled_mention_ids
            .extend(request_mentions.iter().map(|mention| mention.id));
        return dispatch;
    }

    // Automatic collaboration is one hop. A response may still persist new
    // mentions and notifications, but its own structured requests are terminal.
    if run.job_type == "respond_to_mention" {
        dispatch
            .handled_mention_ids
            .extend(request_mentions.iter().map(|mention| mention.id));
        return dispatch;
    }
    if run.job_type != "work_on_ticket" {
        dispatch
            .ignored_mention_ids
            .extend(request_mentions.iter().map(|mention| mention.id));
        return dispatch;
    }

    for mention in request_mentions {
        let target_id = mention.mentioned_agent_id;
        if scheduled_agent_ids.insert(target_id) {
            action.enqueue_jobs.push(JobRequest {
                job_type: "respond_to_mention".into(),
                agent_id: target_id,
                resume_agent_id: None,
            });
            dispatch.response_agent_ids.insert(target_id);
        } else {
            dispatch.handled_mention_ids.push(mention.id);
        }
    }

    dispatch
}

fn target_has_ownership(
    target_id: Uuid,
    current_assignee_id: Option<Uuid>,
    pending_recommendation: Option<&Value>,
    project_agent_ids: &HashMap<String, Uuid>,
) -> bool {
    current_assignee_id == Some(target_id)
        || pending_recommendation_target(pending_recommendation, project_agent_ids)
            == Some(target_id)
}

fn pending_recommendation_target(
    pending_recommendation: Option<&Value>,
    project_agent_ids: &HashMap<String, Uuid>,
) -> Option<Uuid> {
    let pending_key = pending_recommendation?
        .get("recommendedAgentKey")?
        .as_str()?;
    project_agent_ids.get(pending_key).copied()
}

fn build_project_agent_maps(agents: &[Agent]) -> (Vec<String>, HashMap<String, Uuid>, Vec<String>) {
    let mut keys = Vec::new();

    for agent in agents {
        if !agent.enabled {
            continue;
        }
        if let Some(ref preset) = agent.preset_source {
            if !keys.iter().any(|k| k == preset) {
                keys.push(preset.clone());
            }
        }
        let slug = slugify(&agent.name);
        if !keys.iter().any(|k| k == &slug) {
            keys.push(slug.clone());
        }
    }

    let agent_ids = resolve_agent_keys(agents);
    let implementer_ids = agents
        .iter()
        .filter(|agent| {
            agent.enabled
                && is_implementer(&agent.role)
                && !agent
                    .preset_source
                    .as_deref()
                    .is_some_and(|preset| preset.eq_ignore_ascii_case("tech_lead"))
                && !is_tech_lead_role(&agent.role)
        })
        .map(|agent| agent.id)
        .collect::<HashSet<_>>();
    let implementer_keys = keys
        .iter()
        .filter(|key| {
            agent_ids
                .get(*key)
                .is_some_and(|id| implementer_ids.contains(id))
        })
        .cloned()
        .collect();

    (keys, agent_ids, implementer_keys)
}

fn is_tech_lead_role(role: &str) -> bool {
    let role = role.to_ascii_lowercase();
    role.contains("tech lead") || role.contains("technical lead")
}

fn merge_substatus(
    action: &TransitionAction,
    apply: &ApplyResult,
) -> (Option<Option<Substatus>>, Option<Option<Value>>) {
    let substatus = match &action.substatus {
        Some(value) => Some(*value),
        None => apply.ticket.substatus.map(Some),
    };
    let substatus_metadata = match &action.substatus_metadata {
        Some(value) => Some(value.clone()),
        None => apply.ticket.substatus_metadata.clone().map(Some),
    };
    (substatus, substatus_metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::comment::CommentIntent;
    use crate::domain::context_profile::ContextProfile;
    use crate::domain::run::run_status_to_str;
    use crate::domain::substatus::TicketStatus;
    use crate::sandbox::permissive::PROFILE_ID;
    use crate::services::job_service::JobService;
    use crate::services::result_contract::ApplyResult;
    use crate::services::result_contract::{ApplyComment, ApplyTicketUpdate};
    use coppice_config::WorkflowConfig;

    async fn test_pool() -> Option<PgPool> {
        let pool = crate::db::shared_test_pool().await.ok()?;
        crate::db::truncate_test_workspace(&pool).await.ok()?;
        Some(pool)
    }

    struct TestFixture {
        project_id: Uuid,
        ticket_id: Uuid,
        run_id: Uuid,
        pm_agent_id: Uuid,
        engineer_agent_id: Uuid,
    }

    async fn insert_fixture(pool: &PgPool) -> TestFixture {
        let project_id = Uuid::new_v4();
        sqlx::query("INSERT INTO projects (id, name, slug) VALUES ($1, $2, $3)")
            .bind(project_id)
            .bind("orchestrator project")
            .bind(format!("orch-{}", project_id))
            .execute(pool)
            .await
            .expect("insert project");

        let pm_agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt, connector, preset_source
            )
            VALUES ($1, $2, $3, '{}', '{}', $4, $5, $6)
            "#,
        )
        .bind(pm_agent_id)
        .bind("PM Agent")
        .bind("PM")
        .bind("prompt")
        .bind("mock")
        .bind("pm")
        .execute(pool)
        .await
        .expect("insert pm agent");

        let engineer_agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt, connector, preset_source
            )
            VALUES ($1, $2, $3, '{}', '{}', $4, $5, $6)
            "#,
        )
        .bind(engineer_agent_id)
        .bind("Backend Engineer")
        .bind("Backend Engineer")
        .bind("prompt")
        .bind("mock")
        .bind("backend_engineer")
        .execute(pool)
        .await
        .expect("insert engineer agent");

        let ticket_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO tickets (
                id, project_id, title, status, created_by, assignee_agent_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(ticket_id)
        .bind(project_id)
        .bind("orchestrator ticket")
        .bind("backlog")
        .bind("test")
        .bind(pm_agent_id)
        .execute(pool)
        .await
        .expect("insert ticket");

        let run_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(run_id)
        .bind(ticket_id)
        .bind(pm_agent_id)
        .bind("work_on_ticket")
        .bind(run_status_to_str(RunStatus::Running))
        .bind(PROFILE_ID)
        .execute(pool)
        .await
        .expect("insert run");

        TestFixture {
            project_id,
            ticket_id,
            run_id,
            pm_agent_id,
            engineer_agent_id,
        }
    }

    fn pm_done_with_assign_to(engineer_key: &str) -> AgentRunResult {
        AgentRunResult::Done {
            summary: "Enriched ticket".into(),
            changed_files: vec![],
            tests_run: vec![],
            next_status: Some("Ready".into()),
            assign_to: Some(engineer_key.into()),
            updated_description: None,
            acceptance_criteria: None,
            mention_agents: vec![],
            agent_requests: vec![],
            blockers: vec![],
            split_tickets: vec![],
        }
    }

    fn blocked_with_mentions(keys: &[&str]) -> AgentRunResult {
        AgentRunResult::Blocked {
            blocker_type: "error".into(),
            summary: "Need clarification".into(),
            next_status: None,
            assign_to: None,
            updated_description: None,
            acceptance_criteria: None,
            mention_agents: keys.iter().map(|k| (*k).into()).collect(),
            required_capabilities: vec![],
            required_secrets: vec![],
        }
    }

    fn done_with_mentions(keys: &[&str]) -> AgentRunResult {
        AgentRunResult::Done {
            summary: "Mentioning another agent".into(),
            changed_files: vec![],
            tests_run: vec![],
            next_status: None,
            assign_to: None,
            updated_description: None,
            acceptance_criteria: None,
            mention_agents: keys.iter().map(|key| (*key).into()).collect(),
            agent_requests: vec![],
            blockers: vec![],
            split_tickets: vec![],
        }
    }

    fn done_with_requests(keys: &[&str]) -> AgentRunResult {
        AgentRunResult::Done {
            summary: "Requesting a focused consultation".into(),
            changed_files: vec![],
            tests_run: vec![],
            next_status: None,
            assign_to: None,
            updated_description: None,
            acceptance_criteria: None,
            mention_agents: vec![],
            agent_requests: keys
                .iter()
                .map(|key| crate::providers::AgentRequest {
                    agent_key: (*key).into(),
                    intent: "consult".into(),
                    request: format!("Review the focused question for {key}."),
                })
                .collect(),
            blockers: vec![],
            split_tickets: vec![],
        }
    }

    fn succeeded_request_apply(keys: &[&str]) -> ApplyResult {
        crate::services::result_contract::apply_agent_result(&done_with_requests(keys))
            .expect("apply consultation request fixture")
    }

    fn succeeded_apply(body: &str, mentions: &[&str]) -> ApplyResult {
        ApplyResult {
            run_status: RunStatus::Succeeded,
            ticket: ApplyTicketUpdate {
                status: None,
                substatus: None,
                substatus_metadata: None,
                updated_description: None,
                acceptance_criteria: None,
            },
            comment: ApplyComment {
                body: body.into(),
                intent: CommentIntent::ImplementationDone,
                mentions: mentions.iter().map(|key| (*key).into()).collect(),
            },
        }
    }

    fn test_run(agent_id: Uuid, job_type: &str) -> AgentRun {
        AgentRun {
            id: Uuid::new_v4(),
            ticket_id: Uuid::new_v4(),
            agent_id,
            job_type: job_type.into(),
            status: RunStatus::Running,
            sandbox_profile_id: PROFILE_ID.to_string(),
            worktree_path: None,
            branch_name: None,
            error_message: None,
            session_id: None,
            context_profile: ContextProfile::Full,
            trigger_comment_id: None,
            started_at: None,
            ended_at: None,
            created_at: time::OffsetDateTime::now_utc(),
        }
    }

    fn pending_mention(agent_id: Uuid) -> crate::domain::mention::TicketMention {
        crate::domain::mention::TicketMention {
            id: Uuid::new_v4(),
            ticket_id: Uuid::new_v4(),
            comment_id: Uuid::new_v4(),
            mentioned_agent_id: agent_id,
            resume_agent_id: None,
            status: crate::domain::mention::MentionStatus::Pending,
        }
    }

    fn collaboration_agent(id: Uuid, name: &str, key: &str, enabled: bool) -> Agent {
        Agent {
            id,
            name: name.into(),
            role: "Reviewer".into(),
            skills: vec![],
            responsibilities: vec![],
            system_prompt: "Review only".into(),
            connector: "mock".into(),
            model_provider: None,
            model: None,
            enabled,
            preset_source: Some(key.into()),
            created_at: time::OffsetDateTime::now_utc(),
            updated_at: time::OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn collaboration_targets_share_limit_and_ignore_invalid_duplicate_and_self_entries() {
        let source_id = Uuid::from_u128(1);
        let first_id = Uuid::from_u128(2);
        let second_id = Uuid::from_u128(3);
        let over_limit_id = Uuid::from_u128(4);
        let disabled_id = Uuid::from_u128(5);
        let agents = vec![
            collaboration_agent(source_id, "Source Agent", "source", true),
            collaboration_agent(first_id, "First Agent", "first", true),
            collaboration_agent(second_id, "Second Agent", "second", true),
            collaboration_agent(over_limit_id, "Third Agent", "third", true),
            collaboration_agent(disabled_id, "Disabled Agent", "disabled", false),
        ];
        let contract = AgentRunResult::Done {
            summary: "Collaborate".into(),
            changed_files: vec![],
            tests_run: vec![],
            next_status: None,
            assign_to: None,
            updated_description: None,
            acceptance_criteria: None,
            mention_agents: vec![
                "unknown".into(),
                "source".into(),
                "first".into(),
                "disabled".into(),
            ],
            agent_requests: vec![
                crate::providers::AgentRequest {
                    agent_key: "first-agent".into(),
                    intent: "consult".into(),
                    request: "Duplicate alias".into(),
                },
                crate::providers::AgentRequest {
                    agent_key: "second".into(),
                    intent: "consult".into(),
                    request: "Second target".into(),
                },
                crate::providers::AgentRequest {
                    agent_key: "third".into(),
                    intent: "consult".into(),
                    request: "Over the shared target limit".into(),
                },
            ],
            blockers: vec![],
            split_tickets: vec![],
        };

        let targets = select_collaboration_targets(&contract, &agents, source_id);
        assert_eq!(targets.keys, vec!["first", "second"]);
        assert_eq!(
            targets.consultation_agent_ids,
            HashSet::from([first_id, second_id])
        );
        assert_eq!(
            targets
                .agent_requests
                .iter()
                .map(|request| request.request.agent_key.as_str())
                .collect::<Vec<_>>(),
            vec!["first-agent", "second"]
        );
    }

    fn dispatch_consultations(
        action: &mut TransitionAction,
        run: &AgentRun,
        status: RunStatus,
        mentions: &[TicketMention],
        consultation_agent_ids: HashSet<Uuid>,
    ) -> SuccessfulMentionDispatch {
        enqueue_successful_consultation_jobs(
            action,
            run,
            status,
            mentions,
            &consultation_agent_ids,
            None,
            None,
            &HashMap::new(),
            true,
        )
    }

    #[test]
    fn successful_attention_mentions_do_not_enqueue_response_runs() {
        let source_id = Uuid::from_u128(1);
        let target_id = Uuid::from_u128(2);
        let run = test_run(source_id, "work_on_ticket");
        let mentions = vec![pending_mention(target_id)];
        let mut action = TransitionAction::default();

        let dispatch = dispatch_consultations(
            &mut action,
            &run,
            RunStatus::Succeeded,
            &mentions,
            HashSet::new(),
        );

        assert!(action.enqueue_jobs.is_empty());
        assert!(dispatch.response_agent_ids.is_empty());
        assert!(dispatch.ignored_mention_ids.is_empty());
        assert!(dispatch.handled_mention_ids.is_empty());
    }

    #[test]
    fn successful_work_request_enqueues_one_response() {
        let source_id = Uuid::from_u128(1);
        let target_id = Uuid::from_u128(2);
        let run = test_run(source_id, "work_on_ticket");
        let mut action = TransitionAction::default();
        let mention = pending_mention(target_id);

        let dispatch = dispatch_consultations(
            &mut action,
            &run,
            RunStatus::Succeeded,
            &[mention],
            HashSet::from([target_id]),
        );

        assert_eq!(action.enqueue_jobs.len(), 1);
        assert_eq!(action.enqueue_jobs[0].agent_id, target_id);
        assert_eq!(action.enqueue_jobs[0].job_type, "respond_to_mention");
        assert_eq!(dispatch.response_agent_ids, HashSet::from([target_id]));
    }

    #[test]
    fn successful_response_request_is_handled_without_chaining() {
        let source_id = Uuid::from_u128(1);
        let target_id = Uuid::from_u128(2);
        let run = test_run(source_id, "respond_to_mention");
        let mut action = TransitionAction::default();
        let mention = pending_mention(target_id);
        let mention_id = mention.id;

        let dispatch = dispatch_consultations(
            &mut action,
            &run,
            RunStatus::Succeeded,
            &[mention],
            HashSet::from([target_id]),
        );

        assert!(action.enqueue_jobs.is_empty());
        assert!(dispatch.response_agent_ids.is_empty());
        assert_eq!(dispatch.handled_mention_ids, vec![mention_id]);
    }

    #[test]
    fn successful_requests_do_not_duplicate_handoff_or_assignee_runs() {
        let source_id = Uuid::from_u128(1);
        let handoff_target = Uuid::from_u128(2);
        let assignee_target = Uuid::from_u128(3);
        let run = test_run(source_id, "work_on_ticket");
        let mut action = TransitionAction {
            new_assignee_id: Some(Some(assignee_target)),
            enqueue_jobs: vec![crate::domain::workflow::JobRequest {
                job_type: "work_on_ticket".into(),
                agent_id: handoff_target,
                resume_agent_id: None,
            }],
            ..TransitionAction::default()
        };

        let dispatch = dispatch_consultations(
            &mut action,
            &run,
            RunStatus::Succeeded,
            &[
                pending_mention(handoff_target),
                pending_mention(assignee_target),
            ],
            HashSet::from([handoff_target, assignee_target]),
        );

        assert_eq!(action.enqueue_jobs.len(), 1);
        assert_eq!(action.enqueue_jobs[0].job_type, "work_on_ticket");
        assert_eq!(action.enqueue_jobs[0].agent_id, handoff_target);
        assert!(dispatch.response_agent_ids.is_empty());
        assert_eq!(dispatch.handled_mention_ids.len(), 2);
        assert!(dispatch.ignored_mention_ids.is_empty());
    }

    #[test]
    fn pending_assignment_recommendation_wins_over_consultation() {
        let source_id = Uuid::from_u128(1);
        let target_id = Uuid::from_u128(2);
        let run = test_run(source_id, "work_on_ticket");
        let mut action = TransitionAction::default();
        let mention = pending_mention(target_id);
        let mention_id = mention.id;
        let pending = serde_json::json!({ "recommendedAgentKey": "tech_lead" });

        let dispatch = enqueue_successful_consultation_jobs(
            &mut action,
            &run,
            RunStatus::Succeeded,
            &[mention],
            &HashSet::from([target_id]),
            None,
            Some(&pending),
            &HashMap::from([("tech_lead".into(), target_id)]),
            true,
        );

        assert!(action.enqueue_jobs.is_empty());
        assert!(dispatch.response_agent_ids.is_empty());
        assert_eq!(dispatch.handled_mention_ids, vec![mention_id]);
    }

    #[test]
    fn mentions_are_not_scheduled_for_blocked_or_unrelated_runs() {
        let source_id = Uuid::from_u128(1);
        let target_id = Uuid::from_u128(2);
        let mentions = [pending_mention(target_id)];

        for (job_type, status, profile) in [
            ("work_on_ticket", RunStatus::Blocked, ContextProfile::Full),
            (
                "prepare_context",
                RunStatus::Succeeded,
                ContextProfile::Full,
            ),
            (
                "respond_to_mention",
                RunStatus::Succeeded,
                ContextProfile::HumanChat,
            ),
        ] {
            let mut run = test_run(source_id, job_type);
            run.context_profile = profile;
            let mut action = TransitionAction::default();
            dispatch_consultations(
                &mut action,
                &run,
                status,
                &mentions,
                HashSet::from([target_id]),
            );
            assert!(action.enqueue_jobs.is_empty());
        }
    }

    #[test]
    fn technical_refinement_requests_are_handled_without_response_runs() {
        let source_id = Uuid::from_u128(1);
        let target_id = Uuid::from_u128(2);
        let run = test_run(source_id, "work_on_ticket");
        let mention = pending_mention(target_id);
        let mention_id = mention.id;
        let mut action = TransitionAction::default();

        let dispatch = enqueue_successful_consultation_jobs(
            &mut action,
            &run,
            RunStatus::Succeeded,
            &[mention],
            &HashSet::from([target_id]),
            None,
            None,
            &HashMap::new(),
            false,
        );

        assert!(action.enqueue_jobs.is_empty());
        assert!(dispatch.response_agent_ids.is_empty());
        assert_eq!(dispatch.handled_mention_ids, vec![mention_id]);
    }

    #[test]
    fn project_agent_maps_only_expose_enabled_implementer_aliases() {
        let tech_lead_id = Uuid::from_u128(1);
        let engineer_id = Uuid::from_u128(2);
        let disabled_engineer_id = Uuid::from_u128(3);
        let dual_lead_id = Uuid::from_u128(4);
        let mut tech_lead = collaboration_agent(tech_lead_id, "Tech Lead", "tech_lead", true);
        tech_lead.role = "Lead Engineer".into();
        let mut engineer =
            collaboration_agent(engineer_id, "Backend Engineer", "backend_engineer", true);
        engineer.role = "Backend Engineer".into();
        let mut disabled = collaboration_agent(
            disabled_engineer_id,
            "Disabled Engineer",
            "disabled_engineer",
            false,
        );
        disabled.role = "Backend Engineer".into();
        let mut dual_lead =
            collaboration_agent(dual_lead_id, "Architecture Lead", "architecture_lead", true);
        dual_lead.role = "Technical Lead Engineer".into();

        let (keys, agent_ids, implementer_keys) =
            build_project_agent_maps(&[tech_lead, engineer, disabled, dual_lead]);

        assert!(keys.contains(&"tech_lead".to_string()));
        assert_eq!(agent_ids.get("backend_engineer"), Some(&engineer_id));
        assert!(implementer_keys.contains(&"backend_engineer".to_string()));
        assert!(implementer_keys.contains(&"backend-engineer".to_string()));
        assert!(!implementer_keys.contains(&"tech_lead".to_string()));
        assert!(!implementer_keys.contains(&"architecture_lead".to_string()));
        assert!(!keys.contains(&"disabled_engineer".to_string()));
    }

    #[test]
    fn human_agent_done_does_not_change_status() {
        let ctx = TransitionContext {
            ticket_id: Uuid::from_u128(1),
            current_status: TicketStatus::Backlog,
            assignee_agent_id: Some(Uuid::from_u128(0x100)),
            agent_role: "PM".into(),
            agent_key: "pm".into(),
            job_type: "work_on_ticket".into(),
            run_outcome: RunOutcome::Succeeded,
            contract: pm_done_with_assign_to("backend_engineer"),
            project_agent_keys: vec!["pm".into(), "backend_engineer".into()],
            project_agent_ids: HashMap::from([
                ("pm".into(), Uuid::from_u128(0x100)),
                ("backend_engineer".into(), Uuid::from_u128(0x200)),
            ]),
            project_implementer_keys: vec!["backend_engineer".into()],
            auto_assign_enabled: true,
            clarification_round: 0,
            context_profile: ContextProfile::HumanAgent,
        };

        let skip_workflow = ctx.context_profile == ContextProfile::HumanAgent
            && ctx.run_outcome == RunOutcome::Succeeded;
        assert!(skip_workflow);

        let full_action = WorkflowService::resolve_transition(TransitionContext {
            context_profile: ContextProfile::Full,
            ..ctx.clone()
        })
        .expect("full profile resolves");
        assert_eq!(full_action.new_status, Some(TicketStatus::Ready));

        let action = if skip_workflow {
            TransitionAction::default()
        } else {
            WorkflowService::resolve_transition(ctx).expect("resolve")
        };
        assert!(action.new_status.is_none());
        assert!(action.new_assignee_id.is_none());
        assert!(action.pending_recommendation.is_none());
    }

    #[tokio::test]
    async fn orchestrator_applies_workflow_status_not_contract_next_status() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let workflow = WorkflowConfig::default();
        let orchestrator = RunOrchestrator::new(&pool, &workflow);

        let contract = pm_done_with_assign_to("backend_engineer");
        let apply = ApplyResult {
            run_status: RunStatus::Succeeded,
            ticket: ApplyTicketUpdate {
                status: None,
                substatus: None,
                substatus_metadata: None,
                updated_description: None,
                acceptance_criteria: None,
            },
            comment: ApplyComment {
                body: "PM done".into(),
                intent: CommentIntent::ImplementationDone,
                mentions: vec![],
            },
        };

        orchestrator
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &contract,
                apply,
                None,
                None,
            )
            .await
            .expect("finish run");

        let ticket = TicketService::new(&pool)
            .get(fx.ticket_id)
            .await
            .expect("load ticket");
        assert_eq!(ticket.ticket.status, TicketStatus::Ready);
        assert!(ticket.ticket.pending_assign_recommendation.is_some());
    }

    #[tokio::test]
    async fn orchestrator_unknown_assign_to_posts_system_comment_when_blocked() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let workflow = WorkflowConfig {
            auto_start_runs: false,
            auto_assign: coppice_config::AutoAssignConfig {
                default: true,
                ..Default::default()
            },
            ..WorkflowConfig::default()
        };
        let orchestrator = RunOrchestrator::new(&pool, &workflow);

        let contract = pm_done_with_assign_to("frontend_engineer");
        let apply = ApplyResult {
            run_status: RunStatus::Succeeded,
            ticket: ApplyTicketUpdate {
                status: None,
                substatus: None,
                substatus_metadata: None,
                updated_description: None,
                acceptance_criteria: None,
            },
            comment: ApplyComment {
                body: "PM done with bad assignee".into(),
                intent: CommentIntent::ImplementationDone,
                mentions: vec![],
            },
        };

        orchestrator
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &contract,
                apply,
                None,
                None,
            )
            .await
            .expect("finish run");

        let ticket = TicketService::new(&pool)
            .get(fx.ticket_id)
            .await
            .expect("load ticket");
        assert_eq!(ticket.ticket.status, TicketStatus::Blocked);

        let comments = CommentService::new(&pool)
            .list_by_ticket(fx.ticket_id)
            .await
            .expect("list comments");
        let system = comments
            .iter()
            .find(|c| c.author_type == AuthorType::System)
            .expect("system comment for unknown assignee");
        assert!(system.body.contains("frontend_engineer"));
        assert!(system.body.contains("Workflow blocked"));
    }

    #[tokio::test]
    async fn orchestrator_split_pending_sets_json_no_children() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let workflow = WorkflowConfig::default();
        let orchestrator = RunOrchestrator::new(&pool, &workflow);

        let contract = AgentRunResult::Done {
            summary: "Split epic into child tickets".into(),
            changed_files: vec![],
            tests_run: vec![],
            next_status: None,
            assign_to: None,
            updated_description: Some("Short epic summary for parent.".into()),
            acceptance_criteria: None,
            mention_agents: vec![],
            agent_requests: vec![],
            blockers: vec![],
            split_tickets: vec![
                crate::domain::workflow::SplitTicketSpec {
                    title: "Child A".into(),
                    description: "First deliverable".into(),
                    acceptance_criteria: Some("- A is done".into()),
                    assign_to: Some("backend_engineer".into()),
                },
                crate::domain::workflow::SplitTicketSpec {
                    title: "Child B".into(),
                    description: "Second deliverable".into(),
                    acceptance_criteria: None,
                    assign_to: None,
                },
            ],
        };
        let apply = ApplyResult {
            run_status: RunStatus::Succeeded,
            ticket: ApplyTicketUpdate {
                status: None,
                substatus: None,
                substatus_metadata: None,
                updated_description: Some("Short epic summary for parent.".into()),
                acceptance_criteria: None,
            },
            comment: ApplyComment {
                body: "PM split proposal".into(),
                intent: CommentIntent::ImplementationDone,
                mentions: vec![],
            },
        };

        orchestrator
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &contract,
                apply,
                None,
                None,
            )
            .await
            .expect("finish run");

        let ticket = TicketService::new(&pool)
            .get(fx.ticket_id)
            .await
            .expect("load ticket");
        assert!(ticket.ticket.pending_split_recommendation.is_some());

        let child_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM tickets WHERE parent_ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("count children");
        assert_eq!(child_count, 0);
    }

    async fn attach_ready_repo(pool: &PgPool, ticket_id: Uuid) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&path)
            .output()
            .expect("git init");
        std::fs::write(path.join("README.md"), "# test\n").expect("write readme");
        std::process::Command::new("git")
            .args(["add", "README.md"])
            .current_dir(&path)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@localhost")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@localhost")
            .current_dir(&path)
            .output()
            .expect("git commit");

        let repo_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO repos (
                id, name, local_path, default_branch, verification_status
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(repo_id)
        .bind("test-repo")
        .bind(path.to_string_lossy().as_ref())
        .bind("main")
        .bind("ready")
        .execute(pool)
        .await
        .expect("insert repo");

        sqlx::query("UPDATE tickets SET repo_id = $2 WHERE id = $1")
            .bind(ticket_id)
            .bind(repo_id)
            .execute(pool)
            .await
            .expect("attach repo");

        dir
    }

    #[tokio::test]
    async fn successful_attention_mention_persists_without_response_run() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };

        RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_mentions(&["backend_engineer"]),
                succeeded_apply("FYI @backend_engineer", &["backend_engineer"]),
                None,
                None,
            )
            .await
            .expect("finish attention-only mention run");

        let mention_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM ticket_mentions WHERE ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("count attention mentions");
        assert_eq!(mention_count, 1);

        let response_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_runs WHERE ticket_id = $1 AND job_type = 'respond_to_mention'",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("count attention response runs");
        assert_eq!(response_count, 0);
    }

    #[tokio::test]
    async fn successful_work_request_persists_and_auto_starts_response() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;
        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };

        RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_requests(&["backend_engineer"]),
                succeeded_request_apply(&["backend_engineer"]),
                None,
                None,
            )
            .await
            .expect("finish successful consultation run");

        let mention_rows = sqlx::query_as::<_, (Uuid, Uuid, Option<Uuid>)>(
            r#"
            SELECT comment_id, mentioned_agent_id, resume_agent_id
            FROM ticket_mentions
            WHERE ticket_id = $1
            "#,
        )
        .bind(fx.ticket_id)
        .fetch_all(&pool)
        .await
        .expect("load persisted mentions");
        assert_eq!(mention_rows.len(), 1);
        assert_eq!(mention_rows[0].1, fx.engineer_agent_id);
        assert_eq!(mention_rows[0].2, None);

        let response_runs = sqlx::query_as::<_, (String, String, Option<Uuid>)>(
            r#"
            SELECT job_type, status, trigger_comment_id
            FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2 AND job_type = 'respond_to_mention'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_all(&pool)
        .await
        .expect("load response runs");
        assert_eq!(response_runs.len(), 1);
        assert_eq!(response_runs[0].0, "respond_to_mention");
        assert_eq!(response_runs[0].1, "queued");
        assert_eq!(response_runs[0].2, Some(mention_rows[0].0));
    }

    #[tokio::test]
    async fn successful_work_request_keeps_resolved_target_when_key_is_reassigned() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;
        let replacement_agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt, connector, preset_source
            )
            VALUES ($1, 'Replacement Engineer', 'Engineer', '{}', '{}', 'prompt', 'mock', 'research')
            "#,
        )
        .bind(replacement_agent_id)
        .execute(&pool)
        .await
        .expect("insert replacement agent");

        sqlx::query(
            "DROP TRIGGER IF EXISTS test_reassign_agent_key_after_comment ON ticket_comments",
        )
        .execute(&pool)
        .await
        .expect("drop stale key reassignment trigger");
        sqlx::query("DROP FUNCTION IF EXISTS test_reassign_agent_key_after_comment()")
            .execute(&pool)
            .await
            .expect("drop stale key reassignment function");
        sqlx::query(&format!(
            r#"
            CREATE FUNCTION test_reassign_agent_key_after_comment()
            RETURNS trigger
            LANGUAGE plpgsql
            AS $$
            BEGIN
                UPDATE agents SET preset_source = 'renamed_engineer'
                WHERE id = '{original_target}'::uuid;
                UPDATE agents SET preset_source = 'backend_engineer'
                WHERE id = '{replacement_target}'::uuid;
                RETURN NEW;
            END;
            $$
            "#,
            original_target = fx.engineer_agent_id,
            replacement_target = replacement_agent_id,
        ))
        .execute(&pool)
        .await
        .expect("create key reassignment function");
        sqlx::query(&format!(
            r#"
            CREATE TRIGGER test_reassign_agent_key_after_comment
            AFTER INSERT ON ticket_comments
            FOR EACH ROW
            WHEN (
                NEW.ticket_id = '{ticket_id}'::uuid
                AND NEW.author_id = '{source_agent_id}'::uuid
                AND NEW.author_type = 'agent'
            )
            EXECUTE FUNCTION test_reassign_agent_key_after_comment()
            "#,
            ticket_id = fx.ticket_id,
            source_agent_id = fx.pm_agent_id,
        ))
        .execute(&pool)
        .await
        .expect("create key reassignment trigger");

        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };
        RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_requests(&["backend_engineer"]),
                succeeded_request_apply(&["backend_engineer"]),
                None,
                None,
            )
            .await
            .expect("finish consultation while target key changes");

        sqlx::query("DROP TRIGGER test_reassign_agent_key_after_comment ON ticket_comments")
            .execute(&pool)
            .await
            .expect("drop key reassignment trigger");
        sqlx::query("DROP FUNCTION test_reassign_agent_key_after_comment()")
            .execute(&pool)
            .await
            .expect("drop key reassignment function");

        let mentioned_agent_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT mentioned_agent_id FROM ticket_mentions WHERE ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("load consultation mention target");
        assert_eq!(mentioned_agent_id, fx.engineer_agent_id);

        let response_agent_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT agent_id FROM agent_runs
            WHERE ticket_id = $1 AND job_type = 'respond_to_mention'
            "#,
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("load consultation response target");
        assert_eq!(response_agent_id, fx.engineer_agent_id);
    }

    #[tokio::test]
    async fn successful_work_request_persists_without_run_when_auto_start_is_disabled() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let workflow = WorkflowConfig {
            auto_start_runs: false,
            ..WorkflowConfig::default()
        };

        RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_requests(&["backend_engineer"]),
                succeeded_request_apply(&["backend_engineer"]),
                None,
                None,
            )
            .await
            .expect("finish successful consultation run");

        let mention_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM ticket_mentions WHERE ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("count mentions");
        assert_eq!(mention_count, 1);

        let response_run_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2 AND job_type = 'respond_to_mention'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count response runs");
        assert_eq!(response_run_count, 0);
    }

    #[tokio::test]
    async fn successful_request_waits_for_active_target_then_starts_response() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;
        let active_target_run_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id
            )
            VALUES ($1, $2, $3, 'work_on_ticket', 'queued', $4)
            "#,
        )
        .bind(active_target_run_id)
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .bind(PROFILE_ID)
        .execute(&pool)
        .await
        .expect("insert existing active target run");

        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };
        let finished = RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_requests(&["backend_engineer"]),
                succeeded_request_apply(&["backend_engineer"]),
                None,
                None,
            )
            .await
            .expect("finish source when mention target is already active");
        assert_eq!(finished.status, RunStatus::Succeeded);

        let mention_comment_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT comment_id FROM ticket_mentions WHERE ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("load persisted mention");

        sqlx::query("UPDATE agent_runs SET status = 'running' WHERE id = $1")
            .bind(active_target_run_id)
            .execute(&pool)
            .await
            .expect("mark unrelated target run running");
        RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: active_target_run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.engineer_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_mentions(&[]),
                succeeded_apply("Unrelated work finished", &[]),
                None,
                None,
            )
            .await
            .expect("finish unrelated target run");

        let target_runs = sqlx::query_as::<_, (String, String, Option<Uuid>)>(
            r#"
            SELECT job_type, status, trigger_comment_id FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2
            ORDER BY created_at ASC
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_all(&pool)
        .await
        .expect("load target runs");
        assert_eq!(
            target_runs,
            vec![
                ("work_on_ticket".into(), "succeeded".into(), None),
                (
                    "respond_to_mention".into(),
                    "queued".into(),
                    Some(mention_comment_id),
                ),
            ]
        );

        let mention_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM ticket_mentions WHERE ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("count persisted mentions");
        assert_eq!(mention_count, 1);
    }

    #[tokio::test]
    async fn deferred_request_is_suppressed_when_target_becomes_pending_owner() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;
        let active_target_run_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id
            )
            VALUES ($1, $2, $3, 'work_on_ticket', 'queued', $4)
            "#,
        )
        .bind(active_target_run_id)
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .bind(PROFILE_ID)
        .execute(&pool)
        .await
        .expect("insert active consultation target");

        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };
        RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_requests(&["backend_engineer"]),
                succeeded_request_apply(&["backend_engineer"]),
                None,
                None,
            )
            .await
            .expect("defer request while target is active");

        let pending = serde_json::json!({
            "recommendedAgentKey": "backend_engineer",
            "recommendedByAgentId": fx.pm_agent_id,
            "recommendedAt": "2026-08-03T00:00:00Z",
            "summary": "Ownership now wins"
        });
        sqlx::query("UPDATE tickets SET pending_assign_recommendation = $2 WHERE id = $1")
            .bind(fx.ticket_id)
            .bind(&pending)
            .execute(&pool)
            .await
            .expect("set pending ownership recommendation");
        sqlx::query("UPDATE agent_runs SET status = 'cancelled', ended_at = now() WHERE id = $1")
            .bind(active_target_run_id)
            .execute(&pool)
            .await
            .expect("terminalize active target");
        let terminal_run = RunService::new(&pool)
            .get(active_target_run_id)
            .await
            .expect("load terminal target run");

        RunOrchestrator::new(&pool, &workflow)
            .handle_terminal_run(&terminal_run)
            .await;

        let mention_status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM ticket_mentions WHERE ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("load ownership-superseded request");
        assert_eq!(mention_status, "handled");
        let response_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2 AND job_type = 'respond_to_mention'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count deferred responses");
        assert_eq!(response_count, 0);
    }

    #[tokio::test]
    async fn successful_request_start_error_leaves_pending_without_failing_source() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };

        let finished = RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_requests(&["backend_engineer"]),
                succeeded_request_apply(&["backend_engineer"]),
                None,
                None,
            )
            .await
            .expect("ordinary mention start errors must not fail the source run");
        assert_eq!(finished.status, RunStatus::Succeeded);

        let mention_status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM ticket_mentions WHERE ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("load persisted mention");
        assert_eq!(mention_status, "pending");

        let response_run_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_runs WHERE ticket_id = $1 AND job_type = 'respond_to_mention'",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("count response runs");
        assert_eq!(response_run_count, 0);
    }

    async fn assert_successful_human_profile_mentions_are_not_deferred(
        context_profile: ContextProfile,
    ) {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };

        RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "respond_to_mention".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_mentions(&["backend_engineer"]),
                succeeded_apply("Human-request response", &["backend_engineer"]),
                None,
                None,
            )
            .await
            .expect("finish human-profile response");

        let mention_status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM ticket_mentions WHERE ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("load human-profile mention");
        assert_eq!(mention_status, "ignored");
    }

    #[tokio::test]
    async fn successful_human_agent_mentions_are_not_deferred_for_auto_dispatch() {
        assert_successful_human_profile_mentions_are_not_deferred(ContextProfile::HumanAgent).await;
    }

    #[tokio::test]
    async fn successful_human_chat_mentions_are_not_deferred_for_auto_dispatch() {
        assert_successful_human_profile_mentions_are_not_deferred(ContextProfile::HumanChat).await;
    }

    async fn assert_terminal_active_target_releases_deferred_mention(cancel: bool) {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;
        let active_target_run_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id
            )
            VALUES ($1, $2, $3, 'work_on_ticket', 'running', $4)
            "#,
        )
        .bind(active_target_run_id)
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .bind(PROFILE_ID)
        .execute(&pool)
        .await
        .expect("insert active target run");

        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };
        let orchestrator = RunOrchestrator::new(&pool, &workflow);
        orchestrator
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_requests(&["backend_engineer"]),
                succeeded_request_apply(&["backend_engineer"]),
                None,
                None,
            )
            .await
            .expect("finish source with active target");

        let terminal_target = if cancel {
            RunService::new(&pool)
                .stop(active_target_run_id)
                .await
                .expect("cancel active target")
        } else {
            RunService::new(&pool)
                .finish_failed(active_target_run_id, "provider failed")
                .await
                .expect("fail active target")
        };
        orchestrator.handle_terminal_run(&terminal_target).await;

        let response_run_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2
              AND job_type = 'respond_to_mention' AND status = 'queued'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count deferred response runs");
        assert_eq!(response_run_count, 1);
    }

    #[tokio::test]
    async fn failed_active_target_releases_deferred_mention() {
        assert_terminal_active_target_releases_deferred_mention(false).await;
    }

    #[tokio::test]
    async fn cancelled_active_target_releases_deferred_mention() {
        assert_terminal_active_target_releases_deferred_mention(true).await;
    }

    #[tokio::test]
    async fn failed_ordinary_response_marks_trigger_mention_ignored_without_retry() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let comment = CommentService::new(&pool)
            .create(
                fx.ticket_id,
                AuthorType::Agent,
                Some(fx.pm_agent_id),
                "Please investigate",
                CommentIntent::ImplementationDone,
                &[],
                &[],
            )
            .await
            .expect("create source comment");
        let mention = MentionService::new(&pool)
            .create_mentions(
                fx.ticket_id,
                comment.id,
                &["backend_engineer".into()],
                None,
                fx.project_id,
            )
            .await
            .expect("create ordinary mention")
            .into_iter()
            .next()
            .expect("mention persisted");
        let failed_response_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                trigger_comment_id
            )
            VALUES ($1, $2, $3, 'respond_to_mention', 'failed', $4, $5)
            "#,
        )
        .bind(failed_response_id)
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .bind(PROFILE_ID)
        .bind(comment.id)
        .execute(&pool)
        .await
        .expect("insert failed response run");

        RunOrchestrator::new(&pool, &WorkflowConfig::default())
            .handle_terminal_run(&AgentRun {
                id: failed_response_id,
                ticket_id: fx.ticket_id,
                agent_id: fx.engineer_agent_id,
                job_type: "respond_to_mention".into(),
                status: RunStatus::Failed,
                sandbox_profile_id: PROFILE_ID.to_string(),
                worktree_path: None,
                branch_name: None,
                error_message: Some("provider failed".into()),
                session_id: None,
                context_profile: ContextProfile::Full,
                trigger_comment_id: Some(comment.id),
                started_at: None,
                ended_at: None,
                created_at: time::OffsetDateTime::now_utc(),
            })
            .await;

        let status =
            sqlx::query_scalar::<_, String>("SELECT status FROM ticket_mentions WHERE id = $1")
                .bind(mention.id)
                .fetch_one(&pool)
                .await
                .expect("load mention status");
        assert_eq!(status, "ignored");
    }

    #[tokio::test]
    async fn concurrent_automatic_starts_create_one_active_run() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;
        let run_svc = RunService::new(&pool);

        let (first, second) = tokio::join!(
            run_svc.start_run_for_agent(
                fx.ticket_id,
                fx.engineer_agent_id,
                "respond_to_mention",
                StartRunOptions::default(),
            ),
            run_svc.start_run_for_agent(
                fx.ticket_id,
                fx.engineer_agent_id,
                "respond_to_mention",
                StartRunOptions::default(),
            ),
        );

        let results = [first, second];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(RunError::ActiveRunExists)))
                .count(),
            1
        );

        let run_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2
              AND status IN ('queued', 'running')
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count active target runs");
        assert_eq!(run_count, 1);
    }

    #[tokio::test]
    async fn assigned_start_reports_existing_active_run() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;

        let err = RunService::new(&pool)
            .start_run(fx.ticket_id)
            .await
            .expect_err("fixture already has an active PM run");
        assert!(matches!(err, RunError::ActiveRunExists));
    }

    #[tokio::test]
    async fn successful_requests_ignore_unknown_disabled_duplicate_and_self_targets() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;
        let disabled_agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt,
                connector, enabled, preset_source
            )
            VALUES ($1, 'Disabled Agent', 'Reviewer', '{}', '{}', 'prompt', 'mock', false, 'disabled_agent')
            "#,
        )
        .bind(disabled_agent_id)
        .execute(&pool)
        .await
        .expect("insert disabled agent");

        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };
        let request_keys = [
            "unknown_agent",
            "pm",
            "backend_engineer",
            "backend-engineer",
            "backend_engineer",
            "disabled_agent",
        ];
        let finished = RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_requests(&request_keys),
                succeeded_request_apply(&request_keys),
                None,
                None,
            )
            .await
            .expect("finish run despite invalid request targets");
        assert_eq!(finished.status, RunStatus::Succeeded);

        let engineer_mention_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM ticket_mentions
            WHERE ticket_id = $1 AND mentioned_agent_id = $2
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count deduplicated engineer mentions");
        assert_eq!(engineer_mention_count, 1);

        let source_comment_body = sqlx::query_scalar::<_, String>(
            r#"
            SELECT body FROM ticket_comments
            WHERE ticket_id = $1 AND author_id = $2
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.pm_agent_id)
        .fetch_one(&pool)
        .await
        .expect("load filtered request comment");
        let persisted_requests =
            crate::services::agent_request::agent_requests_from_comment(&source_comment_body);
        assert_eq!(persisted_requests.len(), 1);
        assert_eq!(persisted_requests[0].agent_key, "backend_engineer");

        let target_runs = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT agent_id, job_type FROM agent_runs
            WHERE ticket_id = $1 AND job_type = 'respond_to_mention'
            "#,
        )
        .bind(fx.ticket_id)
        .fetch_all(&pool)
        .await
        .expect("load response runs");
        assert_eq!(
            target_runs,
            vec![(fx.engineer_agent_id, "respond_to_mention".into())]
        );
    }

    #[tokio::test]
    async fn blocked_self_mention_does_not_restart_terminalized_source() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;
        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };

        RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &blocked_with_mentions(&["pm"]),
                ApplyResult {
                    run_status: RunStatus::Blocked,
                    ticket: ApplyTicketUpdate {
                        status: None,
                        substatus: Some(Substatus::BlockedByError),
                        substatus_metadata: None,
                        updated_description: None,
                        acceptance_criteria: None,
                    },
                    comment: ApplyComment {
                        body: "Cannot answer myself".into(),
                        intent: CommentIntent::Blocked,
                        mentions: vec!["pm".into()],
                    },
                },
                None,
                None,
            )
            .await
            .expect("finish blocked self mention");

        let source_agent_runs = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_runs WHERE ticket_id = $1 AND agent_id = $2",
        )
        .bind(fx.ticket_id)
        .bind(fx.pm_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count source agent runs");
        assert_eq!(source_agent_runs, 1);
    }

    #[tokio::test]
    async fn duplicate_preset_key_uses_same_agent_for_handoff_and_persisted_mention() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;

        sqlx::query("UPDATE agents SET created_at = now() - interval '1 hour' WHERE id = $1")
            .bind(fx.engineer_agent_id)
            .execute(&pool)
            .await
            .expect("make original engineer the first preset instance");

        let second_engineer_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt,
                connector, enabled, preset_source
            )
            VALUES (
                $1, 'Secondary Backend Engineer', 'Backend Engineer', '{}', '{}',
                'prompt', 'mock', true, 'backend_engineer'
            )
            "#,
        )
        .bind(second_engineer_id)
        .execute(&pool)
        .await
        .expect("insert duplicate backend preset agent");

        let qc_agent_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, role, skills, responsibilities, system_prompt,
                connector, enabled, preset_source
            )
            VALUES ($1, 'QC Agent', 'QC', '{}', '{}', 'prompt', 'mock', true, 'qc')
            "#,
        )
        .bind(qc_agent_id)
        .execute(&pool)
        .await
        .expect("insert qc agent");

        sqlx::query("UPDATE tickets SET status = 'in_qa', assignee_agent_id = $2 WHERE id = $1")
            .bind(fx.ticket_id)
            .bind(qc_agent_id)
            .execute(&pool)
            .await
            .expect("prepare verification handoff");
        sqlx::query("UPDATE agent_runs SET agent_id = $2 WHERE id = $1")
            .bind(fx.run_id)
            .bind(qc_agent_id)
            .execute(&pool)
            .await
            .expect("make source run belong to qc");

        let contract = AgentRunResult::Done {
            summary: "Defect found".into(),
            changed_files: vec![],
            tests_run: vec![],
            next_status: None,
            assign_to: None,
            updated_description: None,
            acceptance_criteria: None,
            mention_agents: vec!["backend_engineer".into()],
            agent_requests: vec![],
            blockers: vec!["Regression remains".into()],
            split_tickets: vec![],
        };
        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };

        RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: qc_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &contract,
                succeeded_apply("Returning defect to backend", &["backend_engineer"]),
                None,
                None,
            )
            .await
            .expect("finish qc defect handoff");

        let target_runs = sqlx::query_as::<_, (Uuid, Uuid, String)>(
            r#"
            SELECT id, agent_id, job_type FROM agent_runs
            WHERE ticket_id = $1 AND id <> $2
            ORDER BY created_at ASC
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.run_id)
        .fetch_all(&pool)
        .await
        .expect("load handoff runs");
        assert_eq!(
            target_runs,
            vec![(
                target_runs[0].0,
                fx.engineer_agent_id,
                "work_on_ticket".into(),
            )]
        );

        let mentions = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT mentioned_agent_id, status FROM ticket_mentions WHERE ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_all(&pool)
        .await
        .expect("load handoff mention");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0], (fx.engineer_agent_id, "handled".into()));

        let ticket = TicketService::new(&pool)
            .get(fx.ticket_id)
            .await
            .expect("load handed-off ticket");
        assert_eq!(ticket.ticket.assignee_agent_id, Some(fx.engineer_agent_id));

        let handoff_run_id = target_runs[0].0;
        sqlx::query("UPDATE agent_runs SET status = 'running' WHERE id = $1")
            .bind(handoff_run_id)
            .execute(&pool)
            .await
            .expect("mark handoff run running");
        RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: handoff_run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.engineer_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_mentions(&[]),
                succeeded_apply("Defect fixed", &[]),
                None,
                None,
            )
            .await
            .expect("finish handoff run");

        let response_run_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_runs WHERE ticket_id = $1 AND job_type = 'respond_to_mention'",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("count response runs after handoff completion");
        assert_eq!(response_run_count, 0);
    }

    #[tokio::test]
    async fn successful_response_request_does_not_chain_or_change_ticket_state() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;
        sqlx::query(
            r#"
            UPDATE tickets
            SET status = 'in_progress', substatus = 'waiting_for_human', assignee_agent_id = $2
            WHERE id = $1
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .execute(&pool)
        .await
        .expect("prepare ticket state");
        sqlx::query("UPDATE agent_runs SET agent_id = $2 WHERE id = $1")
            .bind(fx.run_id)
            .bind(fx.engineer_agent_id)
            .execute(&pool)
            .await
            .expect("make source run belong to engineer");

        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };
        let orchestrator = RunOrchestrator::new(&pool, &workflow);
        orchestrator
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.engineer_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &done_with_requests(&["pm"]),
                succeeded_request_apply(&["pm"]),
                None,
                None,
            )
            .await
            .expect("finish engineer consultation request run");

        let (pm_run_id, trigger_comment_id) = sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
            r#"
            SELECT id, trigger_comment_id FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2 AND job_type = 'respond_to_mention'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.pm_agent_id)
        .fetch_one(&pool)
        .await
        .expect("load PM response run");
        sqlx::query("UPDATE agent_runs SET status = 'running' WHERE id = $1")
            .bind(pm_run_id)
            .execute(&pool)
            .await
            .expect("mark PM run running");

        let before = TicketService::new(&pool)
            .get(fx.ticket_id)
            .await
            .expect("load ticket before chained response");
        assert_eq!(before.ticket.status, TicketStatus::InReview);
        assert_eq!(before.ticket.substatus, Some(Substatus::WaitingForHuman));
        assert_eq!(before.ticket.assignee_agent_id, Some(fx.engineer_agent_id));
        let pending_pm_mentions = sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
            r#"
            SELECT id, resume_agent_id FROM ticket_mentions
            WHERE ticket_id = $1 AND mentioned_agent_id = $2 AND status = 'pending'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.pm_agent_id)
        .fetch_all(&pool)
        .await
        .expect("load pending PM mentions");
        assert_eq!(pending_pm_mentions.len(), 1);
        assert_eq!(pending_pm_mentions[0].1, None);

        let response_contract = AgentRunResult::Done {
            summary: "Consultation answered".into(),
            changed_files: vec!["must-not-apply.rs".into()],
            tests_run: vec!["must-not-run".into()],
            next_status: Some("Done".into()),
            assign_to: Some("backend_engineer".into()),
            updated_description: Some("Must not replace the description".into()),
            acceptance_criteria: Some("- Must not replace criteria".into()),
            mention_agents: vec![],
            agent_requests: vec![crate::providers::AgentRequest {
                agent_key: "backend_engineer".into(),
                intent: "consult".into(),
                request: "This follow-up must remain notification-only.".into(),
            }],
            blockers: vec![],
            split_tickets: vec![crate::domain::workflow::SplitTicketSpec {
                title: "Must not split".into(),
                description: "Ignored response output".into(),
                acceptance_criteria: None,
                assign_to: None,
            }],
        };
        let mut malicious_apply =
            crate::services::result_contract::apply_agent_result(&response_contract)
                .expect("apply provider response");
        malicious_apply.ticket.substatus = Some(Substatus::BlockedByError);
        malicious_apply.ticket.substatus_metadata =
            Some(serde_json::json!({ "reason": "must not apply" }));

        orchestrator
            .finish_run(
                &AgentRun {
                    id: pm_run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "respond_to_mention".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &response_contract,
                malicious_apply,
                None,
                None,
            )
            .await
            .expect("finish one-hop response");

        let after = TicketService::new(&pool)
            .get(fx.ticket_id)
            .await
            .expect("load ticket after chained response");
        assert_eq!(after.ticket.status, before.ticket.status);
        assert_eq!(after.ticket.substatus, before.ticket.substatus);
        assert_eq!(
            after.ticket.assignee_agent_id,
            before.ticket.assignee_agent_id
        );
        assert_eq!(after.ticket.description, before.ticket.description);
        assert_eq!(
            after.ticket.pending_assign_recommendation,
            before.ticket.pending_assign_recommendation
        );
        assert_eq!(
            after.ticket.pending_split_recommendation,
            before.ticket.pending_split_recommendation
        );
        let child_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM tickets WHERE parent_ticket_id = $1",
        )
        .bind(fx.ticket_id)
        .fetch_one(&pool)
        .await
        .expect("count response-created children");
        assert_eq!(child_count, 0);
        let handled_pm_mentions = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM ticket_mentions
            WHERE ticket_id = $1 AND mentioned_agent_id = $2 AND status = 'handled'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.pm_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count handled PM mentions");
        assert_eq!(handled_pm_mentions, 1);

        let chained_run_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2
              AND job_type = 'respond_to_mention' AND status = 'queued'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count chained response runs");
        assert_eq!(chained_run_count, 0);
    }

    #[tokio::test]
    async fn blocked_consultation_keeps_ticket_lifecycle_and_assignment_unchanged() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;
        sqlx::query("UPDATE agent_runs SET job_type = 'respond_to_mention' WHERE id = $1")
            .bind(fx.run_id)
            .execute(&pool)
            .await
            .expect("persist consultation source job type");
        let pending = serde_json::json!({
            "recommendedAgentKey": "backend_engineer",
            "recommendedByAgentId": fx.pm_agent_id,
            "recommendedAt": "2026-08-03T00:00:00Z",
            "summary": "Pending ownership"
        });
        sqlx::query(
            r#"
            UPDATE tickets
            SET status = 'in_progress', substatus = 'waiting_for_human',
                assignee_agent_id = $2, description = 'Original description',
                pending_assign_recommendation = $3
            WHERE id = $1
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .bind(&pending)
        .execute(&pool)
        .await
        .expect("prepare blocked consultation ticket");

        let contract = AgentRunResult::Blocked {
            blocker_type: "permission".into(),
            summary: concat!(
                "Cannot inspect the protected dependency.\n\n",
                "<!-- coppice-agent-requests: ",
                r#"[{"agentKey":"backend_engineer","intent":"consult","request":"Run me"}]"#,
                " -->"
            )
            .into(),
            next_status: Some("Blocked".into()),
            assign_to: Some("pm".into()),
            updated_description: Some("Must not replace description".into()),
            acceptance_criteria: Some("- Must not replace criteria".into()),
            mention_agents: vec!["backend_engineer".into()],
            required_capabilities: vec![],
            required_secrets: vec![],
        };
        let apply = crate::services::result_contract::apply_consultation_result(&contract)
            .expect("apply blocked consultation result");

        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };
        let orchestrator = RunOrchestrator::new(&pool, &workflow);
        orchestrator
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "respond_to_mention".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &contract,
                apply,
                None,
                None,
            )
            .await
            .expect("finish blocked consultation");

        let ticket = TicketService::new(&pool)
            .get(fx.ticket_id)
            .await
            .expect("load ticket after blocked consultation");
        assert_eq!(ticket.ticket.status, TicketStatus::InProgress);
        assert_eq!(ticket.ticket.substatus, Some(Substatus::WaitingForHuman));
        assert_eq!(ticket.ticket.assignee_agent_id, Some(fx.engineer_agent_id));
        assert_eq!(ticket.ticket.description, "Original description");
        assert_eq!(ticket.ticket.pending_assign_recommendation, Some(pending));

        let mentions = MentionService::new(&pool)
            .list_pending_for_ticket(fx.ticket_id)
            .await
            .expect("load blocked consultation mention");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].mentioned_agent_id, fx.engineer_agent_id);
        assert_eq!(mentions[0].resume_agent_id, None);

        let response_run_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2 AND job_type = 'respond_to_mention'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count blocked consultation response runs");
        assert_eq!(response_run_count, 0);

        // Remove ownership precedence only to exercise deferred request
        // scheduling after the invariance assertions above.
        sqlx::query(
            r#"
            UPDATE tickets
            SET assignee_agent_id = $2, pending_assign_recommendation = NULL
            WHERE id = $1
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.pm_agent_id)
        .execute(&pool)
        .await
        .expect("remove engineer ownership suppression");

        let terminal_engineer_run_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id,
                error_message, ended_at
            )
            VALUES ($1, $2, $3, 'work_on_ticket', 'failed', $4, $5, now())
            "#,
        )
        .bind(terminal_engineer_run_id)
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .bind(PROFILE_ID)
        .bind("provider failed")
        .execute(&pool)
        .await
        .expect("insert terminal engineer run");
        let terminal_engineer_run = RunService::new(&pool)
            .get(terminal_engineer_run_id)
            .await
            .expect("load terminal engineer run");

        orchestrator
            .handle_terminal_run(&terminal_engineer_run)
            .await;

        let deferred_response_run_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2 AND job_type = 'respond_to_mention'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count deferred blocked consultation response runs");
        assert_eq!(deferred_response_run_count, 0);
    }

    #[tokio::test]
    async fn blocked_clarification_auto_start_error_is_returned() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        sqlx::query("UPDATE tickets SET status = $2, assignee_agent_id = $3 WHERE id = $1")
            .bind(fx.ticket_id)
            .bind("in_progress")
            .bind(fx.engineer_agent_id)
            .execute(&pool)
            .await
            .expect("prepare ticket");
        sqlx::query("UPDATE agent_runs SET agent_id = $2 WHERE id = $1")
            .bind(fx.run_id)
            .bind(fx.engineer_agent_id)
            .execute(&pool)
            .await
            .expect("prepare engineer run");

        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };
        let error = RunOrchestrator::new(&pool, &workflow)
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.engineer_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &blocked_with_mentions(&["pm"]),
                ApplyResult {
                    run_status: RunStatus::Blocked,
                    ticket: ApplyTicketUpdate {
                        status: None,
                        substatus: Some(Substatus::BlockedByError),
                        substatus_metadata: Some(
                            serde_json::json!({ "reason": "Need clarification" }),
                        ),
                        updated_description: None,
                        acceptance_criteria: None,
                    },
                    comment: ApplyComment {
                        body: "Need clarification".into(),
                        intent: CommentIntent::Blocked,
                        mentions: vec!["pm".into()],
                    },
                },
                None,
                None,
            )
            .await
            .expect_err("missing repository must surface the required follow-up start failure");

        assert!(matches!(
            error,
            RunError::Validation(message) if message == "ticket has no repo"
        ));
        let mention = MentionService::new(&pool)
            .list_pending_for_ticket(fx.ticket_id)
            .await
            .expect("load clarification mention")
            .into_iter()
            .next()
            .expect("clarification mention persisted");
        assert_eq!(mention.resume_agent_id, Some(fx.engineer_agent_id));
    }

    #[tokio::test]
    async fn orchestrator_blocked_mention_enqueues_respond_to_mention_when_auto_start() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;

        sqlx::query("UPDATE tickets SET status = $2, assignee_agent_id = $3 WHERE id = $1")
            .bind(fx.ticket_id)
            .bind("in_progress")
            .bind(fx.engineer_agent_id)
            .execute(&pool)
            .await
            .expect("update ticket");

        sqlx::query("UPDATE agent_runs SET agent_id = $2, job_type = $3 WHERE id = $1")
            .bind(fx.run_id)
            .bind(fx.engineer_agent_id)
            .bind("work_on_ticket")
            .execute(&pool)
            .await
            .expect("update run");

        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };
        let orchestrator = RunOrchestrator::new(&pool, &workflow);

        let contract = blocked_with_mentions(&["pm"]);
        let apply = ApplyResult {
            run_status: RunStatus::Blocked,
            ticket: ApplyTicketUpdate {
                status: None,
                substatus: Some(Substatus::BlockedByError),
                substatus_metadata: Some(serde_json::json!({ "reason": "Need clarification" })),
                updated_description: None,
                acceptance_criteria: None,
            },
            comment: ApplyComment {
                body: "Need clarification".into(),
                intent: CommentIntent::Blocked,
                mentions: vec!["pm".into()],
            },
        };

        orchestrator
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.engineer_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &contract,
                apply,
                None,
                None,
            )
            .await
            .expect("finish run");

        let ticket = TicketService::new(&pool)
            .get(fx.ticket_id)
            .await
            .expect("load ticket");
        assert_eq!(ticket.ticket.substatus, Some(Substatus::WaitingForAgent));

        let mentions = MentionService::new(&pool)
            .list_pending_for_ticket(fx.ticket_id)
            .await
            .expect("list mentions");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].mentioned_agent_id, fx.pm_agent_id);
        assert_eq!(mentions[0].resume_agent_id, Some(fx.engineer_agent_id));

        let jobs = JobService::new(&pool).list_all().await.expect("list jobs");
        assert!(jobs.iter().any(|j| j.job_type == "respond_to_mention"));
    }

    #[tokio::test]
    async fn orchestrator_respond_to_mention_resumes_engineer_when_under_limit() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;

        // Fixture seeds a running PM run; clear it so auto-start can enqueue respond_to_mention.
        sqlx::query("UPDATE agent_runs SET status = $1 WHERE id = $2")
            .bind(run_status_to_str(RunStatus::Succeeded))
            .bind(fx.run_id)
            .execute(&pool)
            .await
            .expect("clear fixture pm run");

        sqlx::query("UPDATE tickets SET status = $2, assignee_agent_id = $3 WHERE id = $1")
            .bind(fx.ticket_id)
            .bind("in_progress")
            .bind(fx.engineer_agent_id)
            .execute(&pool)
            .await
            .expect("update ticket");

        let workflow = WorkflowConfig {
            auto_start_runs: true,
            ..WorkflowConfig::default()
        };
        let orchestrator = RunOrchestrator::new(&pool, &workflow);

        let ordinary_comment = CommentService::new(&pool)
            .create(
                fx.ticket_id,
                AuthorType::Agent,
                Some(fx.engineer_agent_id),
                "Earlier non-blocking question for PM",
                CommentIntent::ProgressUpdate,
                &[],
                &[],
            )
            .await
            .expect("create older ordinary mention comment");
        let ordinary_mention = MentionService::new(&pool)
            .create_mentions(
                fx.ticket_id,
                ordinary_comment.id,
                &["pm".into()],
                None,
                fx.project_id,
            )
            .await
            .expect("create older ordinary mention")
            .into_iter()
            .next()
            .expect("ordinary mention persisted");

        // Engineer blocks and mentions PM
        let block_run_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(block_run_id)
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .bind("work_on_ticket")
        .bind(run_status_to_str(RunStatus::Running))
        .bind(PROFILE_ID)
        .execute(&pool)
        .await
        .expect("insert block run");

        orchestrator
            .finish_run(
                &AgentRun {
                    id: block_run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.engineer_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &blocked_with_mentions(&["pm"]),
                ApplyResult {
                    run_status: RunStatus::Blocked,
                    ticket: ApplyTicketUpdate {
                        status: None,
                        substatus: Some(Substatus::BlockedByError),
                        substatus_metadata: Some(
                            serde_json::json!({ "reason": "Need clarification" }),
                        ),
                        updated_description: None,
                        acceptance_criteria: None,
                    },
                    comment: ApplyComment {
                        body: "Need clarification".into(),
                        intent: CommentIntent::Blocked,
                        mentions: vec!["pm".into()],
                    },
                },
                None,
                None,
            )
            .await
            .expect("finish blocked run");

        let (pm_run_id, trigger_comment_id) = sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
            r#"
            SELECT id, trigger_comment_id FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2 AND job_type = 'respond_to_mention'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.pm_agent_id)
        .fetch_one(&pool)
        .await
        .expect("auto-started pm respond_to_mention run");
        let trigger_comment_id = trigger_comment_id.expect("response run links blocked comment");
        let clarification_mention_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id FROM ticket_mentions
            WHERE ticket_id = $1 AND mentioned_agent_id = $2 AND comment_id = $3
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.pm_agent_id)
        .bind(trigger_comment_id)
        .fetch_one(&pool)
        .await
        .expect("load linked clarification mention");

        sqlx::query("UPDATE agent_runs SET status = $1 WHERE id = $2")
            .bind(run_status_to_str(RunStatus::Running))
            .bind(pm_run_id)
            .execute(&pool)
            .await
            .expect("mark pm run running");

        orchestrator
            .finish_run(
                &AgentRun {
                    id: pm_run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.pm_agent_id,
                    job_type: "respond_to_mention".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: Some(trigger_comment_id),
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &AgentRunResult::Done {
                    summary: "Use option A".into(),
                    changed_files: vec![],
                    tests_run: vec![],
                    next_status: None,
                    assign_to: None,
                    updated_description: None,
                    acceptance_criteria: None,
                    mention_agents: vec!["backend_engineer".into()],
                    agent_requests: vec![],
                    blockers: vec![],
                    split_tickets: vec![],
                },
                ApplyResult {
                    run_status: RunStatus::Succeeded,
                    ticket: ApplyTicketUpdate {
                        status: None,
                        substatus: None,
                        substatus_metadata: None,
                        updated_description: None,
                        acceptance_criteria: None,
                    },
                    comment: ApplyComment {
                        body: "Use option A".into(),
                        intent: CommentIntent::ClarificationAnswer,
                        mentions: vec!["backend_engineer".into()],
                    },
                },
                None,
                None,
            )
            .await
            .expect("finish pm run");

        let ticket = TicketService::new(&pool)
            .get(fx.ticket_id)
            .await
            .expect("load ticket");
        assert_eq!(ticket.ticket.substatus, None);
        assert_eq!(ticket.ticket.assignee_agent_id, Some(fx.engineer_agent_id));
        assert_eq!(ticket.ticket.clarification_round, 1);

        let mentions = MentionService::new(&pool)
            .list_pending_for_ticket(fx.ticket_id)
            .await
            .expect("list mentions");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].id, ordinary_mention.id);

        let handled_resume_mention_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM ticket_mentions
            WHERE ticket_id = $1 AND mentioned_agent_id = $2
              AND resume_agent_id IS NULL AND status = 'handled'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count response mention covered by resume handoff");
        assert_eq!(handled_resume_mention_count, 1);

        let mention_statuses = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, status FROM ticket_mentions WHERE id = ANY($1)",
        )
        .bind(vec![ordinary_mention.id, clarification_mention_id])
        .fetch_all(&pool)
        .await
        .expect("load mention statuses");
        assert!(mention_statuses
            .iter()
            .any(|(id, status)| *id == ordinary_mention.id && status == "pending"));
        assert!(mention_statuses
            .iter()
            .any(|(id, status)| *id == clarification_mention_id && status == "handled"));

        let resume_run_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2
              AND job_type = 'work_on_ticket' AND status = 'queued'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count resume runs");
        assert_eq!(resume_run_count, 1);

        let duplicate_response_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM agent_runs
            WHERE ticket_id = $1 AND agent_id = $2
              AND job_type = 'respond_to_mention'
            "#,
        )
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .fetch_one(&pool)
        .await
        .expect("count duplicate response runs");
        assert_eq!(duplicate_response_count, 0);
    }

    #[tokio::test]
    async fn continuation_context_includes_progress_update() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;

        CommentService::new(&pool)
            .create(
                fx.ticket_id,
                AuthorType::Agent,
                Some(fx.engineer_agent_id),
                "Implemented TmuxStream create/kill; capture loop next.",
                CommentIntent::ProgressUpdate,
                &[],
                &[],
            )
            .await
            .expect("insert progress comment");

        let run = AgentRun {
            id: fx.run_id,
            ticket_id: fx.ticket_id,
            agent_id: fx.engineer_agent_id,
            job_type: "work_on_ticket".into(),
            status: RunStatus::Queued,
            sandbox_profile_id: PROFILE_ID.to_string(),
            worktree_path: None,
            branch_name: None,
            error_message: None,
            session_id: None,
            context_profile: ContextProfile::Full,
            trigger_comment_id: None,
            started_at: None,
            ended_at: None,
            created_at: time::OffsetDateTime::now_utc(),
        };

        let ctx = load_run_continuation_context(&pool, &run)
            .await
            .expect("load continuation context")
            .expect("resume context");

        assert!(ctx.contains("Recent activity on this ticket"));
        assert!(ctx.contains("Implemented TmuxStream create/kill"));
        assert!(ctx.contains("progress update"));
    }

    #[tokio::test]
    async fn continued_run_resume_appears_in_context_file() {
        use crate::domain::context_profile::ContextProfile;
        use crate::providers::fixtures_root;
        use crate::services::context_builder::{
            build_context_md, write_context_file, ContextInput,
        };
        use crate::services::result_contract::apply_agent_result;

        let Some(pool) = test_pool().await else {
            return;
        };
        let fx = insert_fixture(&pool).await;
        let _repo_dir = attach_ready_repo(&pool, fx.ticket_id).await;

        sqlx::query("UPDATE tickets SET status = $2, assignee_agent_id = $3 WHERE id = $1")
            .bind(fx.ticket_id)
            .bind("in_progress")
            .bind(fx.engineer_agent_id)
            .execute(&pool)
            .await
            .expect("update ticket");

        let continued_path = fixtures_root().join("backend_engineer/continued.json");
        let raw = std::fs::read_to_string(&continued_path).expect("read continued fixture");
        let contract: AgentRunResult = serde_json::from_str(&raw).expect("parse continued fixture");
        let apply = apply_agent_result(&contract).expect("apply continued");

        let workflow = WorkflowConfig::default();
        let orchestrator = RunOrchestrator::new(&pool, &workflow);
        orchestrator
            .finish_run(
                &AgentRun {
                    id: fx.run_id,
                    ticket_id: fx.ticket_id,
                    agent_id: fx.engineer_agent_id,
                    job_type: "work_on_ticket".into(),
                    status: RunStatus::Running,
                    sandbox_profile_id: PROFILE_ID.to_string(),
                    worktree_path: None,
                    branch_name: None,
                    error_message: None,
                    session_id: None,
                    context_profile: ContextProfile::Full,
                    trigger_comment_id: None,
                    started_at: None,
                    ended_at: None,
                    created_at: time::OffsetDateTime::now_utc(),
                },
                &contract,
                apply,
                None,
                None,
            )
            .await
            .expect("finish continued run");

        let second_run_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO agent_runs (
                id, ticket_id, agent_id, job_type, status, sandbox_profile_id
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(second_run_id)
        .bind(fx.ticket_id)
        .bind(fx.engineer_agent_id)
        .bind("work_on_ticket")
        .bind(run_status_to_str(RunStatus::Queued))
        .bind(PROFILE_ID)
        .execute(&pool)
        .await
        .expect("insert second run");

        let second_run = AgentRun {
            id: second_run_id,
            ticket_id: fx.ticket_id,
            agent_id: fx.engineer_agent_id,
            job_type: "work_on_ticket".into(),
            status: RunStatus::Queued,
            sandbox_profile_id: PROFILE_ID.to_string(),
            worktree_path: None,
            branch_name: None,
            error_message: None,
            session_id: None,
            context_profile: ContextProfile::Full,
            trigger_comment_id: None,
            started_at: None,
            ended_at: None,
            created_at: time::OffsetDateTime::now_utc(),
        };

        let resume_context = load_run_continuation_context(&pool, &second_run)
            .await
            .expect("load continuation")
            .expect("resume context after continued");

        let worktree = tempfile::tempdir().expect("worktree tempdir");
        let context_input = ContextInput {
            ticket_title: "orchestrator ticket",
            ticket_description: "",
            ticket_status: "in_progress",
            ticket_substatus: None,
            agent_name: "Backend Engineer",
            agent_key: "backend_engineer",
            agent_role: "Backend Engineer",
            agent_skills: &[],
            agent_responsibilities: &[],
            agent_system_prompt: "prompt",
            repo_name: None,
            repo_remote_url: None,
            repo_default_branch: None,
            worktree_path: None,
            resume_context: Some(&resume_context),
            context_profile: ContextProfile::Full,
            human_request: None,
            ticket_id: None,
            assignee_agent_key: None,
            thread_excerpt: None,
            consultation_request: None,
        };
        write_context_file(worktree.path(), &context_input).expect("write context");
        let md = std::fs::read_to_string(worktree.path().join(".agent/context.md"))
            .expect("read context.md");

        assert!(md.contains("## Ticket thread"));
        assert!(md.contains("Implemented TmuxStream create/kill"));
        assert!(md.contains("tmux_stream.rs"));

        let built = build_context_md(&context_input);
        assert!(built.len() >= md.len());
    }
}
