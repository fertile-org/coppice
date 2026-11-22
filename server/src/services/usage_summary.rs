//! Workspace token usage for Settings → Usage, read from the `usage_slices`
//! view. Slices add up to each run's total whatever a connector reports (per
//! request, per turn or only a total), so every grouping adds up to the same
//! headline.
use crate::domain::Id;

use crate::domain::connector_check::JOB_TYPE_CONNECTOR_CHECK;
use crate::domain::knowledge_compaction::JOB_TYPE_COMPACT_KNOWLEDGE;
use crate::domain::usage::TokenCounts;
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
/// What the table groups slices by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    /// The model, together with the connector that reported it.
    Model,
    Agent,
    /// Runs without a board group under `workspace`.
    Board,
    Connector,
    /// Compaction and connector checks group under `workspace_overhead`.
    JobType,
}

impl GroupBy {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "model" => Some(GroupBy::Model),
            "agent" => Some(GroupBy::Agent),
            "board" => Some(GroupBy::Board),
            "connector" => Some(GroupBy::Connector),
            "jobType" => Some(GroupBy::JobType),
            _ => None,
        }
    }

    /// `(key, label, connector)` SQL over `usage_slices s`, `agents a` and
    /// `boards b`. Static strings only: nothing here comes from a request.
    fn sql(self) -> (String, &'static str, &'static str) {
        match self {
            GroupBy::Model => (
                "COALESCE(s.connector, '') || '/' || COALESCE(s.model, '')".into(),
                "max(s.model)",
                "max(s.connector)",
            ),
            GroupBy::Agent => (
                "COALESCE(s.agent_id::text, 'deleted:' || COALESCE(s.agent_name, ''))".into(),
                "COALESCE(max(a.name), max(s.agent_name))",
                "NULL::text",
            ),
            GroupBy::Board => (
                "COALESCE(s.board_id::text, CASE WHEN s.board_name IS NULL THEN 'workspace' \
                 ELSE 'deleted:' || s.board_name END)"
                    .into(),
                "COALESCE(max(b.name), max(s.board_name))",
                "NULL::text",
            ),
            GroupBy::Connector => (
                "COALESCE(s.connector, 'unknown')".into(),
                "NULL::text",
                "max(s.connector)",
            ),
            GroupBy::JobType => (
                format!(
                    "CASE WHEN s.job_type IN ('{JOB_TYPE_COMPACT_KNOWLEDGE}', \
                     '{JOB_TYPE_CONNECTOR_CHECK}') THEN 'workspace_overhead' \
                     ELSE COALESCE(s.job_type, 'unknown') END"
                ),
                "NULL::text",
                "NULL::text",
            ),
        }
    }
}

/// A summary request. Dates are days in `tz`, both inclusive; `None` is open.
#[derive(Debug, Clone)]
pub struct SummaryQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    /// An IANA time zone Postgres knows (checked by [`timezone_exists`]).
    pub tz: String,
    pub group_by: GroupBy,
    pub board: Option<Id>,
    pub connector: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryTotals {
    pub tokens: TokenCounts,
    /// Subagent totals without a breakdown, on top of `tokens`.
    pub unsplit: i64,
    pub total: i64,
    /// Tracked runs with tokens in the range.
    pub runs: i64,
    /// Days in the range with any tokens.
    pub active_days: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryRow {
    pub key: String,
    /// A name for the key when the server knows one (model, agent, board,
    /// connector); `None` leaves the label to the page ("Workspace",
    /// "Model not reported", job type names).
    pub label: Option<String>,
    pub connector: Option<String>,
    pub tokens: TokenCounts,
    pub unsplit: i64,
    pub total: i64,
    pub runs: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryDay {
    /// `YYYY-MM-DD` in the query's time zone.
    pub day: String,
    pub by_key: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UntrackedConnector {
    pub connector: String,
    pub runs: i64,
}

/// Runs created in the range on a known connector: tracked ones have usage,
/// untracked ones ran on a connector Coppice cannot read.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryCoverage {
    pub tracked_runs: i64,
    pub untracked_runs: i64,
    pub untracked_by_connector: Vec<UntrackedConnector>,
}

/// Tokens per top-level ticket that reached Done in the range, each counting
/// its whole history and every subticket. Tickets without tracked usage are
/// left out.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerDoneTicket {
    pub tickets: i64,
    pub median: f64,
    pub average: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub totals: SummaryTotals,
    /// Largest first.
    pub rows: Vec<SummaryRow>,
    /// Days with tokens, oldest first.
    pub daily: Vec<SummaryDay>,
    pub coverage: SummaryCoverage,
    /// `None` when no such ticket has tracked usage.
    pub per_done_ticket: Option<PerDoneTicket>,
}

pub async fn timezone_exists(pool: &PgPool, tz: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1)")
        .bind(tz)
        .fetch_one(pool)
        .await
}

