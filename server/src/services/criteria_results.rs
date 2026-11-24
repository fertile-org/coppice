//! Per-criterion results from done runs, and how Human Review shows them
//! against the approved plan's acceptance criteria (#150).
//!
//! Information only: nothing here gates Accept or Merge.
use crate::domain::Id;

use serde::Serialize;
use sqlx::PgPool;
use std::path::{Component, Path, PathBuf};
use crate::providers::{AgentRunResult, CriterionResult, CriterionStatus};

const QA_EVIDENCE_PREFIX: &str = ".agent/qa-evidence/";
const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];
const MAX_FILE_BYTES: u64 = 512 * 1024;

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
    /// Stable artifact filenames under the ticket's qa-evidence folder.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
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
    ///
    /// When `worktree` and `artifacts_dir` are set, accepted screenshots under
    /// `.agent/qa-evidence/` are copied into
    /// `artifacts_dir/tickets/{ticket_id}/qa-evidence/` and the stored paths
    /// are rewritten to those filenames. Invalid paths are dropped.
    pub async fn record(
        &self,
        ticket_id: Id,
        result: &AgentRunResult,
        commit_sha: &str,
        worktree: Option<&Path>,
        artifacts_dir: Option<&Path>,
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
        let mut stored = criteria_results.clone();
        if let (Some(worktree), Some(artifacts_dir)) = (worktree, artifacts_dir) {
            ingest_screenshots(ticket_id, &mut stored, worktree, artifacts_dir);
        } else {
            for result in &mut stored {
                result.screenshots.clear();
            }
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
        .bind(serde_json::to_value(&stored).unwrap_or_default())
        .execute(self.pool)
        .await?;
        Ok(())
    }

    pub async fn get(&self, ticket_id: Id) -> Result<Option<StoredCriteriaResults>, sqlx::Error> {
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

/// Absolute path of the durable QA evidence folder for a ticket.
pub fn qa_evidence_dir(artifacts_dir: &Path, ticket_id: Id) -> PathBuf {
    artifacts_dir
        .join("tickets")
        .join(ticket_id.to_string())
        .join("qa-evidence")
}

/// Read one image from the ticket's qa-evidence folder. Rejects path escape and
/// non-images. Caps at 512 KiB.
pub fn read_qa_evidence(
    artifacts_dir: &Path,
    ticket_id: Id,
    file: &str,
) -> Result<Vec<u8>, QaEvidenceReadError> {
    let name = sanitize_artifact_name(file).ok_or(QaEvidenceReadError::Invalid)?;
    let root = qa_evidence_dir(artifacts_dir, ticket_id);
    let dest = root.join(&name);
    let meta = std::fs::metadata(&dest).map_err(|_| QaEvidenceReadError::NotFound)?;
    if !meta.is_file() {
        return Err(QaEvidenceReadError::NotFound);
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(QaEvidenceReadError::Invalid);
    }
    std::fs::read(&dest).map_err(|_| QaEvidenceReadError::NotFound)
}

#[derive(Debug, PartialEq, Eq)]
pub enum QaEvidenceReadError {
    Invalid,
    NotFound,
}

fn ingest_screenshots(
    ticket_id: Id,
    results: &mut [CriterionResult],
    worktree: &Path,
    artifacts_dir: &Path,
) {
    let dest_root = qa_evidence_dir(artifacts_dir, ticket_id);
    let _ = std::fs::remove_dir_all(&dest_root);
    if let Err(err) = std::fs::create_dir_all(&dest_root) {
        tracing::warn!(error = %err, %ticket_id, "qa-evidence folder create failed");
        for result in results.iter_mut() {
            result.screenshots.clear();
        }
        return;
    }
    for (ci, result) in results.iter_mut().enumerate() {
        let mut kept = Vec::new();
        for (si, raw) in result.screenshots.iter().enumerate() {
            let Some(rel) = accept_source_path(raw) else {
                tracing::warn!(path = %raw, %ticket_id, "rejected qa screenshot path");
                continue;
            };
            let src = worktree.join(&rel);
            let Ok(meta) = std::fs::metadata(&src) else {
                tracing::warn!(path = %rel, %ticket_id, "qa screenshot missing");
                continue;
            };
            if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_FILE_BYTES {
                tracing::warn!(path = %rel, %ticket_id, "qa screenshot rejected (size/type)");
                continue;
            }
            let ext = Path::new(&rel)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("png")
                .to_ascii_lowercase();
            let name = format!("c{ci}-s{si}.{ext}");
            let dest = dest_root.join(&name);
            if let Err(err) = std::fs::copy(&src, &dest) {
                tracing::warn!(error = %err, path = %rel, %ticket_id, "qa screenshot copy failed");
                continue;
            }
            kept.push(name);
        }
        result.screenshots = kept;
    }
}

/// Worktree-relative path under `.agent/qa-evidence/` with an image extension.
fn accept_source_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_start_matches("./");
    if !trimmed.starts_with(QA_EVIDENCE_PREFIX) {
        return None;
    }
    let rest = &trimmed[QA_EVIDENCE_PREFIX.len()..];
    if rest.is_empty() {
        return None;
    }
    let path = Path::new(rest);
    if path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::RootDir))
    {
        return None;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !IMAGE_EXT.iter().any(|a| *a == ext) {
        return None;
    }
    Some(format!("{QA_EVIDENCE_PREFIX}{rest}"))
}

