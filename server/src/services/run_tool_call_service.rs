use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunToolCallRow {
    pub id: Uuid,
    pub tool: String,
    pub source: String,
    pub plugin_id: Option<Uuid>,
    pub plugin_name: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub duration_ms: i32,
    pub args_summary: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct RunToolCalls {
    pub items: Vec<RunToolCallRow>,
    pub skills_used: Vec<String>,
}

/// Read side of the per-run tool-call log written by `mcp::registry`.
pub struct RunToolCallService<'a> {
    pool: &'a PgPool,
}

impl<'a> RunToolCallService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn run_exists(&self, run_id: Uuid) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_runs WHERE id = $1)")
            .bind(run_id)
            .fetch_one(self.pool)
            .await
    }

    pub async fn list_for_run(&self, run_id: Uuid) -> Result<RunToolCalls, sqlx::Error> {
        let items: Vec<RunToolCallRow> = sqlx::query_as(
            r#"
            SELECT c.id, c.tool, c.source, c.plugin_id, p.name AS plugin_name,
                   c.status, c.error, c.duration_ms, c.args_summary, c.created_at
            FROM run_tool_calls c
            LEFT JOIN plugins p ON p.id = c.plugin_id
            WHERE c.run_id = $1
            ORDER BY c.created_at, c.id
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool)
        .await?;
        let skills_used = skills_used(&items);
        Ok(RunToolCalls { items, skills_used })
    }
}

/// Distinct `name` of successful `skill_load` calls in first-use order.
/// `args_summary` may be truncated JSON; such rows are skipped.
fn skills_used(items: &[RunToolCallRow]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for item in items {
        if item.tool != "skill_load" || item.status != "ok" {
            continue;
        }
        let Ok(args) = serde_json::from_str::<Value>(&item.args_summary) else {
            continue;
        };
        if let Some(name) = args.get("name").and_then(Value::as_str) {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(tool: &str, status: &str, args_summary: &str) -> RunToolCallRow {
        RunToolCallRow {
            id: Uuid::new_v4(),
            tool: tool.into(),
            source: "skill".into(),
            plugin_id: None,
            plugin_name: None,
            status: status.into(),
            error: None,
            duration_ms: 1,
            args_summary: args_summary.into(),
            created_at: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn skills_used_distinct_ok_loads_in_first_use_order() {
        let items = vec![
            row("skill_list", "ok", "{}"),
            row("skill_load", "ok", r#"{"name":"b"}"#),
            row("skill_load", "error", r#"{"name":"x"}"#),
            row("skill_load", "ok", r#"{"name":"cut"#),
            row("skill_load", "ok", r#"{"name":7}"#),
            row("skill_load", "ok", r#"{"name":"a"}"#),
            row("skill_load", "ok", r#"{"name":"b"}"#),
        ];
        assert_eq!(skills_used(&items), vec!["b", "a"]);
    }
}
