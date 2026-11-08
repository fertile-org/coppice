//! Stores run usage snapshots and answers usage questions for runs and tickets.
//!
//! Works only with normalized usage (`domain::usage`); it never looks at which
//! connector produced it. A run is tracked when it has a `run_usage` row: the
//! stream handle stores one as soon as a reader is attached.

use crate::domain::usage::{
    Granularity, RequestUsage, SubagentUsage, TokenCounts, UsageSnapshot, UsageStatus,
};
use crate::sessions::run_registry::RunStreamHandle;
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

pub struct UsageService<'a> {
    pool: &'a PgPool,
}

/// A run's stored totals. Cost is stored but not served.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunUsage {
    pub status: UsageStatus,
    pub tokens: TokenCounts,
    /// Partial totals only: subagent tokens without a breakdown, on top of `tokens`.
    pub subagent_tokens: Option<i64>,
    pub request_count: i64,
    pub subagent_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunUsageDetail {
    /// `None` when Coppice could not read the run's usage; status `none` when
    /// it could but nothing was reported.
    pub totals: Option<RunUsage>,
    pub requests: Vec<RequestUsage>,
    pub subagents: Vec<SubagentUsage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketRunUsage {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub job_type: String,
    /// The run is no longer queued or running.
    #[serde(skip)]
    pub finished: bool,
    /// `None` when Coppice could not read the run's usage.
    pub usage: Option<RunUsage>,
}

/// Tracked runs grouped by one key (agent id or job type). Runs Coppice could
/// not read are left out entirely.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageGroup {
    pub key: String,
    pub runs: i64,
    /// Runs that reported any usage; only these are in `tokens`.
    pub runs_with_usage: i64,
    /// Finished runs that reported nothing.
    pub runs_not_reported: i64,
    /// Finished runs whose total is only the sum of what they streamed.
    pub partial_runs: i64,
    pub tokens: TokenCounts,
    /// Subagent tokens without a breakdown (partial runs), on top of `tokens`.
    pub subagent_tokens: Option<i64>,
}

impl UsageGroup {
    fn add(&mut self, run: &TicketRunUsage, usage: &RunUsage) {
        self.runs += 1;
        match usage.status {
            UsageStatus::None => {
                if run.finished {
                    self.runs_not_reported += 1;
                }
            }
            UsageStatus::Reported | UsageStatus::Partial => {
                self.runs_with_usage += 1;
                self.tokens.add(&usage.tokens);
                self.subagent_tokens = add_opt(self.subagent_tokens, usage.subagent_tokens);
                if usage.status == UsageStatus::Partial && run.finished {
                    self.partial_runs += 1;
                }
            }
        }
    }

    fn total(&self) -> i64 {
        self.tokens.total().unwrap_or(0) + self.subagent_tokens.unwrap_or(0)
    }
}