/// Legacy IANA names Chromium may send that host tzdata without `tzdata-legacy` omits.
fn canonical_timezone(tz: &str) -> &str {
    match tz {
        "Asia/Saigon" => "Asia/Ho_Chi_Minh",
        "Asia/Calcutta" => "Asia/Kolkata",
        "Asia/Katmandu" => "Asia/Kathmandu",
        "Asia/Rangoon" => "Asia/Yangon",
        "Europe/Kiev" => "Europe/Kyiv",
        "America/Buenos_Aires" => "America/Argentina/Buenos_Aires",
        other => other,
    }
}

/// Resolve a client `tz` to a name Postgres knows, or `None` if neither the
/// original nor its IANA backward alias exists on this host.
pub async fn resolve_timezone(pool: &PgPool, tz: &str) -> Result<Option<String>, sqlx::Error> {
    if timezone_exists(pool, tz).await? {
        return Ok(Some(tz.to_string()));
    }
    let mapped = canonical_timezone(tz);
    if mapped != tz && timezone_exists(pool, mapped).await? {
        return Ok(Some(mapped.to_string()));
    }
    Ok(None)
}

/// Slice filters, with `$1` from, `$2` to, `$3` tz, `$4` board, `$5` connector.
const SLICE_FILTER: &str = "
    ($1::date IS NULL OR s.at >= ($1::date::timestamp AT TIME ZONE $3))
    AND ($2::date IS NULL OR s.at < (($2::date + 1)::timestamp AT TIME ZONE $3))
    AND ($4::varchar IS NULL OR s.board_id = $4)
    AND ($5::text IS NULL OR s.connector = $5)";

fn tokens_from_row(row: &sqlx::postgres::PgRow) -> Result<TokenCounts, sqlx::Error> {
    Ok(TokenCounts {
        input: row.try_get("input_tokens")?,
        cache_read: row.try_get("cache_read_tokens")?,
        cache_write: row.try_get("cache_write_tokens")?,
        output: row.try_get("output_tokens")?,
        reasoning: row.try_get("reasoning_tokens")?,
    })
}

const SUMS: &str = "
    sum(s.input_tokens)::int8 AS input_tokens,
    sum(s.cache_read_tokens)::int8 AS cache_read_tokens,
    sum(s.cache_write_tokens)::int8 AS cache_write_tokens,
    sum(s.output_tokens)::int8 AS output_tokens,
    sum(s.reasoning_tokens)::int8 AS reasoning_tokens,
    COALESCE(sum(s.unsplit_tokens), 0)::int8 AS unsplit,
    COALESCE(sum(s.total_tokens), 0)::int8 AS total,
    count(DISTINCT s.usage_id) AS runs";

type PgQuery<'q> = sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>;

/// `sql` with the query's five parameters bound as `$1` to `$5`.
fn bound<'q>(sql: &'q str, q: &SummaryQuery) -> PgQuery<'q> {
    sqlx::query(sql)
        .bind(q.from.clone())
        .bind(q.to.clone())
        .bind(q.tz.clone())
        .bind(q.board)
        .bind(q.connector.clone())
}

