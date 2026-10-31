//! Code citations on knowledge candidates: path validation, anchoring to the
//! default branch head, and storage. Nothing here fails a compaction batch.

use sqlx::{PgPool, Postgres, Transaction};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

use crate::services::git_cmd;

pub const MAX_CITATIONS_PER_CANDIDATE: usize = 10;
const MAX_PATH_BYTES: usize = 512;
pub(crate) const GIT_TIMEOUT: Duration = Duration::from_secs(5);

/// A repo-relative path, safe to hand to git as part of a `<rev>:<path>` argument.
pub fn validate_citation_path(raw: &str) -> Result<String, String> {
    let path = raw.trim();
    if path.is_empty() {
        return Err("path is empty".into());
    }
    if path.len() > MAX_PATH_BYTES {
        return Err(format!("path is longer than {MAX_PATH_BYTES} bytes"));
    }
    if path.chars().any(char::is_control) {
        return Err("path contains a control character".into());
    }
    if path.starts_with('/') {
        return Err("path must be relative to the repository".into());
    }
    if path.starts_with('-') {
        return Err("path must not start with `-`".into());
    }
    if path.split('/').any(|part| part == "..") {
        return Err("path must not contain `..`".into());
    }
    Ok(path.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCitation {
    pub path: String,
    pub commit_sha: Option<String>,
}

/// Anchor each path to the head of the repo's default branch. `commit_sha` is
/// that head when the path exists there, otherwise `None`. Never fails: a
/// missing repo, a missing branch, or a git error leaves every sha `None`.
pub async fn resolve_anchors(
    pool: &PgPool,
    repo_id: Option<Uuid>,
    paths: &[String],
) -> Vec<ResolvedCitation> {
    let unanchored = || {
        paths
            .iter()
            .map(|path| ResolvedCitation {
                path: path.clone(),
                commit_sha: None,
            })
            .collect::<Vec<_>>()
    };
    let (Some(repo_id), false) = (repo_id, paths.is_empty()) else {
        return unanchored();
    };
    let repo: Option<(String, String)> =
        sqlx::query_as("SELECT local_path, default_branch FROM repos WHERE id = $1")
            .bind(repo_id)
            .fetch_optional(pool)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "citation anchoring could not read the repo");
                None
            });
    let Some((local_path, branch)) = repo else {
        return unanchored();
    };
    let cwd = Path::new(&local_path);
    let Some(head) = head_sha(cwd, &branch).await else {
        return unanchored();
    };
    let mut resolved = Vec::with_capacity(paths.len());
    for path in paths {
        let exists = git_cmd::output_within(
            Some(cwd),
            &["cat-file", "-e", &format!("{head}:{path}")],
            GIT_TIMEOUT,
        )
        .await
        .is_ok_and(|output| output.status.success());
        resolved.push(ResolvedCitation {
            path: path.clone(),
            commit_sha: exists.then(|| head.clone()),
        });
    }
    resolved
}

pub(crate) async fn head_sha(cwd: &Path, branch: &str) -> Option<String> {
    let rev = format!("{branch}^{{commit}}");
    let output = git_cmd::output_within(
        Some(cwd),
        &["rev-parse", "--verify", "--quiet", "--end-of-options", &rev],
        GIT_TIMEOUT,
    )
    .await
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (sha.len() >= 40 && sha.chars().all(|c| c.is_ascii_hexdigit())).then_some(sha)
}

pub async fn insert_citations(
    tx: &mut Transaction<'_, Postgres>,
    revision_id: Uuid,
    repo_id: Option<Uuid>,
    citations: &[ResolvedCitation],
) -> Result<(), sqlx::Error> {
    for citation in citations {
        sqlx::query(
            r#"
            INSERT INTO knowledge_citations (id, revision_id, repo_id, path, commit_sha)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(revision_id)
        .bind(repo_id)
        .bind(&citation.path)
        .bind(&citation.commit_sha)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Give a new revision the same citations, anchors included, as an earlier one.
pub async fn copy_citations(
    tx: &mut Transaction<'_, Postgres>,
    from_revision_id: Uuid,
    to_revision_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO knowledge_citations (id, revision_id, repo_id, path, commit_sha)
        SELECT gen_random_uuid(), $2, repo_id, path, commit_sha
        FROM knowledge_citations
        WHERE revision_id = $1
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(from_revision_id)
    .bind(to_revision_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_plain_repo_path() {
        assert_eq!(
            validate_citation_path("server/src/lib.rs").unwrap(),
            "server/src/lib.rs"
        );
    }

    #[test]
    fn rejects_unsafe_paths() {
        for bad in [
            "../../etc/passwd",
            "a/../../b",
            "/etc/passwd",
            "--output=x",
            "",
            "   ",
            "a\0b",
        ] {
            assert!(validate_citation_path(bad).is_err(), "{bad:?} was accepted");
        }
        let long = "a".repeat(513);
        assert!(validate_citation_path(&long).is_err());
        assert!(validate_citation_path(&"a".repeat(512)).is_ok());
    }
}
