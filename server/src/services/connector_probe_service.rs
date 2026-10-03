//! Cached local connector probes and the admin Tools → Connectors status view.

use crate::AppConfig;
use coppice_connectors::probe::{probe, ProbeEnv, ProbeOutcome, ProbeReport};
use coppice_connectors::ConnectorDescriptor;
use serde::Serialize;
use sqlx::PgPool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

pub type EnvLookup = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct CachedProbe {
    pub report: ProbeReport,
    pub probed_at: OffsetDateTime,
}

/// In-memory probe results keyed by connector id. Probes use the server
/// process `HOME` and `PATH`, the same environment real runs get.
pub struct ConnectorProbes {
    cache: RwLock<HashMap<String, CachedProbe>>,
    env_lookup: EnvLookup,
}

impl Default for ConnectorProbes {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectorProbes {
    pub fn new() -> Self {
        Self::with_env_lookup(Arc::new(process_env_lookup))
    }

    pub fn with_env_lookup(env_lookup: EnvLookup) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            env_lookup,
        }
    }

    pub fn get(&self, id: &str) -> Option<CachedProbe> {
        self.cache
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
    }

    /// Re-probes one diagnosable connector; `None` for `mock`, unknown ids,
    /// or a probe task that panicked.
    pub async fn refresh(&self, config: &AppConfig, id: &str) -> Option<CachedProbe> {
        let descriptor = diagnosable(id)?;
        let command = config.agent.connectors.command(id).map(str::to_string);
        let lookup = self.env_lookup.clone();
        let report = tokio::task::spawn_blocking(move || {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            let path = std::env::var_os("PATH").unwrap_or_default();
            let env = ProbeEnv {
                home: &home,
                path: &path,
                command_override: command.as_deref(),
                env_lookup: &*lookup,
            };
            probe(descriptor, &env, PROBE_TIMEOUT)
        })
        .await;
        let report = match report {
            Ok(report) => report,
            Err(err) => {
                tracing::warn!(connector = id, error = %err, "connector probe task failed");
                return None;
            }
        };
        let cached = CachedProbe {
            report,
            probed_at: OffsetDateTime::now_utc(),
        };
        self.cache
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), cached.clone());
        Some(cached)
    }

    pub async fn refresh_all(&self, config: &AppConfig) {
        let ids: Vec<&str> = diagnosable_descriptors().map(|d| d.id).collect();
        futures_util::future::join_all(ids.into_iter().map(|id| self.refresh(config, id))).await;
    }
}

/// Any set, non-empty value counts, including non-UTF-8 ones.
fn process_env_lookup(key: &str) -> Option<String> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string_lossy().into_owned())
}

fn diagnosable_descriptors() -> impl Iterator<Item = &'static ConnectorDescriptor> {
    coppice_connectors::all()
        .iter()
        .filter(|d| d.id != coppice_connectors::MOCK)
}

/// Descriptor for an id shown on the Connectors page (every connector but `mock`).
pub fn diagnosable(id: &str) -> Option<&'static ConnectorDescriptor> {
    diagnosable_descriptors().find(|d| d.id == id)
}

#[derive(Debug, Clone)]
pub struct LastRun {
    pub run_id: Uuid,
    pub ticket_id: Option<Uuid>,
    pub status: String,
    pub finished_at: OffsetDateTime,
    pub ticket_get: bool,
    pub result_submit: bool,
}

