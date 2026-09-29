pub mod agents;
pub mod comments;
pub mod knowledge;
pub mod result;
pub mod skills;
pub mod tickets;

use crate::mcp::catalog::CoreTool;
use crate::mcp::token::RunToolScope;
use crate::AppState;
use serde_json::Value;
use sqlx::PgPool;

pub struct ToolCtx<'a> {
    pub state: &'a AppState,
    pub pool: &'a PgPool,
    pub scope: &'a RunToolScope,
}

#[derive(Debug)]
pub enum ToolError {
    InvalidArgs(String),
    NotFound(String),
    Denied(String),
    Limit(String),
    Internal(anyhow::Error),
}

impl ToolError {
    /// Text returned to the agent; internal details stay in tracing only.
    pub fn message(&self) -> String {
        match self {
            Self::InvalidArgs(m) | Self::NotFound(m) | Self::Denied(m) | Self::Limit(m) => {
                m.clone()
            }
            Self::Internal(_) => "internal error".to_string(),
        }
    }
}

impl From<sqlx::Error> for ToolError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.into())
    }
}

pub async fn dispatch(tool: CoreTool, ctx: &ToolCtx<'_>, args: Value) -> Result<Value, ToolError> {
    match tool {
        CoreTool::BoardAgents => agents::call_board_agents(ctx, args).await,
        CoreTool::TicketGet => tickets::call_ticket_get(ctx, args).await,
        CoreTool::TicketComments => tickets::call_ticket_comments(ctx, args).await,
        CoreTool::TicketRuns => tickets::call_ticket_runs(ctx, args).await,
        CoreTool::TicketSearch => tickets::call_ticket_search(ctx, args).await,
        CoreTool::KnowledgeSearch => knowledge::call_knowledge_search(ctx, args).await,
        CoreTool::CommentPost => comments::call_comment_post(ctx, args).await,
        CoreTool::ResultSubmit => result::call_result_submit(ctx, args).await,
        CoreTool::SkillList => skills::call_skill_list(ctx, args).await,
        CoreTool::SkillLoad => skills::call_skill_load(ctx, args).await,
    }
}