pub async fn usage_summary(pool: &PgPool, q: &SummaryQuery) -> Result<UsageSummary, sqlx::Error> {
    let bind = |sql| bound(sql, q);

    let totals_sql = format!(
        "SELECT {SUMS}, count(DISTINCT (s.at AT TIME ZONE $3)::date) AS active_days
         FROM usage_slices s WHERE {SLICE_FILTER}"
    );
    let row = bind(&totals_sql).fetch_one(pool).await?;
    let totals = SummaryTotals {
        tokens: tokens_from_row(&row)?,
        unsplit: row.try_get("unsplit")?,
        total: row.try_get("total")?,
        runs: row.try_get("runs")?,
        active_days: row.try_get("active_days")?,
    };

    let (key, label, connector) = q.group_by.sql();
    let rows_sql = format!(
        "SELECT {key} AS key, {label} AS label, {connector} AS connector, {SUMS}
         FROM usage_slices s
         LEFT JOIN agents a ON a.id = s.agent_id
         LEFT JOIN boards b ON b.id = s.board_id
         WHERE {SLICE_FILTER}
         GROUP BY 1
         ORDER BY total DESC, key"
    );
    let mut rows = Vec::new();
    for row in bind(&rows_sql).fetch_all(pool).await? {
        let key: String = row.try_get("key")?;
        let mut label: Option<String> = row.try_get("label")?;
        if q.group_by == GroupBy::Connector {
            label = coppice_connectors::get(&key).map(|d| d.display_name.to_string());
        }
        rows.push(SummaryRow {
            key,
            label,
            connector: row.try_get("connector")?,
            tokens: tokens_from_row(&row)?,
            unsplit: row.try_get("unsplit")?,
            total: row.try_get("total")?,
            runs: row.try_get("runs")?,
        });
    }

    let daily_sql = format!(
        "SELECT to_char((s.at AT TIME ZONE $3)::date, 'YYYY-MM-DD') AS day, {key} AS key,
                COALESCE(sum(s.total_tokens), 0)::int8 AS total
         FROM usage_slices s
         WHERE {SLICE_FILTER}
         GROUP BY 1, 2
         ORDER BY 1"
    );
    let mut daily: Vec<SummaryDay> = Vec::new();
    for row in bind(&daily_sql).fetch_all(pool).await? {
        let day: String = row.try_get("day")?;
        let key: String = row.try_get("key")?;
        let total: i64 = row.try_get("total")?;
        match daily.last_mut() {
            Some(last) if last.day == day => {
                last.by_key.insert(key, total);
            }
            _ => daily.push(SummaryDay {
                day,
                by_key: BTreeMap::from([(key, total)]),
            }),
        }
    }

    let coverage = coverage(pool, q).await?;
    let per_done_ticket = per_done_ticket(pool, q).await?;
    Ok(UsageSummary {
        totals,
        rows,
        daily,
        coverage,
        per_done_ticket,
    })
}

async fn coverage(pool: &PgPool, q: &SummaryQuery) -> Result<SummaryCoverage, sqlx::Error> {
    let in_range = |column: &str| {
        format!(
            "($1::date IS NULL OR {column} >= ($1::date::timestamp AT TIME ZONE $3))
             AND ($2::date IS NULL OR {column} < (($2::date + 1)::timestamp AT TIME ZONE $3))"
        )
    };
    let tracked_sql = format!(
        "SELECT count(*) FROM run_usage u
         WHERE {}
           AND ($4::varchar IS NULL OR u.board_id = $4)
           AND ($5::text IS NULL OR u.connector = $5)",
        in_range("u.run_created_at")
    );
    let tracked_runs: i64 = bound(&tracked_sql, q).fetch_one(pool).await?.try_get(0)?;
    let untracked_sql = format!(
        "SELECT ar.connector, count(*) AS runs
         FROM agent_runs ar
         LEFT JOIN tickets t ON t.id = ar.ticket_id
         WHERE ar.connector IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM run_usage u WHERE u.run_id = ar.id)
           AND {}
           AND ($4::varchar IS NULL OR t.board_id = $4)
           AND ($5::text IS NULL OR ar.connector = $5)
         GROUP BY ar.connector
         ORDER BY runs DESC, ar.connector",
        in_range("ar.created_at")
    );
    let mut untracked_by_connector = Vec::new();
    for row in bound(&untracked_sql, q).fetch_all(pool).await? {
        untracked_by_connector.push(UntrackedConnector {
            connector: row.try_get("connector")?,
            runs: row.try_get("runs")?,
        });
    }
    Ok(SummaryCoverage {
        tracked_runs,
        untracked_runs: untracked_by_connector.iter().map(|c| c.runs).sum(),
        untracked_by_connector,
    })
}

