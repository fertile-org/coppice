//! Freshness of a knowledge revision, judged from its code citations against
//! the default branch head. Never fails: any trouble reads as `unknown`.

use serde::Serialize;
use sqlx::PgPool;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

use crate::knowledge::citations::{head_sha, GIT_TIMEOUT};
use crate::services::git_cmd;

/// Citations checked per call; the rest read as `unknown`.
const MAX_CHECKED_CITATIONS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Freshness {
    Current,
    Changed,
    Missing,
    Unknown,
    Uncited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshnessReport {
    pub state: Freshness,
    /// Cited paths that changed or disappeared.
    pub paths: Vec<String>,
}

impl FreshnessReport {
    fn of(state: Freshness) -> Self {
        Self {
            state,
            paths: Vec::new(),
        }
    }
}

/// Worst wins: Missing > Changed > Unknown > Current.
fn severity(state: Freshness) -> u8 {
    match state {
        Freshness::Missing => 3,
        Freshness::Changed => 2,
        Freshness::Unknown => 1,
        Freshness::Current | Freshness::Uncited => 0,
    }
}

fn combine(states: &[(Freshness, &str)]) -> FreshnessReport {
    let Some(worst) = states
        .iter()
        .map(|(state, _)| *state)
        .max_by_key(|state| severity(*state))
    else {
        return FreshnessReport::of(Freshness::Uncited);
    };
    let paths = states
        .iter()
        .filter(|(state, _)| matches!(state, Freshness::Changed | Freshness::Missing))
        .map(|(_, path)| (*path).to_string())
        .collect();
    FreshnessReport {
        state: worst,
        paths,
    }
}

struct RepoHead {
    cwd: PathBuf,
    head: String,
}

type CitationKey = (Uuid, String, String);

pub async fn freshness_for(pool: &PgPool, revision_ids: &[Uuid]) -> HashMap<Uuid, FreshnessReport> {
    if revision_ids.is_empty() {
        return HashMap::new();
    }
    let rows: Vec<(Uuid, Option<Uuid>, String, Option<String>)> = match sqlx::query_as(
        "SELECT revision_id, repo_id, path, commit_sha FROM knowledge_citations \
         WHERE revision_id = ANY($1) ORDER BY revision_id, created_at, path",
    )
    .bind(revision_ids)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "knowledge freshness could not read citations");
            return revision_ids
                .iter()
                .map(|id| (*id, FreshnessReport::of(Freshness::Unknown)))
                .collect();
        }
    };

    let mut heads: HashMap<Uuid, Option<RepoHead>> = HashMap::new();
    let mut checked: HashMap<CitationKey, Freshness> = HashMap::new();
    let mut per_revision: HashMap<Uuid, Vec<(Freshness, String)>> = HashMap::new();
    for (index, (revision_id, repo_id, path, sha)) in rows.into_iter().enumerate() {
        let state = match (repo_id, sha) {
            _ if index >= MAX_CHECKED_CITATIONS => Freshness::Unknown,
            (Some(repo_id), Some(sha)) => {
                let key = (repo_id, sha.clone(), path.clone());
                if let Some(state) = checked.get(&key) {
                    *state
                } else {
                    if let Entry::Vacant(slot) = heads.entry(repo_id) {
                        slot.insert(repo_head(pool, repo_id).await);
                    }
                    let state = match heads.get(&repo_id).and_then(Option::as_ref) {
                        Some(head) => check_citation(head, &sha, &path).await,
                        None => Freshness::Unknown,
                    };
                    checked.insert(key, state);
                    state
                }
            }
            _ => Freshness::Unknown,
        };
        per_revision
            .entry(revision_id)
            .or_default()
            .push((state, path));
    }

    revision_ids
        .iter()
        .map(|id| {
            let report = match per_revision.get(id) {
                Some(states) => {
                    let borrowed: Vec<(Freshness, &str)> =
                        states.iter().map(|(s, p)| (*s, p.as_str())).collect();
                    combine(&borrowed)
                }
                None => FreshnessReport::of(Freshness::Uncited),
            };
            (*id, report)
        })
        .collect()
}

async fn repo_head(pool: &PgPool, repo_id: Uuid) -> Option<RepoHead> {
    let (local_path, branch): (String, String) =
        sqlx::query_as("SELECT local_path, default_branch FROM repos WHERE id = $1")
            .bind(repo_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()?;
    let cwd = PathBuf::from(local_path);
    let head = head_sha(&cwd, &branch).await?;
    Some(RepoHead { cwd, head })
}

async fn check_citation(repo: &RepoHead, sha: &str, path: &str) -> Freshness {
    if sha.len() < 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Freshness::Unknown;
    }
    let cwd = Some(repo.cwd.as_path());
    let Ok(exists) = git_cmd::output_within(
        cwd,
        &["cat-file", "-e", &format!("{}:{path}", repo.head)],
        GIT_TIMEOUT,
    )
    .await
    else {
        return Freshness::Unknown;
    };
    if !exists.status.success() {
        return Freshness::Missing;
    }
    let range = format!("{sha}..{}", repo.head);
    let Ok(log) = git_cmd::output_within(
        cwd,
        &["log", "-1", "--format=%H", &range, "--", path],
        GIT_TIMEOUT,
    )
    .await
    else {
        return Freshness::Unknown;
    };
    if !log.status.success() {
        return Freshness::Unknown;
    }
    if log.stdout.iter().all(u8::is_ascii_whitespace) {
        Freshness::Current
    } else {
        Freshness::Changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_citations_is_uncited() {
        assert_eq!(combine(&[]).state, Freshness::Uncited);
    }

    #[test]
    fn worst_state_wins_and_lists_changed_paths() {
        let report = combine(&[
            (Freshness::Current, "a.rs"),
            (Freshness::Unknown, "b.rs"),
            (Freshness::Changed, "c.rs"),
        ]);
        assert_eq!(report.state, Freshness::Changed);
        assert_eq!(report.paths, vec!["c.rs"]);
        let report = combine(&[(Freshness::Changed, "c.rs"), (Freshness::Missing, "d.rs")]);
        assert_eq!(report.state, Freshness::Missing);
        assert_eq!(report.paths, vec!["c.rs", "d.rs"]);
        assert_eq!(
            combine(&[(Freshness::Current, "a.rs"), (Freshness::Unknown, "b.rs")]).state,
            Freshness::Unknown
        );
        assert_eq!(
            combine(&[(Freshness::Current, "a.rs")]).state,
            Freshness::Current
        );
    }

    #[test]
    fn serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&Freshness::Uncited).unwrap(),
            "\"uncited\""
        );
    }
}
