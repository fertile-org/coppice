//! Reviewer brief carried on a done result and shown in Human Review.
//!
//! Connectors that omit the fields leave no brief. A later verification run
//! does not clear one an implementation run already stored.

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::providers::AgentRunResult;

/// Short on purpose: Human Review shows each field in a couple of lines.
const MAX_CHARS: usize = 280;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewerBrief {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub how_it_works: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes_from_plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look_here_first: Option<String>,
    /// Commit this brief was written for.
    pub commit_sha: String,
}

pub struct ReviewerBriefService<'a> {
    pool: &'a PgPool,
}

impl<'a> ReviewerBriefService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Store a done result's brief for `commit_sha`, or clear it when that
    /// result reported none. Other outcomes leave the stored brief alone.
    pub async fn record(
        &self,
        ticket_id: Uuid,
        result: &AgentRunResult,
        commit_sha: &str,
    ) -> Result<(), sqlx::Error> {
        let AgentRunResult::Done {
            how_it_works,
            changes_from_plan,
            look_here_first,
            ..
        } = result
        else {
            return Ok(());
        };
        match brief_from_parts(
            how_it_works.as_deref(),
            changes_from_plan.as_deref(),
            look_here_first.as_deref(),
        ) {
            Some(brief) => {
                let commit_sha =
                    crate::services::human_review_service::normalize_sha(commit_sha);
                sqlx::query(
                    r#"
                    INSERT INTO ticket_reviewer_briefs (
                        ticket_id, how_it_works, changes_from_plan, look_here_first,
                        commit_sha, updated_at
                    )
                    VALUES ($1, $2, $3, $4, $5, now())
                    ON CONFLICT (ticket_id) DO UPDATE SET
                        how_it_works = EXCLUDED.how_it_works,
                        changes_from_plan = EXCLUDED.changes_from_plan,
                        look_here_first = EXCLUDED.look_here_first,
                        commit_sha = EXCLUDED.commit_sha,
                        updated_at = now()
                    "#,
                )
                .bind(ticket_id)
                .bind(&brief.how_it_works)
                .bind(&brief.changes_from_plan)
                .bind(&brief.look_here_first)
                .bind(&commit_sha)
                .execute(self.pool)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM ticket_reviewer_briefs WHERE ticket_id = $1")
                    .bind(ticket_id)
                    .execute(self.pool)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn get(&self, ticket_id: Uuid) -> Result<Option<ReviewerBrief>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT how_it_works, changes_from_plan, look_here_first, commit_sha
            FROM ticket_reviewer_briefs
            WHERE ticket_id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.as_ref().and_then(row_to_brief))
    }
}

/// Human Review shows a brief only when it was written for the commit that
/// review is bound to. A missing reviewed commit hides the brief.
pub fn brief_matches_reviewed_commit(brief_sha: &str, reviewed_sha: Option<&str>) -> bool {
    let Some(reviewed) = reviewed_sha.map(str::trim).filter(|sha| !sha.is_empty()) else {
        return false;
    };
    crate::services::human_review_service::reviewed_sha_matches(brief_sha, reviewed)
}

pub fn brief_from_parts(
    how_it_works: Option<&str>,
    changes_from_plan: Option<&str>,
    look_here_first: Option<&str>,
) -> Option<ReviewerBrief> {
    let brief = ReviewerBrief {
        how_it_works: clip_field(how_it_works),
        changes_from_plan: clip_field(changes_from_plan),
        look_here_first: clip_field(look_here_first),
        commit_sha: String::new(),
    };
    if brief.how_it_works.is_none()
        && brief.changes_from_plan.is_none()
        && brief.look_here_first.is_none()
    {
        None
    } else {
        Some(brief)
    }
}

fn clip_field(value: Option<&str>) -> Option<String> {
    let text = value?.trim();
    if text.is_empty() {
        return None;
    }
    let clipped: String = text.chars().take(MAX_CHARS).collect();
    Some(clipped)
}

fn row_to_brief(row: &sqlx::postgres::PgRow) -> Option<ReviewerBrief> {
    let mut brief = brief_from_parts(
        row.get::<Option<String>, _>("how_it_works").as_deref(),
        row.get::<Option<String>, _>("changes_from_plan").as_deref(),
        row.get::<Option<String>, _>("look_here_first").as_deref(),
    )?;
    brief.commit_sha =
        crate::services::human_review_service::normalize_sha(&row.get::<String, _>("commit_sha"));
    Some(brief)
}

/// Paths named in an approved plan. A diff file that matches none is outside the plan.
pub fn planned_paths(markdown: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = markdown;
    while let Some(start) = rest.find('`') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('`') else {
            break;
        };
        consider_token(&rest[..end], &mut found);
        rest = &rest[end + 1..];
    }
    for raw in markdown.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '"' | '\'' | '|' | '*'
            )
    }) {
        consider_token(raw, &mut found);
    }
    found
}

pub fn path_in_plan(diff_path: &str, planned: &[String]) -> bool {
    let diff = normalize_path(diff_path);
    if diff.is_empty() {
        return false;
    }
    planned.iter().any(|planned_path| {
        planned_path == &diff
            || diff.ends_with(&format!("/{planned_path}"))
            || planned_path.ends_with(&format!("/{diff}"))
    })
}