async fn per_done_ticket(
    pool: &PgPool,
    q: &SummaryQuery,
) -> Result<Option<PerDoneTicket>, sqlx::Error> {
    let row = bound(
        "WITH done AS (
             SELECT t.id FROM tickets t
             WHERE t.parent_ticket_id IS NULL AND t.status = 'done' AND t.done_at IS NOT NULL
               AND ($1::date IS NULL OR t.done_at >= ($1::date::timestamp AT TIME ZONE $3))
               AND ($2::date IS NULL OR t.done_at < (($2::date + 1)::timestamp AT TIME ZONE $3))
               AND ($4::varchar IS NULL OR t.board_id = $4)
         ),
         totals AS (
             SELECT s.root_ticket_id, sum(s.total_tokens)::float8 AS total
             FROM usage_slices s
             JOIN done ON done.id = s.root_ticket_id
             WHERE ($5::text IS NULL OR s.connector = $5)
             GROUP BY s.root_ticket_id
         )
         SELECT count(*) AS tickets,
                percentile_cont(0.5) WITHIN GROUP (ORDER BY total) AS median,
                avg(total) AS average
         FROM totals",
        q,
    )
    .fetch_one(pool)
    .await?;
    let tickets: i64 = row.try_get("tickets")?;
    if tickets == 0 {
        return Ok(None);
    }
    Ok(Some(PerDoneTicket {
        tickets,
        median: row.try_get("median")?,
        average: row.try_get("average")?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::usage::{
        Granularity, ModelTokens, RequestUsage, RunTotalUsage, SubagentUsage, UsageEvent,
        UsageSnapshot, UsageTracker,
    };
    use crate::services::usage_service::UsageService;

    async fn pool() -> Option<PgPool> {
        crate::db::shared_test_pool().await.ok()
    }

    /// One test's own world: a board, an agent, and a connector name no other
    /// test uses, so filters keep parallel tests apart.
    struct World {
        pool: PgPool,
        board: Id,
        agent: Id,
        connector: String,
    }

    impl World {
        async fn new(pool: PgPool) -> Self {
            let board = Id::new();
            sqlx::query("INSERT INTO boards (id, name, slug) VALUES ($1, 'summary', $2)")
                .bind(board)
                .bind(format!("summary-{board}"))
                .execute(&pool)
                .await
                .unwrap();
            let agent = Id::new();
            sqlx::query(
                "INSERT INTO agents (id, name, role, responsibilities, system_prompt, connector)
                 VALUES ($1, 'summary agent', 'worker', '{}', 'prompt', 'mock')",
            )
            .bind(agent)
            .execute(&pool)
            .await
            .unwrap();
            World {
                pool,
                board,
                agent,
                connector: format!("test-{}", Id::new()),
            }
        }

        async fn ticket(&self, parent: Option<Id>) -> Id {
            let id = Id::new();
            sqlx::query(
                "INSERT INTO tickets (id, board_id, title, status, created_by, parent_ticket_id)
                 VALUES ($1, $2, 'summary', 'todo', 'test', $3)",
            )
            .bind(id)
            .bind(self.board)
            .bind(parent)
            .execute(&self.pool)
            .await
            .unwrap();
            id
        }

        /// A chat session to own a run without a ticket (no board: Workspace).
        async fn chat_session(&self) -> Id {
            let user = Id::new();
            sqlx::query(
                "INSERT INTO users (id, email, password_hash, role) VALUES ($1, $2, 'x', 'member')",
            )
            .bind(user)
            .bind(format!("{user}@summary.test"))
            .execute(&self.pool)
            .await
            .unwrap();
            let session = Id::new();
            sqlx::query(
                "INSERT INTO chat_sessions (id, owner_user_id, agent_id) VALUES ($1, $2, $3)",
            )
            .bind(session)
            .bind(user)
            .bind(self.agent)
            .execute(&self.pool)
            .await
            .unwrap();
            session
        }

        /// A run that started on `connector` (this world's when `None`). Without
        /// a ticket, a chat session owns it.
        async fn run(&self, ticket: Option<Id>, job_type: &str, connector: Option<&str>) -> Id {
            let chat = match ticket {
                Some(_) => None,
                None => Some(self.chat_session().await),
            };
            let id = Id::new();
            sqlx::query(
                "INSERT INTO agent_runs
                     (id, ticket_id, chat_session_id, agent_id, job_type, status,
                      sandbox_profile_id, connector, reported_model)
                 VALUES ($1, $2, $3, $4, $5, 'succeeded', 'default', $6, 'reported-model')",
            )
            .bind(id)
            .bind(ticket)
            .bind(chat)
            .bind(self.agent)
            .bind(job_type)
            .bind(connector.unwrap_or(&self.connector))
            .execute(&self.pool)
            .await
            .unwrap();
            id
        }

        async fn save(&self, run: Id, events: Vec<UsageEvent>) -> UsageSnapshot {
            let mut tracker = UsageTracker::new();
            for event in events {
                tracker.apply(event);
            }
            let snapshot = tracker.snapshot();
            assert!(UsageService::new(&self.pool)
                .save_snapshot(run, &snapshot)
                .await
                .unwrap());
            snapshot
        }

        fn query(&self, group_by: GroupBy) -> SummaryQuery {
            SummaryQuery {
                from: None,
                to: None,
                tz: "UTC".into(),
                group_by,
                board: None,
                connector: Some(self.connector.clone()),
            }
        }

        async fn slice_total(&self, run: Id) -> i64 {
            sqlx::query_scalar(
                "SELECT COALESCE(sum(total_tokens), 0)::int8 FROM usage_slices WHERE usage_id = $1",
            )
            .bind(run)
            .fetch_one(&self.pool)
            .await
            .unwrap()
        }
    }

    fn tokens(input: i64, output: i64) -> TokenCounts {
        TokenCounts {
            input: Some(input),
            output: Some(output),
            ..TokenCounts::default()
        }
    }

    fn request(id: &str, model: &str, granularity: Granularity, t: TokenCounts) -> UsageEvent {
        UsageEvent::Request(RequestUsage {
            id: Some(id.into()),
            granularity,
            model: Some(model.into()),
            tokens: t,
        })
    }

    fn run_total(t: TokenCounts, by_model: Vec<(&str, TokenCounts)>) -> UsageEvent {
        UsageEvent::RunTotal(RunTotalUsage {
            tokens: t,
            by_model: by_model
                .into_iter()
                .map(|(model, tokens)| ModelTokens {
                    model: model.into(),
                    tokens,
                })
                .collect(),
            cost_usd: None,
        })
    }

    fn snapshot_total(snapshot: &UsageSnapshot) -> i64 {
        snapshot.totals.tokens.total().unwrap_or(0) + snapshot.totals.subagent_tokens.unwrap_or(0)
    }

    /// Four runs, one per level of detail a connector may report at.
    async fn four_runs(w: &World, ticket: Option<Id>) -> Vec<(Id, UsageSnapshot)> {
        let mut runs = Vec::new();
        // Per request, a total-only subagent, and a reported total split by model.
        let run = w.run(ticket, "work_on_ticket", None).await;
        let snap = w
            .save(
                run,
                vec![
                    request("r1", "big", Granularity::Request, tokens(10, 5)),
                    request("r2", "big", Granularity::Request, tokens(20, 5)),
                    UsageEvent::Subagent(SubagentUsage {
                        id: "s1".into(),
                        model: Some("small".into()),
                        total_tokens: Some(40),
                        ..SubagentUsage::default()
                    }),
                    run_total(
                        tokens(70, 20),
                        vec![("big", tokens(35, 15)), ("small", tokens(35, 5))],
                    ),
                ],
            )
            .await;
        runs.push((run, snap));
        // Per turn, no total.
        let run = w.run(ticket, "work_on_ticket", None).await;
        let snap = w
            .save(
                run,
                vec![
                    request("t1", "turny", Granularity::Turn, tokens(100, 10)),
                    request("t2", "turny", Granularity::Turn, tokens(200, 20)),
                ],
            )
            .await;
        runs.push((run, snap));
        // Only a run total.
        let run = w.run(ticket, "plan_ticket", None).await;
        let snap = w.save(run, vec![run_total(tokens(500, 50), vec![])]).await;
        runs.push((run, snap));
        // A request and a subagent total, no run total.
        let run = w.run(ticket, "work_on_ticket", None).await;
        let snap = w
            .save(
                run,
                vec![
                    request("p1", "big", Granularity::Request, tokens(1, 1)),
                    UsageEvent::Subagent(SubagentUsage {
                        id: "s1".into(),
                        total_tokens: Some(300),
                        ..SubagentUsage::default()
                    }),
                ],
            )
            .await;
        runs.push((run, snap));
        runs
    }

    #[tokio::test]
    async fn slices_add_up_to_each_runs_total_at_every_level_of_detail() {
        let Some(pool) = pool().await else { return };
        let w = World::new(pool).await;
        let ticket = w.ticket(None).await;
        for (run, snapshot) in four_runs(&w, Some(ticket)).await {
            assert_eq!(w.slice_total(run).await, snapshot_total(&snapshot), "{run}");
        }

        // The split run: its remainder follows the split by model.
        let summary = usage_summary(&w.pool, &w.query(GroupBy::Model)).await.unwrap();
        let by_model: BTreeMap<_, _> = summary
            .rows
            .iter()
            .map(|r| (r.label.clone().unwrap_or_default(), r.total))
            .collect();
        // big: 50 from the split run (its rows plus its remainder) and 2 from
        // the partial run; small: 40, its remainder holding the subagent's
        // total; turny: 330; the total-only run uses its reported model.
        assert_eq!(by_model["big"], 52);
        assert_eq!(by_model["small"], 40);
        assert_eq!(by_model["turny"], 330);
        assert_eq!(by_model["reported-model"], 550);
        assert_eq!(by_model[""], 300, "the unsplit subagent has no model");
        assert_eq!(summary.totals.total, 52 + 40 + 330 + 550 + 300);
        assert_eq!(summary.totals.unsplit, 300);
        assert_eq!(summary.totals.runs, 4);
        assert_eq!(summary.totals.active_days, 1);
    }

    #[tokio::test]
    async fn every_grouping_adds_up_to_the_total() {
        let Some(pool) = pool().await else { return };
        let w = World::new(pool).await;
        let ticket = w.ticket(None).await;
        four_runs(&w, Some(ticket)).await;
        // Workspace runs: compaction and a chat turn have no board.
        let compaction = w.run(None, JOB_TYPE_COMPACT_KNOWLEDGE, None).await;
        w.save(compaction, vec![run_total(tokens(1000, 0), vec![])]).await;
        let chat = w.run(None, "chat_turn", None).await;
        w.save(chat, vec![run_total(tokens(7, 0), vec![])]).await;

        for group_by in [
            GroupBy::Model,
            GroupBy::Agent,
            GroupBy::Board,
            GroupBy::Connector,
            GroupBy::JobType,
        ] {
            let summary = usage_summary(&w.pool, &w.query(group_by)).await.unwrap();
            let rows: i64 = summary.rows.iter().map(|r| r.total).sum();
            assert_eq!(rows, summary.totals.total, "{group_by:?}");
            let days: i64 = summary.daily.iter().flat_map(|d| d.by_key.values()).sum();
            assert_eq!(days, summary.totals.total, "{group_by:?}");
            let keys: Vec<_> = summary.rows.iter().map(|r| r.key.as_str()).collect();
            match group_by {
                GroupBy::Board => {
                    assert!(keys.contains(&"workspace"), "{keys:?}");
                    assert!(keys.contains(&w.board.to_string().as_str()), "{keys:?}");
                }
                GroupBy::JobType => {
                    assert!(keys.contains(&"workspace_overhead"), "{keys:?}");
                    assert!(keys.contains(&"chat_turn"), "{keys:?}");
                }
                GroupBy::Agent => assert_eq!(keys, [w.agent.to_string().as_str()]),
                GroupBy::Connector => assert_eq!(keys, [w.connector.as_str()]),
                GroupBy::Model => {}
            }
        }

        // The board filter leaves Workspace runs out.
        let mut on_board = w.query(GroupBy::Board);
        on_board.board = Some(w.board);
        let summary = usage_summary(&w.pool, &on_board).await.unwrap();
        let keys: Vec<_> = summary.rows.iter().map(|r| r.key.clone()).collect();
        assert_eq!(keys, [w.board.to_string()]);
    }

    #[tokio::test]
    async fn days_follow_the_viewers_time_zone() {
        let Some(pool) = pool().await else { return };
        let w = World::new(pool).await;
        let run = w.run(None, "work_on_ticket", None).await;
        w.save(
            run,
            vec![request("r1", "m", Granularity::Request, tokens(10, 0))],
        )
        .await;
        sqlx::query(
            "UPDATE run_llm_requests SET created_at = '2026-03-01 23:30:00+00' WHERE usage_id = $1",
        )
        .bind(run)
        .execute(&w.pool)
        .await
        .unwrap();
        let around_midnight = |tz: &str| {
            let mut q = w.query(GroupBy::Model);
            q.tz = tz.into();
            q.from = Some("2026-03-01".into());
            q.to = Some("2026-03-02".into());
            q
        };
        let utc = usage_summary(&w.pool, &around_midnight("UTC")).await.unwrap().daily;
        assert_eq!(utc.len(), 1);
        assert_eq!(utc[0].day, "2026-03-01");
        let hanoi = usage_summary(&w.pool, &around_midnight("Asia/Ho_Chi_Minh"))
            .await
            .unwrap()
            .daily;
        assert_eq!(hanoi[0].day, "2026-03-02");

        // Legacy Chromium alias: map table points at the canonical zone; day
        // buckets match whether Postgres still knows the alias or we remap.
        assert_eq!(canonical_timezone("Asia/Saigon"), "Asia/Ho_Chi_Minh");
        assert_eq!(canonical_timezone("UTC"), "UTC");
        let resolved = resolve_timezone(&w.pool, "Asia/Saigon").await.unwrap().unwrap();
        assert!(
            resolved == "Asia/Saigon" || resolved == "Asia/Ho_Chi_Minh",
            "unexpected resolve: {resolved}"
        );
        let saigon = usage_summary(&w.pool, &around_midnight(&resolved)).await.unwrap().daily;
        assert_eq!(saigon, hanoi);
        assert_eq!(resolve_timezone(&w.pool, "Mars/Base").await.unwrap(), None);

        // The range is in the viewer's days too: 23:30 UTC on Mar 1 is Mar 2 in Hanoi.
        let mut q = w.query(GroupBy::Model);
        q.tz = "Asia/Ho_Chi_Minh".into();
        q.from = Some("2026-03-01".into());
        q.to = Some("2026-03-01".into());
        assert_eq!(usage_summary(&w.pool, &q).await.unwrap().totals.total, 0);
    }

    #[tokio::test]
    async fn untracked_runs_are_only_in_the_coverage() {
        let Some(pool) = pool().await else { return };
        let w = World::new(pool).await;
        let tracked = w.run(None, "work_on_ticket", None).await;
        w.save(tracked, vec![run_total(tokens(5, 5), vec![])]).await;
        let other = format!("{}-untracked", w.connector);
        w.run(None, "work_on_ticket", Some(&other)).await;
        w.run(None, "work_on_ticket", Some(&other)).await;

        let mut q = w.query(GroupBy::Connector);
        q.connector = Some(other.clone());
        let summary = usage_summary(&w.pool, &q).await.unwrap();
        assert!(summary.rows.is_empty());
        assert_eq!(summary.totals.total, 0);
        assert_eq!(summary.coverage.tracked_runs, 0);
        assert_eq!(summary.coverage.untracked_runs, 2);
        assert_eq!(summary.coverage.untracked_by_connector[0].connector, other);

        let summary = usage_summary(&w.pool, &w.query(GroupBy::Connector)).await.unwrap();
        assert_eq!(summary.coverage.tracked_runs, 1);
        assert_eq!(summary.coverage.untracked_runs, 0);
    }

    #[tokio::test]
    async fn deleting_a_ticket_leaves_the_totals_unchanged() {
        let Some(pool) = pool().await else { return };
        let w = World::new(pool).await;
        let ticket = w.ticket(None).await;
        four_runs(&w, Some(ticket)).await;
        let before = usage_summary(&w.pool, &w.query(GroupBy::Board)).await.unwrap();
        sqlx::query("DELETE FROM tickets WHERE id = $1")
            .bind(ticket)
            .execute(&w.pool)
            .await
            .unwrap();
        let after = usage_summary(&w.pool, &w.query(GroupBy::Board)).await.unwrap();
        assert_eq!(after.totals, before.totals);
        assert_eq!(after.rows, before.rows);
    }

    #[tokio::test]
    async fn tokens_per_done_ticket_is_the_median_over_whole_trees() {
        let Some(pool) = pool().await else { return };
        let w = World::new(pool).await;
        let output = |n| tokens(0, n);
        // A split ticket: 10 on the parent, 30 on its child. Counted once, as 40.
        let parent = w.ticket(None).await;
        let child = w.ticket(Some(parent)).await;
        let run = w.run(Some(parent), "work_on_ticket", None).await;
        w.save(run, vec![run_total(output(10), vec![])]).await;
        let run = w.run(Some(child), "work_on_ticket", None).await;
        w.save(run, vec![run_total(output(30), vec![])]).await;
        for n in [100, 50] {
            let ticket = w.ticket(None).await;
            let run = w.run(Some(ticket), "work_on_ticket", None).await;
            w.save(run, vec![run_total(output(n), vec![])]).await;
        }
        // Not Done: left out.
        let open = w.ticket(None).await;
        let run = w.run(Some(open), "work_on_ticket", None).await;
        w.save(run, vec![run_total(output(9999), vec![])]).await;
        sqlx::query("UPDATE tickets SET status = 'done' WHERE board_id = $1 AND id <> $2")
            .bind(w.board)
            .bind(open)
            .execute(&w.pool)
            .await
            .unwrap();

        let mut q = w.query(GroupBy::Model);
        q.board = Some(w.board);
        let per = usage_summary(&w.pool, &q).await.unwrap().per_done_ticket.unwrap();
        assert_eq!(per.tickets, 3);
        assert_eq!(per.median, 50.0);
        assert!((per.average - 190.0 / 3.0).abs() < 1e-9, "{}", per.average);
    }
}
