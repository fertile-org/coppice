use std::collections::HashMap;

use crate::config::WorkflowConfig;
use crate::domain::agent::Agent;
use crate::domain::comment::{AuthorType, CommentIntent};
use crate::domain::run::{AgentRun, RunStatus};
use crate::domain::slug::slugify;
use crate::domain::substatus::Substatus;
use crate::domain::ticket::status_to_str;
use crate::domain::workflow::{RunOutcome, TransitionAction, TransitionContext};
use crate::providers::AgentRunResult;
use crate::services::agent_service::AgentService;
use crate::services::comment_service::{CommentError, CommentService};
use crate::services::mention_service::MentionService;
use crate::services::result_contract::ApplyResult;
use crate::services::run_service::{RunError, RunService};
use crate::services::ticket_service::TicketService;
use crate::services::workflow_service::WorkflowService;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub struct RunOrchestrator<'a> {
    pool: &'a PgPool,
    workflow: &'a WorkflowConfig,
}

pub async fn load_resume_context(
    pool: &PgPool,
    run: &AgentRun,
) -> Result<Option<String>, CommentError> {
    if run.job_type != "work_on_ticket" {
        return Ok(None);
    }

    let comments = CommentService::new(pool)
        .list_by_ticket(run.ticket_id)
        .await?;

    let answer_idx = comments
        .iter()
        .rposition(|c| c.intent == CommentIntent::ClarificationAnswer);

    let Some(answer_idx) = answer_idx else {
        return Ok(None);
    };

    let blocker = comments[..answer_idx]
        .iter()
        .rfind(|c| c.intent == CommentIntent::Blocked);

    let Some(blocker) = blocker else {
        return Ok(None);
    };

    let answer = &comments[answer_idx];
    Ok(Some(format!(
        "**Prior blocker:**\n\n{}\n\n**PM answer:**\n\n{}",
        blocker.body, answer.body
    )))
}

impl<'a> RunOrchestrator<'a> {
    pub fn new(pool: &'a PgPool, workflow: &'a WorkflowConfig) -> Self {
        Self { pool, workflow }
    }

    pub async fn finish_run(
        &self,
        run: &AgentRun,
        contract: &AgentRunResult,
        apply: ApplyResult,
        worktree_path: Option<String>,
        branch_name: Option<String>,
    ) -> Result<AgentRun, RunError> {
        let ticket_svc = TicketService::new(self.pool);
        let ticket = ticket_svc.get(run.ticket_id).await?;
        let agent = AgentService::new(self.pool).get(run.agent_id).await?;
        let agents = AgentService::new(self.pool).list_agents().await?;
        let (project_agent_keys, project_agent_ids) = build_project_agent_maps(&agents);

        let agent_key = agent
            .preset_source
            .clone()
            .unwrap_or_else(|| slugify(&agent.name));

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
            project_agent_ids,
            auto_assign_enabled,
        };

        let action = WorkflowService::resolve_transition(ctx)
            .map_err(RunError::Validation)?;

        let (substatus, substatus_metadata) = merge_substatus(&action, &apply);

        ticket_svc
            .apply_workflow_update(
                run.ticket_id,
                action.new_status,
                substatus,
                substatus_metadata,
                action.new_assignee_id,
                action.pending_recommendation,
                i32::from(action.increment_clarification_round),
            )
            .await?;

        let comment = CommentService::new(self.pool)
            .create(
                run.ticket_id,
                AuthorType::Agent,
                Some(run.agent_id),
                &apply.comment.body,
                apply.comment.intent,
                &[],
                &apply.comment.mentions,
            )
            .await?;

        let mention_keys = mention_agents_from_contract(contract);
        if !mention_keys.is_empty() {
            let resume_agent_id = if apply.run_status == RunStatus::Blocked {
                Some(run.agent_id)
            } else {
                None
            };
            MentionService::new(self.pool)
                .create_mentions(
                    run.ticket_id,
                    comment.id,
                    &mention_keys,
                    resume_agent_id,
                    ticket.ticket.project_id,
                )
                .await?;
        }

        if self.workflow.auto_start_runs {
            let run_svc = RunService::new(self.pool);
            for job_req in &action.enqueue_jobs {
                run_svc
                    .start_run_for_agent(run.ticket_id, job_req.agent_id, &job_req.job_type)
                    .await?;
            }
        }

        RunService::new(self.pool)
            .finish_run(run.id, apply.run_status, worktree_path, branch_name)
            .await
    }
}

fn build_project_agent_maps(agents: &[Agent]) -> (Vec<String>, HashMap<String, Uuid>) {
    let mut keys = Vec::new();
    let mut ids = HashMap::new();

    for agent in agents {
        if !agent.enabled {
            continue;
        }
        if let Some(ref preset) = agent.preset_source {
            if !keys.iter().any(|k| k == preset) {
                keys.push(preset.clone());
            }
            ids.insert(preset.clone(), agent.id);
        }
        let slug = slugify(&agent.name);
        if !keys.iter().any(|k| k == &slug) {
            keys.push(slug.clone());
        }
        ids.insert(slug, agent.id);
    }

    (keys, ids)
}

fn merge_substatus(
    action: &TransitionAction,
    apply: &ApplyResult,
) -> (Option<Option<Substatus>>, Option<Option<Value>>) {
    let substatus = match &action.substatus {
        Some(value) => Some(*value),
        None => apply.ticket.substatus.map(|s| Some(s)),
    };
    let substatus_metadata = match &action.substatus_metadata {
        Some(value) => Some(value.clone()),
        None => apply.ticket.substatus_metadata.clone().map(|m| Some(m)),
    };
    (substatus, substatus_metadata)
}

fn mention_agents_from_contract(contract: &AgentRunResult) -> Vec<String> {
    match contract {
        AgentRunResult::Done { mention_agents, .. }
        | AgentRunResult::Blocked { mention_agents, .. } => mention_agents.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::comment::CommentIntent;
    use crate::domain::run::run_status_to_str;
    use crate::domain::substatus::TicketStatus;
    use crate::sandbox::permissive::PROFILE_ID;
    use crate::services::job_service::JobService;
    use crate::services::result_contract::ApplyResult;
    use crate::services::result_contract::{ApplyComment, ApplyTicketUpdate};
    use coppice_config::WorkflowConfig;

    async fn test_pool() -> Option<PgPool> {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://coppice:coppice@localhost:5432/coppice".into()
        });
        crate::db::connect_and_migrate(&database_url).await.ok()
    }

    struct TestFixture {
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
            mention_agents: vec![],
            blockers: vec![],
        }
    }

    fn blocked_with_mentions(keys: &[&str]) -> AgentRunResult {
        AgentRunResult::Blocked {
            blocker_type: "error".into(),
            summary: "Need clarification".into(),
            next_status: None,
            assign_to: None,
            mention_agents: keys.iter().map(|k| (*k).into()).collect(),
            required_capabilities: vec![],
            required_secrets: vec![],
        }
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

        let jobs = JobService::new(&pool)
            .list_all()
            .await
            .expect("list jobs");
        assert!(jobs.iter().any(|j| j.job_type == "respond_to_mention"));
    }
}
