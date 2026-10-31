use crate::domain::context_profile::ContextProfile;
use crate::mcp::catalog::object;
use crate::mcp::protocol::ToolDefinition;
use crate::mcp::source::{json_result, SourceKind, SourcedTool, ToolSource};
use crate::mcp::token::RunToolScope;
use crate::mcp::tools::{design, ToolCtx, ToolError};
use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

pub struct DesignToolSource {
    pub pool: PgPool,
}

const TOOLS: &[(&str, bool)] = &[
    ("design__create", false),
    ("design__write", false),
    ("design__publish", false),
    ("design__list", true),
];

#[async_trait]
impl ToolSource for DesignToolSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Plugin
    }

    async fn list(&self, scope: &RunToolScope) -> Vec<SourcedTool> {
        if scope.profile == ContextProfile::ConnectorCheck || scope.plugin_ids.is_empty() {
            return Vec::new();
        }
        let Ok(Some(id)) = design_plugin_id(&self.pool, &scope.plugin_ids).await else {
            return Vec::new();
        };
        TOOLS
            .iter()
            .map(|(name, read_only)| SourcedTool {
                def: ToolDefinition {
                    name: (*name).to_string(),
                    description: description(name).to_string(),
                    input_schema: input_schema(name),
                    read_only: *read_only,
                },
                source: SourceKind::Plugin,
                plugin_id: Some(id),
                key: (*name).to_string(),
            })
            .collect()
    }

    async fn call(
        &self,
        ctx: &ToolCtx<'_>,
        tool: &SourcedTool,
        args: Value,
    ) -> Result<crate::mcp::protocol::ToolResult, ToolError> {
        match (ctx.scope.ticket_id, ctx.scope.chat_session_id) {
            (Some(_), None) | (None, Some(_)) => {}
            _ => {
                return Err(ToolError::InvalidArgs(
                    "design set needs a ticket or a chat, not both".into(),
                ));
            }
        }
        if design_plugin_id(&self.pool, &ctx.scope.plugin_ids)
            .await?
            .is_none()
        {
            return Err(ToolError::Denied("design plugin is off".into()));
        }
        design::dispatch(&tool.key, ctx, args)
            .await
            .map(json_result)
    }
}

async fn design_plugin_id(pool: &PgPool, plugin_ids: &[Uuid]) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM plugins WHERE name = 'design' AND enabled AND status = 'ok' AND id = ANY($1)",
    )
    .bind(plugin_ids)
    .fetch_optional(pool)
    .await
}

fn description(name: &str) -> &'static str {
    match name {
        "design__create" => {
            "Create a design set and return its id and absolute folder under artifacts."
        }
        "design__write" => "Write a utf-8 file under that design set's folder.",
        "design__publish" => {
            "Publish 2 to 4 HTML options for a design set on this run, marking the one you recommend."
        }
        "design__list" => {
            "List published design sets for this ticket or chat, newest first, with the chosen, recommended and selected option."
        }
        _ => "",
    }
}

fn input_schema(name: &str) -> Value {
    match name {
        "design__create" => object(json!({ "title": { "type": "string" } }), &["title"]),
        "design__write" => object(
            json!({
                "setId": { "type": "string" },
                "path": { "type": "string" },
                "content": { "type": "string" },
            }),
            &["setId", "path", "content"],
        ),
        "design__publish" => object(
            json!({
                "setId": { "type": "string" },
                "recommendedOptionId": { "type": "string" },
                "options": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "title": { "type": "string" },
                            "entry": { "type": "string" },
                        },
                        "required": ["id", "title", "entry"],
                    },
                },
            }),
            &["setId", "options", "recommendedOptionId"],
        ),
        _ => object(json!({}), &[]),
    }
}
