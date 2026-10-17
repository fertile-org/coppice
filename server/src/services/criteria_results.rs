//! Per-criterion results from done runs, and how Human Review shows them
//! against the approved plan's acceptance criteria (#150).
//!
//! Information only: nothing here gates Accept or Merge.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::providers::{AgentRunResult, CriterionResult, CriterionStatus};

pub struct StoredCriteriaResults {
    pub commit_sha: String,
    pub results: Vec<CriterionResult>,
}

/// One acceptance criterion as Human Review shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CriterionView {
    pub text: String,
    pub status: CriterionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

pub struct CriteriaResultsService<'a> {
    pool: &'a PgPool,
}

impl<'a> CriteriaResultsService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Keep a done result's criteria report for `commit_sha`. A result with no
    /// report leaves the stored one alone; it then belongs to an older commit
    /// and reads as not checked.
    pub async fn record(
        &self,
        ticket_id: Uuid,
        result: &AgentRunResult,
        commit_sha: &str,
    ) -> Result<(), sqlx::Error> {
        let AgentRunResult::Done {
            criteria_results, ..
        } = result
        else {
            return Ok(());
        };
        let commit_sha = crate::services::human_review_service::normalize_sha(commit_sha);
        if criteria_results.is_empty() || commit_sha.is_empty() {
            return Ok(());
        }
        sqlx::query(
            r#"
            INSERT INTO ticket_criteria_results (ticket_id, commit_sha, results, updated_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT (ticket_id) DO UPDATE SET
                commit_sha = EXCLUDED.commit_sha,
                results = EXCLUDED.results,
                updated_at = now()
            "#,
        )
        .bind(ticket_id)
        .bind(&commit_sha)
        .bind(serde_json::to_value(criteria_results).unwrap_or_default())
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn get(&self, ticket_id: Uuid) -> Result<Option<StoredCriteriaResults>, sqlx::Error> {
        let row: Option<(String, serde_json::Value)> = sqlx::query_as(
            "SELECT commit_sha, results FROM ticket_criteria_results WHERE ticket_id = $1",
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|(commit_sha, results)| StoredCriteriaResults {
            commit_sha,
            results: serde_json::from_value(results).unwrap_or_default(),
        }))
    }
}

/// Lowercase, collapse spaces, and drop a trailing "(verified by …)" or
/// "— verified by …", so an agent's echo of a criterion still matches.
fn normalize(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut core = lower.as_str();
    for marker in ["(verified by", "— verified by", "- verified by", "verified by:"] {
        if let Some(at) = core.find(marker) {
            core = &core[..at];
        }
    }
    core.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', ':', ';', ','])
        .to_string()
}

/// The plan's criteria with the stored results when they were written for the
/// reviewed commit. Otherwise every criterion is not checked. A result is
/// matched by its text, or by position when the counts agree.
pub fn criteria_view(
    criteria: &[String],
    stored: Option<&StoredCriteriaResults>,
    reviewed_sha: Option<&str>,
) -> Vec<CriterionView> {
    let current = stored.filter(|stored| {
        reviewed_sha.is_some_and(|reviewed| {
            crate::services::human_review_service::reviewed_sha_matches(&stored.commit_sha, reviewed)
        })
    });
    criteria
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let found = current.and_then(|stored| {
                let want = normalize(text);
                stored
                    .results
                    .iter()
                    .find(|result| normalize(&result.criterion) == want)
                    .or_else(|| {
                        (stored.results.len() == criteria.len()).then(|| &stored.results[index])
                    })
            });
            CriterionView {
                text: text.clone(),
                status: found.map_or(CriterionStatus::NotChecked, |result| result.status),
                evidence: found.and_then(|result| result.evidence.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(criterion: &str, status: CriterionStatus) -> CriterionResult {
        CriterionResult {
            criterion: criterion.into(),
            status,
            evidence: Some(format!("evidence for {criterion}")),
        }
    }

    fn stored(sha: &str, results: Vec<CriterionResult>) -> StoredCriteriaResults {
        StoredCriteriaResults {
            commit_sha: sha.into(),
            results,
        }
    }

    #[test]
    fn a_plan_with_no_criteria_shows_none() {
        assert!(criteria_view(&[], None, Some("abc")).is_empty());
    }

    #[test]
    fn all_met_on_the_reviewed_commit() {
        let criteria = vec![
            "Refuses the drag (verified by integration test)".to_string(),
            "API returns 409".to_string(),
        ];
        let stored = stored(
            "abc123",
            vec![
                result("refuses the drag", CriterionStatus::Met),
                result("API returns 409.", CriterionStatus::Met),
            ],
        );
        let view = criteria_view(&criteria, Some(&stored), Some("ABC123"));
        assert!(view.iter().all(|item| item.status == CriterionStatus::Met));
        assert_eq!(view[0].evidence.as_deref(), Some("evidence for refuses the drag"));
    }

    #[test]
    fn a_mix_of_met_and_not_met_with_a_missing_one() {
        let criteria = vec!["One".to_string(), "Two".to_string(), "Three".to_string()];
        let stored = stored(
            "abc",
            vec![result("one", CriterionStatus::Met), result("two", CriterionStatus::NotMet)],
        );
        let view = criteria_view(&criteria, Some(&stored), Some("abc"));
        assert_eq!(
            view.iter().map(|item| item.status).collect::<Vec<_>>(),
            vec![CriterionStatus::Met, CriterionStatus::NotMet, CriterionStatus::NotChecked]
        );
    }

    #[test]
    fn a_result_for_an_older_commit_reads_as_not_checked() {
        let criteria = vec!["One".to_string()];
        let stored = stored("old", vec![result("one", CriterionStatus::Met)]);
        let view = criteria_view(&criteria, Some(&stored), Some("new"));
        assert_eq!(view[0].status, CriterionStatus::NotChecked);
        assert_eq!(view[0].evidence, None);
        let none = criteria_view(&criteria, Some(&stored), None);
        assert_eq!(none[0].status, CriterionStatus::NotChecked);
    }

    #[test]
    fn results_parse_leniently() {
        let done: AgentRunResult = serde_json::from_value(serde_json::json!({
            "status": "done",
            "summary": "s",
            "criteriaResults": [
                {"criterion": "a", "status": "met", "evidence": "test x"},
                {"criterion": "b", "status": "maybe"},
                {"status": "met"},
                "junk"
            ]
        }))
        .unwrap();
        let AgentRunResult::Done { criteria_results, .. } = done else {
            panic!("done");
        };
        assert_eq!(criteria_results.len(), 2);
        assert_eq!(criteria_results[1].status, CriterionStatus::NotChecked);
        let missing: AgentRunResult =
            serde_json::from_value(serde_json::json!({"status": "done", "summary": "s", "criteriaResults": "nope"}))
                .unwrap();
        let AgentRunResult::Done { criteria_results, .. } = missing else {
            panic!("done");
        };
        assert!(criteria_results.is_empty());
    }
}
