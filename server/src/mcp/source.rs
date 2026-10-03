use crate::mcp::catalog::{core_tools_for, CoreTool};
use crate::mcp::protocol::ToolDefinition;
pub use crate::mcp::protocol::{ToolContent, ToolResult};
use crate::mcp::token::RunToolScope;
use crate::mcp::tools::{self, skills, ToolCtx, ToolError};
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

/// Stored as `run_tool_calls.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Core,
    Skill,
    Plugin,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Skill => "skill",
            Self::Plugin => "plugin",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourcedTool {
    pub def: ToolDefinition,
    pub source: SourceKind,
    /// Set only for plugin-owned tools.
    pub plugin_id: Option<Uuid>,
    /// Source-private handle (e.g. the core tool name or `<server>/<tool>`).
    pub key: String,
}

/// A source's claim on a tool name it did not list, e.g. a tool of a plugin whose
/// server is down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlistedClaim {
    pub plugin_id: Option<Uuid>,
    /// Tool error shown to the agent and logged.
    pub message: String,
}

#[async_trait]
pub trait ToolSource: Send + Sync {
    fn kind(&self) -> SourceKind;
    async fn list(&self, scope: &RunToolScope) -> Vec<SourcedTool>;
    async fn call(
        &self,
        ctx: &ToolCtx<'_>,
        tool: &SourcedTool,
        args: Value,
    ) -> Result<ToolResult, ToolError>;
    /// Called when no source listed `name`, so the failed call is logged against
    /// the source that owns it rather than as an unknown core tool.
    async fn claim_unlisted(&self, _scope: &RunToolScope, _name: &str) -> Option<UnlistedClaim> {
        None
    }
}

fn json_result(value: Value) -> ToolResult {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    ToolResult::text(text, false)
}

fn sourced(def: ToolDefinition, source: SourceKind) -> SourcedTool {
    SourcedTool {
        key: def.name.clone(),
        def,
        source,
        plugin_id: None,
    }
}

/// Built-in board, ticket, knowledge, comment, and result tools.
pub struct CoreToolSource;

#[async_trait]
impl ToolSource for CoreToolSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Core
    }

    async fn list(&self, scope: &RunToolScope) -> Vec<SourcedTool> {
        core_tools_for(scope.profile)
            .into_iter()
            .map(|t| sourced(t.definition(), SourceKind::Core))
            .collect()
    }

    async fn call(
        &self,
        ctx: &ToolCtx<'_>,
        tool: &SourcedTool,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        let core = CoreTool::from_name(&tool.key).ok_or_else(|| {
            ToolError::Internal(anyhow::anyhow!("unknown core tool {}", tool.key))
        })?;
        tools::dispatch(core, ctx, args).await.map(json_result)
    }
}

/// `skill_list` / `skill_load` over the skill catalog, scoped by the run's plugin snapshot.
pub struct SkillToolSource;

#[async_trait]
impl ToolSource for SkillToolSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Skill
    }

    async fn list(&self, _scope: &RunToolScope) -> Vec<SourcedTool> {
        skills::definitions()
            .into_iter()
            .map(|def| sourced(def, SourceKind::Skill))
            .collect()
    }

    async fn call(
        &self,
        ctx: &ToolCtx<'_>,
        tool: &SourcedTool,
        args: Value,
    ) -> Result<ToolResult, ToolError> {
        skills::dispatch(&tool.key, ctx, args)
            .await
            .map(json_result)
    }
}