fn add_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubticketUsage {
    pub ticket_count: i64,
    pub runs_with_usage: i64,
    /// Finished runs whose total is only the sum of what they streamed.
    pub partial_runs: i64,
    pub tokens: TokenCounts,
    pub subagent_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketUsage {
    pub runs: Vec<TicketRunUsage>,
    /// This ticket's own runs.
    pub own: UsageGroup,
    /// Split children, recursively. `None` when the ticket has none.
    pub subtickets: Option<SubticketUsage>,
    pub by_agent: Vec<UsageGroup>,
    pub by_job_type: Vec<UsageGroup>,
}

impl<'a> UsageService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Writes a snapshot unless a newer one is already stored, upserting only
    /// the rows that changed since the stored version. Returns whether it was
    /// written.
    pub async fn save_snapshot(
        &self,
        run_id: Uuid,
        snapshot: &UsageSnapshot,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        // Locks the row, so a concurrent writer waits and then sees this version.
        // With no row yet, a concurrent first insert is still settled by the
        // version guard below; missing it here only writes more rows than needed.
        let stored: Option<i64> =
            sqlx::query_scalar("SELECT version FROM run_usage WHERE run_id = $1 FOR UPDATE")
                .bind(run_id)
                .fetch_optional(&mut *tx)
                .await?;
        // Version 0 (tracked, nothing reported) is written only when no row exists.
        if stored.is_some_and(|stored| stored >= snapshot.version) {
            tx.rollback().await?;
            return Ok(false);
        }
        let stored = stored.unwrap_or(0);
        let totals = &snapshot.totals;
        let written = sqlx::query(
            r#"
            INSERT INTO run_usage (
                run_id, status, version,
                input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, reasoning_tokens,
                subagent_tokens, request_count, subagent_count, reported_cost_usd, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12::float8, now())
            ON CONFLICT (run_id) DO UPDATE SET
                status = EXCLUDED.status,
                version = EXCLUDED.version,
                input_tokens = EXCLUDED.input_tokens,
                cache_read_tokens = EXCLUDED.cache_read_tokens,
                cache_write_tokens = EXCLUDED.cache_write_tokens,
                output_tokens = EXCLUDED.output_tokens,
                reasoning_tokens = EXCLUDED.reasoning_tokens,
                subagent_tokens = EXCLUDED.subagent_tokens,
                request_count = EXCLUDED.request_count,
                subagent_count = EXCLUDED.subagent_count,
                reported_cost_usd = EXCLUDED.reported_cost_usd,
                updated_at = now()
            WHERE run_usage.version < EXCLUDED.version
            "#,
        )
        .bind(run_id)
        .bind(totals.status.as_str())
        .bind(snapshot.version)
        .bind(totals.tokens.input)
        .bind(totals.tokens.cache_read)
        .bind(totals.tokens.cache_write)
        .bind(totals.tokens.output)
        .bind(totals.tokens.reasoning)
        .bind(totals.subagent_tokens)
        .bind(totals.request_count as i32)
        .bind(totals.subagent_count as i32)
        .bind(totals.cost_usd)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            > 0;
        if !written {
            tx.rollback().await?;
            return Ok(false);
        }

        let changed = |versions: &[i64]| -> Vec<usize> {
            (0..versions.len())
                .filter(|&i| versions[i] > stored)
                .collect()
        };

        let seqs = changed(&snapshot.request_versions);
        if !seqs.is_empty() {
            let rows: Vec<_> = seqs.iter().map(|&i| &snapshot.requests[i]).collect();
            sqlx::query(
                r#"
                INSERT INTO run_llm_requests (
                    run_id, seq, external_id, granularity, model,
                    input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, reasoning_tokens
                )
                SELECT $1::uuid, r.*
                FROM UNNEST(
                    $2::int4[], $3::text[], $4::text[], $5::text[],
                    $6::int8[], $7::int8[], $8::int8[], $9::int8[], $10::int8[]
                ) AS r
                ON CONFLICT (run_id, seq) DO UPDATE SET
                    external_id = EXCLUDED.external_id,
                    granularity = EXCLUDED.granularity,
                    model = EXCLUDED.model,
                    input_tokens = EXCLUDED.input_tokens,
                    cache_read_tokens = EXCLUDED.cache_read_tokens,
                    cache_write_tokens = EXCLUDED.cache_write_tokens,
                    output_tokens = EXCLUDED.output_tokens,
                    reasoning_tokens = EXCLUDED.reasoning_tokens
                "#,
            )
            .bind(run_id)
            .bind(seqs.iter().map(|&i| i as i32).collect::<Vec<_>>())
            .bind(rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>())
            .bind(rows.iter().map(|r| r.granularity.as_str()).collect::<Vec<_>>())
            .bind(rows.iter().map(|r| r.model.clone()).collect::<Vec<_>>())
            .bind(rows.iter().map(|r| r.tokens.input).collect::<Vec<_>>())
            .bind(rows.iter().map(|r| r.tokens.cache_read).collect::<Vec<_>>())
            .bind(rows.iter().map(|r| r.tokens.cache_write).collect::<Vec<_>>())
            .bind(rows.iter().map(|r| r.tokens.output).collect::<Vec<_>>())
            .bind(rows.iter().map(|r| r.tokens.reasoning).collect::<Vec<_>>())
            .execute(&mut *tx)
            .await?;
        }

        let seqs = changed(&snapshot.subagent_versions);
        if !seqs.is_empty() {
            let rows: Vec<_> = seqs.iter().map(|&i| &snapshot.subagents[i]).collect();
            sqlx::query(
                r#"
                INSERT INTO run_subagents (
                    run_id, seq, external_id, agent_type, description, model, status,
                    input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, reasoning_tokens,
                    total_tokens, tool_uses, duration_ms
                )
                SELECT $1::uuid, s.*
                FROM UNNEST(
                    $2::int4[], $3::text[], $4::text[], $5::text[], $6::text[], $7::text[],
                    $8::int8[], $9::int8[], $10::int8[], $11::int8[], $12::int8[],
                    $13::int8[], $14::int4[], $15::int8[]
                ) AS s
                ON CONFLICT (run_id, seq) DO UPDATE SET
                    external_id = EXCLUDED.external_id,
                    agent_type = EXCLUDED.agent_type,
                    description = EXCLUDED.description,
                    model = EXCLUDED.model,
                    status = EXCLUDED.status,
                    input_tokens = EXCLUDED.input_tokens,
                    cache_read_tokens = EXCLUDED.cache_read_tokens,
                    cache_write_tokens = EXCLUDED.cache_write_tokens,
                    output_tokens = EXCLUDED.output_tokens,
                    reasoning_tokens = EXCLUDED.reasoning_tokens,
                    total_tokens = EXCLUDED.total_tokens,
                    tool_uses = EXCLUDED.tool_uses,
                    duration_ms = EXCLUDED.duration_ms
                "#,
            )
            .bind(run_id)
            .bind(seqs.iter().map(|&i| i as i32).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.id.clone()).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.agent_type.clone()).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.description.clone()).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.model.clone()).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.status.clone()).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.tokens.input).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.tokens.cache_read).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.tokens.cache_write).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.tokens.output).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.tokens.reasoning).collect::<Vec<_>>())
            .bind(rows.iter().map(|s| s.total_tokens).collect::<Vec<_>>())
            .bind(
                rows.iter()
                    .map(|s| s.tool_uses.map(|n| n as i32))
                    .collect::<Vec<_>>(),
            )
            .bind(rows.iter().map(|s| s.duration_ms).collect::<Vec<_>>())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(true)
    }

    pub async fn run_exists(&self, run_id: Uuid) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM agent_runs WHERE id = $1)")
            .bind(run_id)
            .fetch_one(self.pool)
            .await
    }

    pub async fn ticket_exists(&self, ticket_id: Uuid) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tickets WHERE id = $1)")
            .bind(ticket_id)
            .fetch_one(self.pool)
            .await
    }

    pub async fn run_usage(&self, run_id: Uuid) -> Result<RunUsageDetail, sqlx::Error> {
        let totals = sqlx::query(
            r#"
            SELECT status, input_tokens, cache_read_tokens, cache_write_tokens,
                   output_tokens, reasoning_tokens, subagent_tokens, request_count, subagent_count
            FROM run_usage WHERE run_id = $1
            "#,
        )
        .bind(run_id)
        .fetch_optional(self.pool)
        .await?
        .map(|row| run_usage_from_row(&row, "status"))
        .transpose()?
        .flatten();

        let requests = sqlx::query(
            r#"
            SELECT external_id, granularity, model, input_tokens, cache_read_tokens,
                   cache_write_tokens, output_tokens, reasoning_tokens
            FROM run_llm_requests WHERE run_id = $1 ORDER BY seq
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool)
        .await?
        .iter()
        .map(|row| -> Result<RequestUsage, sqlx::Error> {
            let granularity: String = row.try_get("granularity")?;
            Ok(RequestUsage {
                id: row.try_get("external_id")?,
                granularity: if granularity == "turn" {
                    Granularity::Turn
                } else {
                    Granularity::Request
                },
                model: row.try_get("model")?,
                tokens: tokens_from_row(row)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

        let subagents = sqlx::query(
            r#"
            SELECT external_id, agent_type, description, model, status,
                   input_tokens, cache_read_tokens, cache_write_tokens, output_tokens,
                   reasoning_tokens, total_tokens, tool_uses, duration_ms
            FROM run_subagents WHERE run_id = $1 ORDER BY seq
            "#,
        )
        .bind(run_id)
        .fetch_all(self.pool)
        .await?
        .iter()
        .map(|row| -> Result<SubagentUsage, sqlx::Error> {
            Ok(SubagentUsage {
                id: row.try_get("external_id")?,
                agent_type: row.try_get("agent_type")?,
                description: row.try_get("description")?,
                model: row.try_get("model")?,
                status: row.try_get("status")?,
                tokens: tokens_from_row(row)?,
                total_tokens: row.try_get("total_tokens")?,
                tool_uses: row.try_get::<Option<i32>, _>("tool_uses")?.map(i64::from),
                duration_ms: row.try_get("duration_ms")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

        Ok(RunUsageDetail {
            totals,
            requests,
            subagents,
        })
    }

    /// Usage for a ticket's own runs, grouped by agent and job type, plus its
    /// split children. Runs without a ticket (knowledge compaction, connector
    /// checks) are never part of a ticket's total.
    pub async fn ticket_usage(&self, ticket_id: Uuid) -> Result<TicketUsage, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT ar.id, ar.agent_id, ar.job_type,
                   ar.status NOT IN ('queued', 'running') AS finished,
                   u.status AS usage_status, u.input_tokens, u.cache_read_tokens,
                   u.cache_write_tokens, u.output_tokens, u.reasoning_tokens,
                   u.subagent_tokens, u.request_count, u.subagent_count
            FROM agent_runs ar
            LEFT JOIN run_usage u ON u.run_id = ar.id
            WHERE ar.ticket_id = $1
            ORDER BY ar.created_at DESC
            "#,
        )
        .bind(ticket_id)
        .fetch_all(self.pool)
        .await?;

        let mut runs = Vec::with_capacity(rows.len());
        for row in &rows {
            runs.push(TicketRunUsage {
                run_id: row.try_get("id")?,
                agent_id: row.try_get("agent_id")?,
                job_type: row.try_get("job_type")?,
                finished: row.try_get("finished")?,
                usage: run_usage_from_row(row, "usage_status")?,
            });
        }

        let subtickets = sqlx::query(
            r#"
            WITH RECURSIVE tree AS (
                SELECT id FROM tickets WHERE parent_ticket_id = $1
                UNION
                SELECT t.id FROM tickets t JOIN tree ON t.parent_ticket_id = tree.id
            )
            SELECT
                (SELECT count(*) FROM tree) AS ticket_count,
                count(*) FILTER (WHERE u.status <> 'none') AS runs_with_usage,
                count(*) FILTER (
                    WHERE u.status = 'partial' AND ar.status NOT IN ('queued', 'running')
                ) AS partial_runs,
                sum(u.input_tokens)::int8 AS input_tokens,
                sum(u.cache_read_tokens)::int8 AS cache_read_tokens,
                sum(u.cache_write_tokens)::int8 AS cache_write_tokens,
                sum(u.output_tokens)::int8 AS output_tokens,
                sum(u.reasoning_tokens)::int8 AS reasoning_tokens,
                sum(u.subagent_tokens)::int8 AS subagent_tokens
            FROM tree
            JOIN agent_runs ar ON ar.ticket_id = tree.id
            JOIN run_usage u ON u.run_id = ar.id
            "#,
        )
        .bind(ticket_id)
        .fetch_one(self.pool)
        .await?;
        let ticket_count: i64 = subtickets.try_get("ticket_count")?;
        let subtickets = (ticket_count > 0)
            .then(|| -> Result<SubticketUsage, sqlx::Error> {
                Ok(SubticketUsage {
                    ticket_count,
                    runs_with_usage: subtickets.try_get("runs_with_usage")?,
                    partial_runs: subtickets.try_get("partial_runs")?,
                    tokens: tokens_from_row(&subtickets)?,
                    subagent_tokens: subtickets.try_get("subagent_tokens")?,
                })
            })
            .transpose()?;

        Ok(rollup(runs, subtickets))
    }
}

/// Groups a ticket's runs. Pure, so the counting rules are unit-tested.
fn rollup(runs: Vec<TicketRunUsage>, subtickets: Option<SubticketUsage>) -> TicketUsage {
    let mut own = UsageGroup {
        key: "ticket".into(),
        ..UsageGroup::default()
    };
    let mut by_agent: BTreeMap<String, UsageGroup> = BTreeMap::new();
    let mut by_job_type: BTreeMap<String, UsageGroup> = BTreeMap::new();
    for run in &runs {
        // Runs Coppice could not read say nothing about tokens.
        let Some(usage) = &run.usage else {
            continue;
        };
        for group in [
            &mut own,
            by_agent
                .entry(run.agent_id.to_string())
                .or_insert_with_key(|key| UsageGroup {
                    key: key.clone(),
                    ..UsageGroup::default()
                }),
            by_job_type
                .entry(run.job_type.clone())
                .or_insert_with_key(|key| UsageGroup {
                    key: key.clone(),
                    ..UsageGroup::default()
                }),
        ] {
            group.add(run, usage);
        }
    }
    let by_total = |groups: BTreeMap<String, UsageGroup>| {
        let mut groups: Vec<_> = groups.into_values().collect();
        groups.sort_by_key(|g| std::cmp::Reverse(g.total()));
        groups
    };
    TicketUsage {
        runs,
        own,
        subtickets,
        by_agent: by_total(by_agent),
        by_job_type: by_total(by_job_type),
    }
}

fn tokens_from_row(row: &sqlx::postgres::PgRow) -> Result<TokenCounts, sqlx::Error> {
    Ok(TokenCounts {
        input: row.try_get("input_tokens")?,
        cache_read: row.try_get("cache_read_tokens")?,
        cache_write: row.try_get("cache_write_tokens")?,
        output: row.try_get("output_tokens")?,
        reasoning: row.try_get("reasoning_tokens")?,
    })
}

/// `None` when the status column is NULL (no usage row joined).
fn run_usage_from_row(
    row: &sqlx::postgres::PgRow,
    status_column: &str,
) -> Result<Option<RunUsage>, sqlx::Error> {
    let status: Option<String> = row.try_get(status_column)?;
    let Some(status) = status.as_deref().and_then(UsageStatus::parse) else {
        return Ok(None);
    };
    Ok(Some(RunUsage {
        status,
        tokens: tokens_from_row(row)?,
        subagent_tokens: row.try_get("subagent_tokens")?,
        request_count: row.try_get::<i32, _>("request_count")?.into(),
        subagent_count: row.try_get::<i32, _>("subagent_count")?.into(),
    }))
}

/// Writes the run's usage as the connector reports it, so a running row shows
/// it before the process exits. Holds no stream handle: it ends when the
/// handle is dropped.
pub fn spawn_usage_writer(
    pool: PgPool,
    run_id: Uuid,
    stream: &RunStreamHandle,
) -> tokio::task::JoinHandle<()> {
    let mut rx = stream.subscribe_usage();
    tokio::spawn(async move {
        while rx.changed().await.is_ok() {
            let snapshot = rx.borrow_and_update().clone();
            if let Some(snapshot) = snapshot {
                save_or_warn(&pool, run_id, &snapshot).await;
            }
        }
    })
}

/// Persist the latest usage. Safe to call more than once and alongside the writer.
/// Returns true only when a snapshot existed and the save succeeded, so callers
/// can announce it.
pub async fn record_usage(pool: &PgPool, run_id: Uuid, stream: &RunStreamHandle) -> bool {
    match stream.usage_snapshot() {
        Some(snapshot) => save_or_warn(pool, run_id, &snapshot).await,
        None => false,
    }
}

async fn save_or_warn(pool: &PgPool, run_id: Uuid, snapshot: &UsageSnapshot) -> bool {
    match UsageService::new(pool)
        .save_snapshot(run_id, snapshot)
        .await
    {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!(%run_id, error = %err, "failed to save run usage");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A finished run; `usage` `None` means Coppice could not read it.
    fn run(agent: Uuid, job_type: &str, usage: Option<RunUsage>) -> TicketRunUsage {
        TicketRunUsage {
            run_id: Uuid::new_v4(),
            agent_id: agent,
            job_type: job_type.into(),
            finished: true,
            usage,
        }
    }

    fn usage(status: UsageStatus, output: Option<i64>, subagent_tokens: Option<i64>) -> RunUsage {
        RunUsage {
            status,
            tokens: TokenCounts {
                output,
                ..TokenCounts::default()
            },
            subagent_tokens,
            request_count: 1,
            subagent_count: 0,
        }
    }

    fn reported(output: i64) -> Option<RunUsage> {
        Some(usage(UsageStatus::Reported, Some(output), None))
    }

    fn nothing() -> Option<RunUsage> {
        Some(usage(UsageStatus::None, None, None))
    }

    #[test]
    fn rollup_groups_by_agent_and_job_type() {
        let worker = Uuid::new_v4();
        let planner = Uuid::new_v4();
        let untracked_agent = Uuid::new_v4();
        let usage = rollup(
            vec![
                run(worker, "work_on_ticket", reported(100)),
                run(worker, "work_on_ticket", nothing()),
                run(worker, "respond_to_mention", reported(10)),
                run(planner, "plan_ticket", reported(40)),
                // A connector without a reader, or a run from before tracking.
                run(untracked_agent, "work_on_ticket", None),
            ],
            None,
        );
        assert_eq!(usage.own.runs, 4);
        assert_eq!(usage.own.runs_with_usage, 3);
        assert_eq!(usage.own.runs_not_reported, 1);
        assert_eq!(usage.own.tokens.output, Some(150));
        assert!(usage
            .by_agent
            .iter()
            .all(|group| group.key != untracked_agent.to_string()));
        // Largest first.
        assert_eq!(usage.by_agent[0].key, worker.to_string());
        assert_eq!(usage.by_agent[0].runs, 3);
        assert_eq!(usage.by_agent[0].tokens.output, Some(110));
        assert_eq!(usage.by_agent[1].tokens.output, Some(40));
        let keys: Vec<_> = usage.by_job_type.iter().map(|g| g.key.as_str()).collect();
        assert_eq!(
            keys,
            ["work_on_ticket", "plan_ticket", "respond_to_mention"]
        );
    }

    mod db {
        use super::super::*;
        use crate::domain::usage::{RunTotalUsage, UsageEvent, UsageTracker};

        async fn pool() -> Option<PgPool> {
            crate::db::shared_test_pool().await.ok()
        }

        async fn insert_ticket(
            pool: &PgPool,
            board: Uuid,
            agent: Uuid,
            parent: Option<Uuid>,
        ) -> Uuid {
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO tickets (id, board_id, title, status, created_by, assignee_agent_id, parent_ticket_id)
                 VALUES ($1, $2, 'usage test', 'todo', 'test', $3, $4)",
            )
            .bind(id)
            .bind(board)
            .bind(agent)
            .bind(parent)
            .execute(pool)
            .await
            .expect("insert ticket");
            id
        }

        async fn insert_run(pool: &PgPool, ticket: Uuid, agent: Uuid) -> Uuid {
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO agent_runs (id, ticket_id, agent_id, job_type, status, sandbox_profile_id)
                 VALUES ($1, $2, $3, 'work_on_ticket', 'succeeded', 'default')",
            )
            .bind(id)
            .bind(ticket)
            .bind(agent)
            .execute(pool)
            .await
            .expect("insert run");
            id
        }

        /// Board, agent, a parent ticket with a child and a grandchild.
        async fn tree(pool: &PgPool) -> (Uuid, [Uuid; 3]) {
            let board = Uuid::new_v4();
            sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, 'usage', $2)")
                .bind(board)
                .bind(format!("usage-{board}"))
                .execute(pool)
                .await
                .expect("insert board");
            let agent = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO agents (id, name, role, responsibilities, system_prompt, connector)
                 VALUES ($1, 'usage agent', 'worker', '{}', 'prompt', 'mock')",
            )
            .bind(agent)
            .execute(pool)
            .await
            .expect("insert agent");
            let parent = insert_ticket(pool, board, agent, None).await;
            let child = insert_ticket(pool, board, agent, Some(parent)).await;
            let grandchild = insert_ticket(pool, board, agent, Some(child)).await;
            (agent, [parent, child, grandchild])
        }

        fn snapshot(events: Vec<UsageEvent>) -> UsageSnapshot {
            let mut tracker = UsageTracker::new();
            for event in events {
                tracker.apply(event);
            }
            tracker.snapshot()
        }

        fn output(n: i64) -> TokenCounts {
            TokenCounts {
                output: Some(n),
                ..TokenCounts::default()
            }
        }

        #[tokio::test]
        async fn stale_snapshot_never_replaces_a_newer_one() {
            let Some(pool) = pool().await else { return };
            let (agent, [ticket, ..]) = tree(&pool).await;
            let run = insert_run(&pool, ticket, agent).await;
            let service = UsageService::new(&pool);

            let older = snapshot(vec![UsageEvent::Request(RequestUsage {
                id: Some("m1".into()),
                tokens: output(5),
                ..RequestUsage::default()
            })]);
            let newer = snapshot(vec![
                UsageEvent::Request(RequestUsage {
                    id: Some("m1".into()),
                    tokens: output(5),
                    ..RequestUsage::default()
                }),
                UsageEvent::Subagent(SubagentUsage {
                    id: "t1".into(),
                    tool_uses: Some(2),
                    tokens: output(7),
                    ..SubagentUsage::default()
                }),
                UsageEvent::RunTotal(RunTotalUsage {
                    tokens: output(12),
                    cost_usd: Some(0.01),
                }),
            ]);
            // Tracked before anything was reported: version 0, written once.
            let tracked = UsageTracker::new().snapshot();
            assert!(service.save_snapshot(run, &tracked).await.unwrap());
            assert!(!service.save_snapshot(run, &tracked).await.unwrap());
            let totals = service
                .run_usage(run)
                .await
                .unwrap()
                .totals
                .expect("tracked");
            assert_eq!(totals.status, UsageStatus::None);

            assert!(service.save_snapshot(run, &newer).await.unwrap());
            assert!(!service.save_snapshot(run, &older).await.unwrap());
            assert!(!service.save_snapshot(run, &tracked).await.unwrap());

            let detail = service.run_usage(run).await.unwrap();
            let totals = detail.totals.expect("totals");
            assert_eq!(totals.status, UsageStatus::Reported);
            assert_eq!(totals.tokens.output, Some(12));
            assert_eq!(totals.subagent_count, 1);
            assert_eq!(detail.requests.len(), 1);
            assert_eq!(detail.requests[0].id.as_deref(), Some("m1"));
            assert_eq!(detail.subagents[0].tool_uses, Some(2));
        }

        #[tokio::test]
        async fn later_snapshots_write_only_changed_rows() {
            let Some(pool) = pool().await else { return };
            let (agent, [ticket, ..]) = tree(&pool).await;
            let run = insert_run(&pool, ticket, agent).await;
            let service = UsageService::new(&pool);
            let mut tracker = UsageTracker::new();
            let request = |id: &str, n: i64| {
                UsageEvent::Request(RequestUsage {
                    id: Some(id.into()),
                    tokens: output(n),
                    ..RequestUsage::default()
                })
            };

            tracker.apply(request("m1", 1));
            tracker.apply(request("m2", 2));
            assert!(service
                .save_snapshot(run, &tracker.snapshot())
                .await
                .unwrap());
            // Only m2 changes; m1 is not rewritten but stays stored.
            tracker.apply(request("m2", 20));
            tracker.apply(request("m3", 3));
            let snap = tracker.snapshot();
            assert_eq!(snap.request_versions, [1, 3, 4]);
            assert!(service.save_snapshot(run, &snap).await.unwrap());

            let outputs: Vec<_> = service
                .run_usage(run)
                .await
                .unwrap()
                .requests
                .iter()
                .map(|r| r.tokens.output)
                .collect();
            assert_eq!(outputs, [Some(1), Some(20), Some(3)]);
        }

        #[tokio::test]
        async fn ticket_usage_adds_subtickets_separately() {
            let Some(pool) = pool().await else { return };
            let (agent, [parent, child, grandchild]) = tree(&pool).await;
            let service = UsageService::new(&pool);
            for (ticket, n) in [(parent, 10), (child, 20), (grandchild, 30)] {
                let run = insert_run(&pool, ticket, agent).await;
                let snap = snapshot(vec![UsageEvent::RunTotal(RunTotalUsage {
                    tokens: output(n),
                    cost_usd: None,
                })]);
                service.save_snapshot(run, &snap).await.unwrap();
            }
            // Tracked but reported nothing: counted as a run, not in the tokens.
            let silent = insert_run(&pool, parent, agent).await;
            service
                .save_snapshot(silent, &UsageTracker::new().snapshot())
                .await
                .unwrap();
            // Untracked (no row): left out.
            insert_run(&pool, parent, agent).await;
            // A child run that ended before the CLI's total.
            let cut_short = insert_run(&pool, child, agent).await;
            let snap = snapshot(vec![
                UsageEvent::Request(RequestUsage {
                    tokens: output(4),
                    ..RequestUsage::default()
                }),
                UsageEvent::Subagent(SubagentUsage {
                    id: "t1".into(),
                    total_tokens: Some(300),
                    ..SubagentUsage::default()
                }),
            ]);
            service.save_snapshot(cut_short, &snap).await.unwrap();

            let usage = service.ticket_usage(parent).await.unwrap();
            assert_eq!(usage.runs.len(), 3);
            assert_eq!(usage.own.runs, 2);
            assert_eq!(usage.own.runs_with_usage, 1);
            assert_eq!(usage.own.runs_not_reported, 1);
            assert_eq!(usage.own.tokens.output, Some(10));
            let subtickets = usage.subtickets.expect("subtickets");
            assert_eq!(subtickets.ticket_count, 2);
            assert_eq!(subtickets.runs_with_usage, 3);
            assert_eq!(subtickets.partial_runs, 1);
            assert_eq!(subtickets.tokens.output, Some(54));
            assert_eq!(subtickets.subagent_tokens, Some(300));

            let leaf = service.ticket_usage(grandchild).await.unwrap();
            assert!(leaf.subtickets.is_none());
        }
    }

    #[test]
    fn rollup_leaves_untracked_runs_out() {
        let usage = rollup(vec![run(Uuid::new_v4(), "work_on_ticket", None)], None);
        assert_eq!(usage.own.runs, 0);
        assert_eq!(usage.own.runs_not_reported, 0);
        assert_eq!(usage.own.tokens.total(), None);
        assert!(usage.by_agent.is_empty());
        assert_eq!(usage.runs.len(), 1);
    }

    #[test]
    fn rollup_counts_partial_runs_and_skips_active_ones() {
        let agent = Uuid::new_v4();
        let active = |usage| TicketRunUsage {
            finished: false,
            ..run(agent, "work_on_ticket", usage)
        };
        let usage = rollup(
            vec![
                run(
                    agent,
                    "work_on_ticket",
                    Some(usage(UsageStatus::Partial, Some(5), Some(200))),
                ),
                // Still running: neither "not reported" nor "partial" yet.
                active(nothing()),
                active(Some(usage(UsageStatus::Partial, Some(1), None))),
            ],
            None,
        );
        let own = &usage.own;
        assert_eq!(own.runs, 3);
        assert_eq!(own.runs_with_usage, 2);
        assert_eq!(own.runs_not_reported, 0);
        assert_eq!(own.partial_runs, 1);
        assert_eq!(own.tokens.output, Some(6));
        assert_eq!(own.subagent_tokens, Some(200));
        assert_eq!(own.total(), 206);
    }

    #[tokio::test]
    async fn writers_end_when_the_handle_is_dropped_with_nothing_reported() {
        use crate::sessions::run_registry::RunStreamRegistry;
        use std::time::Duration;

        // Lazy: nothing is written, so the pool never connects.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://nobody:secret@127.0.0.1:1/unreachable")
            .unwrap();
        let registry = RunStreamRegistry::new();
        let run_id = Uuid::new_v4();
        let handle = registry.register(run_id);
        let writers = [
            crate::services::run_service::spawn_reported_model_writer(
                pool.clone(),
                run_id,
                &handle,
            ),
            spawn_usage_writer(pool, run_id, &handle),
        ];
        registry.remove(run_id);
        drop(handle);
        for writer in writers {
            tokio::time::timeout(Duration::from_secs(1), writer)
                .await
                .expect("writer ends once the handle is gone")
                .unwrap();
        }
    }
}