/// `true` when `markdown` names paths and `diff_path` is not one of them.
/// A plan that names no files tags nothing.
pub fn file_outside_plan(diff_path: &str, markdown: &str) -> bool {
    let planned = planned_paths(markdown);
    if planned.is_empty() {
        return false;
    }
    !path_in_plan(diff_path, &planned)
}

fn consider_token(raw: &str, found: &mut Vec<String>) {
    let raw = raw.trim().trim_matches('`');
    let raw = raw.trim_end_matches(['.', ':', ',', ';', ')']);
    if raw.is_empty() || raw.contains("://") || !is_path_token(raw) {
        return;
    }
    let normalized = normalize_path(raw);
    if normalized.is_empty() || found.iter().any(|existing| existing == &normalized) {
        return;
    }
    found.push(normalized);
}

fn is_path_token(token: &str) -> bool {
    if token.contains(' ') || token.len() > 240 {
        return false;
    }
    let has_slash = token.contains('/');
    let has_ext = token.rfind('.').is_some_and(|index| {
        let ext = &token[index + 1..];
        let stem = &token[..index];
        !ext.is_empty()
            && ext.len() <= 8
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && stem.chars().any(|c| c.is_ascii_alphanumeric())
    });
    if !has_slash && !has_ext {
        return false;
    }
    token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | '@' | '~'))
}

fn normalize_path(path: &str) -> String {
    let path = path.trim().trim_matches('`').trim();
    let path = path.strip_prefix("./").unwrap_or(path);
    path.trim_start_matches('/').replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn done(extra: serde_json::Value) -> AgentRunResult {
        let mut value = json!({
            "status": "done",
            "summary": "Shipped.",
            "changedFiles": [],
            "testsRun": [],
            "mentionAgents": [],
            "blockers": []
        });
        if let (Some(object), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
            for (key, item) in extra {
                object.insert(key.clone(), item.clone());
            }
        }
        serde_json::from_value(value).expect("done result")
    }

    #[test]
    fn missing_fields_are_not_a_brief() {
        let result = done(json!({}));
        let AgentRunResult::Done {
            how_it_works,
            changes_from_plan,
            look_here_first,
            ..
        } = &result
        else {
            panic!("done");
        };
        assert!(brief_from_parts(
            how_it_works.as_deref(),
            changes_from_plan.as_deref(),
            look_here_first.as_deref(),
        )
        .is_none());
    }

    #[test]
    fn blank_changes_stay_blank_while_other_fields_count() {
        let brief = brief_from_parts(
            Some("  Reads the header.  "),
            Some("   "),
            Some("src/parser.rs"),
        )
        .expect("brief");
        assert_eq!(brief.how_it_works.as_deref(), Some("Reads the header."));
        assert!(brief.changes_from_plan.is_none());
        assert_eq!(brief.look_here_first.as_deref(), Some("src/parser.rs"));
    }

    #[test]
    fn fields_clip_to_a_short_length() {
        let long = "a".repeat(MAX_CHARS + 40);
        let brief = brief_from_parts(Some(&long), None, None).expect("brief");
        assert_eq!(brief.how_it_works.unwrap().chars().count(), MAX_CHARS);
    }

    #[test]
    fn round_trip_keeps_brief_fields_and_old_results_still_parse() {
        let raw = r#"{"status":"done","summary":"ok","howItWorks":"Header first.","changesFromPlan":"Dropped the cache.","lookHereFirst":"src/lib.rs"}"#;
        let result: AgentRunResult = serde_json::from_str(raw).unwrap();
        let stored = serde_json::to_value(&result).unwrap();
        assert_eq!(stored["howItWorks"], "Header first.");
        assert_eq!(stored["changesFromPlan"], "Dropped the cache.");
        assert_eq!(stored["lookHereFirst"], "src/lib.rs");

        let legacy = r#"{"status":"done","summary":"ok","changedFiles":[],"testsRun":[],"blockers":[]}"#;
        let legacy: AgentRunResult = serde_json::from_str(legacy).unwrap();
        let stored = serde_json::to_value(&legacy).unwrap();
        assert!(stored.get("howItWorks").is_none());
    }

    #[test]
    fn planned_paths_match_exact_and_boundary_suffixes() {
        let markdown = "## Plan\n\nTouch `server/src/parser.rs` and web/src/App.tsx.\n";
        let planned = planned_paths(markdown);
        assert!(path_in_plan("server/src/parser.rs", &planned));
        assert!(path_in_plan("src/parser.rs", &planned));
        assert!(path_in_plan("web/src/App.tsx", &planned));
        assert!(!path_in_plan("web/src/Other.tsx", &planned));
        assert!(file_outside_plan("README.md", markdown));
    }

    #[test]
    fn a_plan_that_names_no_files_tags_nothing() {
        let markdown = "## Plan\n\n### Steps\n- [ ] Follow the approach above.\n";
        assert!(planned_paths(markdown).is_empty());
        assert!(!file_outside_plan("src/main.rs", markdown));
        assert!(!file_outside_plan("web/src/App.tsx", markdown));
    }

    #[test]
    fn brief_is_visible_only_for_the_reviewed_commit() {
        let sha = "abc1234deadbeef";
        assert!(brief_matches_reviewed_commit(sha, Some("ABC1234deadbeef")));
        assert!(!brief_matches_reviewed_commit(sha, Some("fff1234deadbeef")));
        assert!(!brief_matches_reviewed_commit(sha, None));
        assert!(!brief_matches_reviewed_commit("", Some(sha)));
    }
}