fn sanitize_artifact_name(file: &str) -> Option<String> {
    let name = file.trim().trim_start_matches('/');
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return None;
    }
    let path = Path::new(name);
    if path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::RootDir))
    {
        return None;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !IMAGE_EXT.iter().any(|a| *a == ext) {
        return None;
    }
    Some(name.to_string())
}

/// Content-Type for a qa-evidence filename.
pub fn qa_evidence_media_type(file: &str) -> &'static str {
    match Path::new(file)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// Lowercase, collapse spaces, and drop a trailing "(verified by …)" or
/// "— verified by …", so an agent's echo of a criterion still matches.
fn normalize(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut core = lower.as_str();
    for marker in [
        "(verified by",
        "— verified by",
        "- verified by",
        "verified by:",
    ] {
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
            crate::services::human_review_service::reviewed_sha_matches(
                &stored.commit_sha,
                reviewed,
            )
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
                screenshots: found
                    .map(|result| result.screenshots.clone())
                    .unwrap_or_default(),
                steps: found
                    .map(|result| result.steps.clone())
                    .unwrap_or_default(),
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
            screenshots: Vec::new(),
            steps: Vec::new(),
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
        assert_eq!(
            view[0].evidence.as_deref(),
            Some("evidence for refuses the drag")
        );
    }

    #[test]
    fn a_mix_of_met_and_not_met_with_a_missing_one() {
        let criteria = vec!["One".to_string(), "Two".to_string(), "Three".to_string()];
        let stored = stored(
            "abc",
            vec![
                result("one", CriterionStatus::Met),
                result("two", CriterionStatus::NotMet),
            ],
        );
        let view = criteria_view(&criteria, Some(&stored), Some("abc"));
        assert_eq!(
            view.iter().map(|item| item.status).collect::<Vec<_>>(),
            vec![
                CriterionStatus::Met,
                CriterionStatus::NotMet,
                CriterionStatus::NotChecked
            ]
        );
    }

    #[test]
    fn a_result_for_an_older_commit_reads_as_not_checked() {
        let criteria = vec!["One".to_string()];
        let mut met = result("one", CriterionStatus::Met);
        met.screenshots = vec!["c0-s0.png".into()];
        met.steps = vec!["click Save".into()];
        let stored = stored("old", vec![met]);
        let view = criteria_view(&criteria, Some(&stored), Some("new"));
        assert_eq!(view[0].status, CriterionStatus::NotChecked);
        assert_eq!(view[0].evidence, None);
        assert!(view[0].screenshots.is_empty());
        assert!(view[0].steps.is_empty());
        let none = criteria_view(&criteria, Some(&stored), None);
        assert_eq!(none[0].status, CriterionStatus::NotChecked);
    }

    #[test]
    fn screenshots_pass_through_on_matching_commit() {
        let criteria = vec!["One".to_string()];
        let mut met = result("one", CriterionStatus::Met);
        met.screenshots = vec!["c0-s0.png".into()];
        met.steps = vec!["open drawer".into()];
        let stored = stored("abc", vec![met]);
        let view = criteria_view(&criteria, Some(&stored), Some("abc"));
        assert_eq!(view[0].screenshots, vec!["c0-s0.png"]);
        assert_eq!(view[0].steps, vec!["open drawer"]);
    }

    #[test]
    fn accept_source_path_requires_qa_evidence_prefix() {
        assert!(accept_source_path(".agent/qa-evidence/a.png").is_some());
        assert!(accept_source_path("./.agent/qa-evidence/a.jpeg").is_some());
        assert!(accept_source_path(".agent/qa-evidence/../secrets.png").is_none());
        assert!(accept_source_path("artifacts/a.png").is_none());
        assert!(accept_source_path(".agent/qa-evidence/a.txt").is_none());
    }

    #[test]
    fn sanitize_artifact_name_rejects_escape_and_non_images() {
        assert_eq!(
            sanitize_artifact_name("c0-s0.png").as_deref(),
            Some("c0-s0.png")
        );
        assert!(sanitize_artifact_name("../x.png").is_none());
        assert!(sanitize_artifact_name("a/b.png").is_none());
        assert!(sanitize_artifact_name("notes.txt").is_none());
    }

    #[test]
    fn ingest_copies_and_rewrites_filenames() {
        let work = tempfile::tempdir().unwrap();
        let artifacts = tempfile::tempdir().unwrap();
        let src_dir = work.path().join(".agent/qa-evidence");
        std::fs::create_dir_all(&src_dir).unwrap();
        let png = minimal_png();
        std::fs::write(src_dir.join("shot.png"), &png).unwrap();
        let ticket_id = Id::new();
        let mut results = vec![CriterionResult {
            criterion: "One".into(),
            status: CriterionStatus::Met,
            evidence: None,
            screenshots: vec![".agent/qa-evidence/shot.png".into()],
            steps: vec!["step".into()],
        }];
        ingest_screenshots(ticket_id, &mut results, work.path(), artifacts.path());
        assert_eq!(results[0].screenshots, vec!["c0-s0.png"]);
        let dest = qa_evidence_dir(artifacts.path(), ticket_id).join("c0-s0.png");
        assert_eq!(std::fs::read(dest).unwrap(), png);
        assert_eq!(
            read_qa_evidence(artifacts.path(), ticket_id, "c0-s0.png").unwrap(),
            png
        );
        assert_eq!(
            read_qa_evidence(artifacts.path(), ticket_id, "../c0-s0.png"),
            Err(QaEvidenceReadError::Invalid)
        );
    }

    fn minimal_png() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE,
            0x02, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn results_parse_leniently() {
        let done: AgentRunResult = serde_json::from_value(serde_json::json!({
            "status": "done",
            "summary": "s",
            "criteriaResults": [
                {
                    "criterion": "a",
                    "status": "met",
                    "evidence": "test x",
                    "screenshots": [".agent/qa-evidence/a.png", ""],
                    "steps": ["open"]
                },
                {"criterion": "b", "status": "maybe"},
                {"status": "met"},
                "junk"
            ]
        }))
        .unwrap();
        let AgentRunResult::Done {
            criteria_results, ..
        } = done
        else {
            panic!("done");
        };
        assert_eq!(criteria_results.len(), 2);
        assert_eq!(
            criteria_results[0].screenshots,
            vec![".agent/qa-evidence/a.png"]
        );
        assert_eq!(criteria_results[0].steps, vec!["open"]);
        assert_eq!(criteria_results[1].status, CriterionStatus::NotChecked);
        let missing: AgentRunResult = serde_json::from_value(
            serde_json::json!({"status": "done", "summary": "s", "criteriaResults": "nope"}),
        )
        .unwrap();
        let AgentRunResult::Done {
            criteria_results, ..
        } = missing
        else {
            panic!("done");
        };
        assert!(criteria_results.is_empty());
    }
}
