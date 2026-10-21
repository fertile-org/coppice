//! Init wizard. Confirm creates one repository, one board, and one starter team.
//!
//! Connector choice reads the cached free sign-in check. This module does not
//! spawn a CLI, send a prompt, or call a model.

use std::collections::HashMap;
use std::path::Path;

use coppice_connectors::sign_in::Readiness;
use coppice_connectors::MOCK;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::repo::verification_status_to_str;
use crate::domain::repo::VerificationStatus;
use crate::services::board_service::slugify;
use crate::services::repo_verifier::{verify_local_path, VerifyResult};
use time::OffsetDateTime;

/// Preset id the wizard sends back on confirm. One team, no picker.
pub const STARTER_PRESET_ID: &str = "starter";

/// Role presets in the starter team, in create order.
pub const STARTER_PRESET_KEYS: &[&str] = &[
    "pm",
    "tech_lead",
    "backend_engineer",
    "frontend_engineer",
    "qc",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectedConnector {
    pub id: &'static str,
    pub display_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detection {
    /// A connector's free check is not in the cache yet.
    Checking,
    /// Every connector has been checked, and none is Ready.
    NoneReady,
    /// Ready connectors, descriptor order. The first is the default.
    Ready(Vec<DetectedConnector>),
}

/// Ready connectors in descriptor order, after every free check is cached.
///
/// `readiness` returns the cached free check: `None` means that check has not
/// finished. Any missing result stays on Checking so the wizard does not offer
/// a partial list. `mock` is skipped. `Found` is not Ready.
pub fn detect_connector(readiness: impl Fn(&str) -> Option<Readiness>) -> Detection {
    let mut ready = Vec::new();
    for descriptor in coppice_connectors::all() {
        if descriptor.id == MOCK {
            continue;
        }
        match readiness(descriptor.id) {
            None => return Detection::Checking,
            Some(Readiness::Ready) => ready.push(DetectedConnector {
                id: descriptor.id,
                display_name: descriptor.display_name,
            }),
            Some(Readiness::NotOnPath | Readiness::FoundNotSignedIn | Readiness::Found) => {}
        }
    }
    if ready.is_empty() {
        Detection::NoneReady
    } else {
        Detection::Ready(ready)
    }
}

#[derive(Debug)]
pub struct InitInput {
    pub preset_id: String,
    pub board_name: String,
    pub repo_name: String,
    pub local_path: String,
    pub remote_url: Option<String>,
    pub default_branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamMemberView {
    pub preset_key: String,
    pub name: String,
    pub role: String,
}

#[derive(Debug)]
pub struct CreatedAgent {
    pub id: Uuid,
    pub name: String,
    pub preset_key: String,
}

#[derive(Debug)]
pub struct CreatedInit {
    pub board_id: Uuid,
    pub repository_id: Uuid,
    pub agents: Vec<CreatedAgent>,
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("connector check still running")]
    Checking,
    #[error("no connector is ready")]
    NoneReady,
    #[error("unknown team preset")]
    UnknownPreset,
    #[error("path is not a git checkout")]
    NotGit,
    #[error("duplicate local path")]
    DuplicatePath,
    #[error("duplicate board name")]
    DuplicateBoard,
    #[error("validation error: {0}")]
    Validation(&'static str),
    #[error("starter preset is not available")]
    PresetUnavailable,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

struct LoadedMember {
    key: String,
    role: String,
    responsibilities: Vec<String>,
    default_plugins: Vec<String>,
    system_prompt: String,
}

struct NewWorkspace<'a> {
    board_name: &'a str,
    repo_name: &'a str,
    local_path: &'a str,
    remote_url: Option<&'a str>,
    default_branch: &'a str,
    verified: &'a VerifyResult,
    members: &'a [LoadedMember],
    connector_id: &'a str,
}

pub struct InitWizardService<'a> {
    pool: &'a PgPool,
    templates: &'a HashMap<String, String>,
}

impl<'a> InitWizardService<'a> {
    pub fn new(pool: &'a PgPool, templates: &'a HashMap<String, String>) -> Self {
        Self { pool, templates }
    }

    pub async fn team_members(&self) -> Result<Vec<TeamMemberView>, InitError> {
        let loaded = self.load_members().await?;
        Ok(loaded
            .into_iter()
            .map(|member| TeamMemberView {
                preset_key: member.key,
                name: member.role.clone(),
                role: member.role,
            })
            .collect())
    }

    /// Insert the board and starter agents. A new folder is registered in the
    /// same transaction. A folder that is already registered is reused, and
    /// that row is left unchanged. Caller has already chosen a Ready
    /// connector. No run is queued.
    pub async fn create(
        &self,
        input: InitInput,
        connector_id: &str,
    ) -> Result<CreatedInit, InitError> {
        if input.preset_id != STARTER_PRESET_ID {
            return Err(InitError::UnknownPreset);
        }
        let board_name = require_text(&input.board_name, "board_name")?;
        let repo_name = require_text(&input.repo_name, "repo_name")?;
        let local_path = require_text(&input.local_path, "local_path")?;
        let default_branch = require_text(&input.default_branch, "default_branch")?;
        let remote_url = input
            .remote_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let verified = verify_local_path(Path::new(&local_path));
        if verified.status != VerificationStatus::Ready {
            return Err(InitError::NotGit);
        }

        let members = self.load_members().await?;
        let workspace = NewWorkspace {
            board_name: &board_name,
            repo_name: &repo_name,
            local_path: &local_path,
            remote_url: remote_url.as_deref(),
            default_branch: &default_branch,
            verified: &verified,
            members: &members,
            connector_id,
        };
        match self.insert_board_and_team(&workspace).await {
            // A concurrent confirm registered the folder first. Retry once
            // and reuse the row that won.
            Err(InitError::DuplicatePath) => self.insert_board_and_team(&workspace).await,
            other => other,
        }
    }

    async fn insert_board_and_team(
        &self,
        workspace: &NewWorkspace<'_>,
    ) -> Result<CreatedInit, InitError> {
        let mut tx = self.pool.begin().await?;

        let repository_id =
            match sqlx::query_scalar::<_, Uuid>("SELECT id FROM repos WHERE local_path = $1")
                .bind(workspace.local_path)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_db)?
            {
                Some(id) => id,
                None => {
                    let repository_id = Uuid::new_v4();
                    let now = OffsetDateTime::now_utc();
                    sqlx::query(
                        r#"
                    INSERT INTO repos (
                        id, name, local_path, remote_url, default_branch,
                        verification_status, verification_error, last_verified_at
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    "#,
                    )
                    .bind(repository_id)
                    .bind(workspace.repo_name)
                    .bind(workspace.local_path)
                    .bind(workspace.remote_url)
                    .bind(workspace.default_branch)
                    .bind(verification_status_to_str(workspace.verified.status))
                    .bind(&workspace.verified.error)
                    .bind(now)
                    .execute(&mut *tx)
                    .await
                    .map_err(map_db)?;
                    repository_id
                }
            };

        let board_id = Uuid::new_v4();
        let slug = slugify(workspace.board_name);
        sqlx::query(
            r#"
            INSERT INTO boards (id, name, slug)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(board_id)
        .bind(workspace.board_name)
        .bind(&slug)
        .execute(&mut *tx)
        .await
        .map_err(map_db)?;

        let mut agents = Vec::with_capacity(workspace.members.len());
        for member in workspace.members {
            let agent_id = Uuid::new_v4();
            sqlx::query(
                r#"
                INSERT INTO agents (
                    id, name, role, responsibilities, system_prompt,
                    connector, model_provider, model, enabled, preset_source
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                "#,
            )
            .bind(agent_id)
            .bind(&member.role)
            .bind(&member.role)
            .bind(&member.responsibilities)
            .bind(&member.system_prompt)
            .bind(workspace.connector_id)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(true)
            .bind(&member.key)
            .execute(&mut *tx)
            .await
            .map_err(map_db)?;

            if !member.default_plugins.is_empty() {
                sqlx::query(
                    r#"
                    INSERT INTO agent_plugins (agent_id, plugin_id)
                    SELECT $1, id FROM plugins
                    WHERE name = ANY($2) AND enabled AND status = 'ok' AND agent_access = 'explicit'
                    ON CONFLICT DO NOTHING
                    "#,
                )
                .bind(agent_id)
                .bind(&member.default_plugins)
                .execute(&mut *tx)
                .await
                .map_err(map_db)?;
            }

            agents.push(CreatedAgent {
                id: agent_id,
                name: member.role.clone(),
                preset_key: member.key.clone(),
            });
        }

        tx.commit().await.map_err(map_db)?;
        Ok(CreatedInit {
            board_id,
            repository_id,
            agents,
        })
    }

    async fn load_members(&self) -> Result<Vec<LoadedMember>, InitError> {
        let mut loaded = Vec::with_capacity(STARTER_PRESET_KEYS.len());
        for key in STARTER_PRESET_KEYS {
            let row = sqlx::query(
                r#"
                SELECT role, responsibilities, default_plugins
                FROM agent_presets
                WHERE key = $1
                "#,
            )
            .bind(key)
            .fetch_optional(self.pool)
            .await?;
            let Some(row) = row else {
                return Err(InitError::PresetUnavailable);
            };
            let Some(system_prompt) = self.templates.get(*key) else {
                return Err(InitError::PresetUnavailable);
            };
            if system_prompt.trim().is_empty() {
                return Err(InitError::PresetUnavailable);
            }
            loaded.push(LoadedMember {
                key: (*key).to_string(),
                role: row.get("role"),
                responsibilities: row.get("responsibilities"),
                default_plugins: row.get("default_plugins"),
                system_prompt: system_prompt.clone(),
            });
        }
        Ok(loaded)
    }
}

fn require_text(value: &str, field: &'static str) -> Result<String, InitError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(InitError::Validation(field));
    }
    Ok(trimmed.to_string())
}

fn map_db(err: sqlx::Error) -> InitError {
    if let sqlx::Error::Database(db_err) = &err {
        match db_err.constraint() {
            Some("repos_local_path_idx") => return InitError::DuplicatePath,
            Some("boards_slug_key" | "projects_slug_key") => return InitError::DuplicateBoard,
            _ => {}
        }
    }
    InitError::Database(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use coppice_connectors::{CLAUDE_CODE, CODEX, CURSOR, KILO_CODE, OPENCODE};

    fn readiness_of<'a>(
        pairs: &'a [(&'a str, Readiness)],
    ) -> impl Fn(&str) -> Option<Readiness> + 'a {
        move |id: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == id)
                .map(|(_, readiness)| *readiness)
        }
    }

    fn checked<'a>(pairs: &'a [(&'a str, Readiness)]) -> Vec<(&'a str, Readiness)> {
        let mut all = pairs.to_vec();
        for id in coppice_connectors::all().iter().map(|d| d.id).filter(|id| *id != MOCK) {
            if !all.iter().any(|(key, _)| *key == id) {
                all.push((id, Readiness::NotOnPath));
            }
        }
        all
    }

    #[test]
    fn a_missing_check_stays_on_checking() {
        let detection = detect_connector(readiness_of(&[(CLAUDE_CODE, Readiness::Ready)]));
        assert_eq!(detection, Detection::Checking);
    }

    #[test]
    fn ready_connectors_follow_descriptor_order_and_skip_mock() {
        let pairs = checked(&[
            (MOCK, Readiness::Ready),
            (CURSOR, Readiness::NotOnPath),
            (CLAUDE_CODE, Readiness::Ready),
            (CODEX, Readiness::Ready),
            (KILO_CODE, Readiness::Found),
        ]);
        let detection = detect_connector(readiness_of(&pairs));
        assert_eq!(
            detection,
            Detection::Ready(vec![
                DetectedConnector {
                    id: CLAUDE_CODE,
                    display_name: "Claude Code",
                },
                DetectedConnector {
                    id: CODEX,
                    display_name: "Codex",
                },
            ])
        );
    }

    #[test]
    fn kilo_found_is_not_a_ready_connector() {
        let detection = detect_connector(readiness_of(&checked(&[
            (CURSOR, Readiness::NotOnPath),
            (CLAUDE_CODE, Readiness::FoundNotSignedIn),
            (CODEX, Readiness::NotOnPath),
            (KILO_CODE, Readiness::Found),
            (OPENCODE, Readiness::NotOnPath),
        ])));
        assert_eq!(detection, Detection::NoneReady);
    }

    #[test]
    fn cursor_is_listed_before_claude_when_both_are_ready() {
        let detection = detect_connector(readiness_of(&checked(&[
            (CURSOR, Readiness::Ready),
            (CLAUDE_CODE, Readiness::Ready),
        ])));
        assert_eq!(
            detection,
            Detection::Ready(vec![
                DetectedConnector {
                    id: CURSOR,
                    display_name: "Cursor",
                },
                DetectedConnector {
                    id: CLAUDE_CODE,
                    display_name: "Claude Code",
                },
            ])
        );
    }
}