/// Latest finished run per `agents.connector`, with whether it made at least
/// one `ok` `ticket_get` / `result_submit` gateway call.
pub async fn last_real_runs(pool: &PgPool) -> Result<HashMap<String, LastRun>, sqlx::Error> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        String,
        Uuid,
        Option<Uuid>,
        String,
        OffsetDateTime,
        bool,
        bool,
    )> = sqlx::query_as(
        "SELECT DISTINCT ON (a.connector) \
                 a.connector, r.id, r.ticket_id, r.status, \
                 COALESCE(r.ended_at, r.created_at) AS finished_at, \
                 EXISTS (SELECT 1 FROM run_tool_calls c \
                         WHERE c.run_id = r.id AND c.tool = 'ticket_get' AND c.status = 'ok'), \
                 EXISTS (SELECT 1 FROM run_tool_calls c \
                         WHERE c.run_id = r.id AND c.tool = 'result_submit' AND c.status = 'ok') \
             FROM agent_runs r JOIN agents a ON a.id = r.agent_id \
             WHERE r.status NOT IN ('queued', 'running') \
             ORDER BY a.connector, COALESCE(r.ended_at, r.created_at) DESC, r.id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(connector, run_id, ticket_id, status, finished_at, ticket_get, result_submit)| {
                (
                    connector,
                    LastRun {
                        run_id,
                        ticket_id,
                        status,
                        finished_at,
                        ticket_get,
                        result_submit,
                    },
                )
            },
        )
        .collect())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorStatusResponse {
    pub id: &'static str,
    pub display_name: &'static str,
    pub enabled: bool,
    pub cli: CliStatus,
    pub auth: AuthStatus,
    pub auth_hint: &'static str,
    pub docs_url: &'static str,
    pub last_run: Option<LastRunResponse>,
    pub last_check: Option<serde_json::Value>,
    pub probed_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    pub found: bool,
    pub path: Option<String>,
    pub probe: ProbeStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeStatus {
    pub status: &'static str,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub status: &'static str,
    pub env_set: Vec<&'static str>,
    pub paths_found: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastRunResponse {
    pub run_id: Uuid,
    pub ticket_id: Option<Uuid>,
    pub status: String,
    pub finished_at: String,
    pub ticket_get: bool,
    pub result_submit: bool,
}

impl From<&LastRun> for LastRunResponse {
    fn from(run: &LastRun) -> Self {
        Self {
            run_id: run.run_id,
            ticket_id: run.ticket_id,
            status: run.status.clone(),
            finished_at: run.finished_at.format(&Rfc3339).unwrap_or_default(),
            ticket_get: run.ticket_get,
            result_submit: run.result_submit,
        }
    }
}

/// A connector not yet probed (startup probe still running) reports
/// `probedAt: null`, `cli.found: false`, and probe status `not_run`.
pub fn connector_status(
    descriptor: &'static ConnectorDescriptor,
    config: &AppConfig,
    cached: Option<&CachedProbe>,
    last_run: Option<&LastRun>,
) -> ConnectorStatusResponse {
    let report = cached.map(|c| &c.report);
    ConnectorStatusResponse {
        id: descriptor.id,
        display_name: descriptor.display_name,
        enabled: config
            .agent
            .connectors
            .enabled(descriptor.id)
            .unwrap_or(false),
        cli: CliStatus {
            found: report.is_some_and(|r| r.binary.is_some()),
            path: report
                .and_then(|r| r.binary.as_ref())
                .map(|p| p.display().to_string()),
            probe: probe_status(report.map(|r| &r.probe)),
        },
        auth: auth_status(report),
        auth_hint: descriptor.install.auth_hint,
        docs_url: descriptor.install.docs_url,
        last_run: last_run.map(LastRunResponse::from),
        last_check: None,
        probed_at: cached.map(|c| c.probed_at.format(&Rfc3339).unwrap_or_default()),
    }
}

fn probe_status(outcome: Option<&ProbeOutcome>) -> ProbeStatus {
    let (status, detail) = match outcome {
        None | Some(ProbeOutcome::NotRun) => ("not_run", None),
        Some(ProbeOutcome::Ok { first_line }) => ("ok", Some(first_line.clone())),
        Some(ProbeOutcome::Failed { message }) => ("failed", Some(message.clone())),
        Some(ProbeOutcome::TimedOut) => ("timed_out", None),
    };
    ProbeStatus { status, detail }
}

fn auth_status(report: Option<&ProbeReport>) -> AuthStatus {
    let Some(report) = report else {
        return AuthStatus {
            status: "not_found",
            env_set: vec![],
            paths_found: vec![],
        };
    };
    let detected = !report.auth_env_set.is_empty() || !report.auth_paths_found.is_empty();
    let status = if detected {
        "detected"
    } else if report.probe_proves_auth && matches!(report.probe, ProbeOutcome::Ok { .. }) {
        "verified_by_probe"
    } else {
        "not_found"
    };
    AuthStatus {
        status,
        env_set: report.auth_env_set.clone(),
        paths_found: report.auth_paths_found.clone(),
    }
}

/// Status for every diagnosable connector from the cache (no probing).
pub async fn list_statuses(
    probes: &ConnectorProbes,
    config: &AppConfig,
    pool: Option<&PgPool>,
) -> Result<Vec<ConnectorStatusResponse>, sqlx::Error> {
    let runs = match pool {
        Some(pool) => last_real_runs(pool).await?,
        None => HashMap::new(),
    };
    Ok(diagnosable_descriptors()
        .map(|d| connector_status(d, config, probes.get(d.id).as_ref(), runs.get(d.id)))
        .collect())
}

pub enum CheckError {
    NotFound,
    ProbeFailed,
    Db(sqlx::Error),
}

/// Re-probes one connector and returns its fresh status.
pub async fn check_connector(
    probes: &ConnectorProbes,
    config: &AppConfig,
    pool: Option<&PgPool>,
    id: &str,
) -> Result<ConnectorStatusResponse, CheckError> {
    let descriptor = diagnosable(id).ok_or(CheckError::NotFound)?;
    let cached = probes
        .refresh(config, id)
        .await
        .ok_or(CheckError::ProbeFailed)?;
    let runs = match pool {
        Some(pool) => last_real_runs(pool).await.map_err(CheckError::Db)?,
        None => HashMap::new(),
    };
    Ok(connector_status(
        descriptor,
        config,
        Some(&cached),
        runs.get(id),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(
        env: &[&'static str],
        paths: &[&'static str],
        probe: ProbeOutcome,
        proves: bool,
    ) -> ProbeReport {
        ProbeReport {
            binary: Some(PathBuf::from("/bin/x")),
            auth_env_set: env.to_vec(),
            auth_paths_found: paths.to_vec(),
            probe,
            probe_proves_auth: proves,
        }
    }

    fn ok() -> ProbeOutcome {
        ProbeOutcome::Ok {
            first_line: "v1".into(),
        }
    }

    #[test]
    fn auth_status_mapping() {
        assert_eq!(auth_status(None).status, "not_found");
        assert_eq!(
            auth_status(Some(&report(&["K"], &[], ok(), true))).status,
            "detected"
        );
        assert_eq!(
            auth_status(Some(&report(&[], &[".x"], ok(), false))).status,
            "detected"
        );
        assert_eq!(
            auth_status(Some(&report(&[], &[], ok(), true))).status,
            "verified_by_probe"
        );
        assert_eq!(
            auth_status(Some(&report(&[], &[], ProbeOutcome::TimedOut, true))).status,
            "not_found"
        );
        assert_eq!(
            auth_status(Some(&report(&[], &[], ok(), false))).status,
            "not_found"
        );
    }

    #[test]
    fn probe_status_mapping() {
        let s = probe_status(Some(&ok()));
        assert_eq!((s.status, s.detail.as_deref()), ("ok", Some("v1")));
        let s = probe_status(Some(&ProbeOutcome::Failed {
            message: "boom".into(),
        }));
        assert_eq!((s.status, s.detail.as_deref()), ("failed", Some("boom")));
        assert_eq!(
            probe_status(Some(&ProbeOutcome::TimedOut)).status,
            "timed_out"
        );
        assert_eq!(probe_status(None).status, "not_run");
        assert!(probe_status(Some(&ProbeOutcome::NotRun)).detail.is_none());
    }

    #[test]
    fn diagnosable_excludes_mock_and_unknown() {
        assert!(diagnosable(coppice_connectors::MOCK).is_none());
        assert!(diagnosable("nope").is_none());
        assert!(diagnosable(coppice_connectors::KILO_CODE).is_some());
    }
}
