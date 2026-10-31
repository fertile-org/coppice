use sqlx::{PgPool, Row};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const ALLOWED_EXT: &[&str] = &["html", "css", "png", "jpg", "jpeg", "gif", "webp"];
pub(crate) const MAX_FILE_BYTES: u64 = 512 * 1024;
pub(crate) const MAX_SET_BYTES: u64 = 2 * 1024 * 1024;

pub fn reject_rel_path(path: &str) -> Option<&'static str> {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return Some("path must be relative");
    }
    if p.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Some("path escapes the design folder");
    }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !ALLOWED_EXT.iter().any(|a| a.eq_ignore_ascii_case(ext)) {
        return Some("file type not allowed");
    }
    None
}

#[derive(Debug, Clone)]
pub struct CreateScope {
    pub ticket_id: Option<Uuid>,
    pub chat_session_id: Option<Uuid>,
    pub run_id: Uuid,
    pub agent_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DesignOption {
    pub id: String,
    pub title: String,
    pub entry: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChosenOption {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedSet {
    pub id: Uuid,
    /// Relative to the artifacts dir: `designs/{id}`.
    pub dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignSetSummary {
    pub id: Uuid,
    pub title: String,
    pub run_id: Uuid,
    pub options: Vec<DesignOption>,
    pub chosen: Option<ChosenOption>,
    pub recommended: Option<ChosenOption>,
    /// The human's choice, else the agent's recommendation.
    pub selected: Option<ChosenOption>,
    pub superseded: bool,
    pub from_plan_run: bool,
}

pub struct DesignSetRecord {
    pub summary: DesignSetSummary,
    pub ticket_id: Option<Uuid>,
    pub chat_session_id: Option<Uuid>,
    pub published: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum DesignError {
    #[error("{0}")]
    Invalid(String),
    #[error("design set not found")]
    NotFound,
    #[error("denied")]
    Denied,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub struct DesignService<'a> {
    pool: &'a PgPool,
    artifacts_dir: PathBuf,
}

impl<'a> DesignService<'a> {
    pub fn new(pool: &'a PgPool, artifacts_dir: impl Into<PathBuf>) -> Self {
        Self {
            pool,
            artifacts_dir: artifacts_dir.into(),
        }
    }

    pub async fn create(&self, title: &str, scope: CreateScope) -> Result<CreatedSet, DesignError> {
        let (ticket_id, chat_session_id) = match (scope.ticket_id, scope.chat_session_id) {
            (Some(ticket_id), None) => (Some(ticket_id), None),
            (None, Some(chat_session_id)) => (None, Some(chat_session_id)),
            _ => {
                return Err(DesignError::Invalid(
                    "design set needs a ticket or a chat, not both".into(),
                ));
            }
        };
        let id = Uuid::new_v4();
        let dir = format!("designs/{id}");
        std::fs::create_dir_all(self.artifacts_dir.join(&dir)).map_err(invalid_io)?;
        sqlx::query(
            r#"
            INSERT INTO design_sets (
                id, title, ticket_id, chat_session_id, run_id, agent_id, dir
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(title)
        .bind(ticket_id)
        .bind(chat_session_id)
        .bind(scope.run_id)
        .bind(scope.agent_id)
        .bind(&dir)
        .execute(self.pool)
        .await?;
        Ok(CreatedSet { id, dir })
    }

    pub async fn write(
        &self,
        set_id: Uuid,
        run_id: Uuid,
        rel: &str,
        bytes: &[u8],
    ) -> Result<(), DesignError> {
        if let Some(reason) = reject_rel_path(rel) {
            return Err(DesignError::Invalid(reason.into()));
        }
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(DesignError::Invalid("file exceeds 512 KiB".into()));
        }
        let row = self.load(set_id).await?;
        let row_run: Uuid = row.get("run_id");
        if row_run != run_id {
            return Err(DesignError::Invalid("run does not match design set".into()));
        }
        let dir: String = row.get("dir");
        let (artifacts, root) = resolve_set_dir(&self.artifacts_dir, &dir)?;
        let dest = dest_under(&artifacts, &root, rel, true)?;
        if dest.is_dir() {
            return Err(DesignError::Invalid("path is a directory".into()));
        }
        let old = if dest.is_file() {
            std::fs::metadata(&dest).map_err(invalid_io)?.len()
        } else {
            0
        };
        let next = dir_size(&root)?
            .saturating_sub(old)
            .saturating_add(bytes.len() as u64);
        if next > MAX_SET_BYTES {
            return Err(DesignError::Invalid("design folder exceeds 2 MB".into()));
        }
        std::fs::write(&dest, bytes).map_err(invalid_io)?;
        Ok(())
    }

    pub async fn publish(
        &self,
        set_id: Uuid,
        run_id: Uuid,
        options: Vec<DesignOption>,
        recommended_option_id: &str,
    ) -> Result<Uuid, DesignError> {
        if !(2..=4).contains(&options.len()) {
            return Err(DesignError::Invalid("publish 2 to 4 options".into()));
        }
        if !options
            .iter()
            .any(|option| option.id == recommended_option_id)
        {
            return Err(DesignError::Invalid(
                "recommendedOptionId must be one of the options".into(),
            ));
        }
        let row = self.load(set_id).await?;
        let row_run: Uuid = row.get("run_id");
        if row_run != run_id {
            return Err(DesignError::Invalid("run does not match design set".into()));
        }
        let dir: String = row.get("dir");
        let ticket_id: Option<Uuid> = row.get("ticket_id");
        let chat_session_id: Option<Uuid> = row.get("chat_session_id");
        let (artifacts, root) = resolve_set_dir(&self.artifacts_dir, &dir)?;
        for option in &options {
            if let Some(reason) = reject_rel_path(&option.entry) {
                return Err(DesignError::Invalid(reason.into()));
            }
            let ext = Path::new(&option.entry)
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("");
            if !ext.eq_ignore_ascii_case("html") {
                return Err(DesignError::Invalid("entry must be html".into()));
            }
            let dest = dest_under(&artifacts, &root, &option.entry, false)?;
            if !dest.is_file() {
                return Err(DesignError::Invalid("missing entry file".into()));
            }
        }
        let options_json =
            serde_json::to_value(&options).map_err(|err| DesignError::Invalid(err.to_string()))?;

        let mut tx = self.pool.begin().await?;
        // One published row per run. A later publish on this run replaces that card.
        sqlx::query(
            r#"
            UPDATE design_sets
            SET published_at = NULL
            WHERE run_id = $1 AND published_at IS NOT NULL AND id <> $2
            "#,
        )
        .bind(run_id)
        .bind(set_id)
        .execute(&mut *tx)
        .await?;
        let updated = sqlx::query(
            r#"
            UPDATE design_sets
            SET options = $1, recommended_option_id = $3,
                published_at = COALESCE(published_at, now())
            WHERE id = $2
            "#,
        )
        .bind(&options_json)
        .bind(set_id)
        .bind(recommended_option_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            tx.rollback().await?;
            return Err(DesignError::NotFound);
        }
        sqlx::query(
            r#"
            UPDATE design_sets
            SET superseded_by = $1
            WHERE id <> $1
              AND published_at IS NOT NULL
              AND created_at < (SELECT created_at FROM design_sets WHERE id = $1)
              AND (
                  ($2::uuid IS NOT NULL AND ticket_id = $2)
                  OR ($3::uuid IS NOT NULL AND chat_session_id = $3)
              )
            "#,
        )
        .bind(set_id)
        .bind(ticket_id)
        .bind(chat_session_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(set_id)
    }

    pub async fn get(&self, set_id: Uuid) -> Result<DesignSetRecord, DesignError> {
        let row = self.load(set_id).await?;
        Ok(DesignSetRecord {
            summary: summary_from_row(&row)?,
            ticket_id: row.get("ticket_id"),
            chat_session_id: row.get("chat_session_id"),
            published: row.get("published"),
        })
    }

    pub async fn read_file(&self, set_id: Uuid, rel: &str) -> Result<Vec<u8>, DesignError> {
        if let Some(reason) = reject_rel_path(rel) {
            return Err(DesignError::Invalid(reason.into()));
        }
        let row = self.load(set_id).await?;
        let dir: String = row.get("dir");
        let (artifacts, root) = resolve_set_dir(&self.artifacts_dir, &dir)?;
        let dest = dest_under(&artifacts, &root, rel, false)?;
        let meta = match std::fs::metadata(&dest) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(DesignError::NotFound);
            }
            Err(err) => return Err(invalid_io(err)),
        };
        if !meta.is_file() {
            return Err(DesignError::NotFound);
        }
        if meta.len() > MAX_FILE_BYTES {
            return Err(DesignError::Invalid("file exceeds 512 KiB".into()));
        }
        std::fs::read(&dest).map_err(invalid_io)
    }

    pub async fn list_for_ticket(
        &self,
        ticket_id: Uuid,
    ) -> Result<Vec<DesignSetSummary>, DesignError> {
        self.list_scoped(Some(ticket_id), None).await
    }

    pub async fn list_for_chat(
        &self,
        chat_session_id: Uuid,
    ) -> Result<Vec<DesignSetSummary>, DesignError> {
        self.list_scoped(None, Some(chat_session_id)).await
    }

    pub async fn choose(
        &self,
        set_id: Uuid,
        option_id: &str,
        user_id: Uuid,
    ) -> Result<(), DesignError> {
        let row = self.load(set_id).await?;
        let options = options_from_row(&row)?;
        if !options.iter().any(|option| option.id == option_id) {
            return Err(DesignError::Invalid("unknown option".into()));
        }
        save_choice(self.pool, set_id, option_id, user_id).await?;
        Ok(())
    }

    async fn list_scoped(
        &self,
        ticket_id: Option<Uuid>,
        chat_session_id: Option<Uuid>,
    ) -> Result<Vec<DesignSetSummary>, DesignError> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, run_id, options, chosen_option_id, recommended_option_id,
                   superseded_by IS NOT NULL AS superseded,
                   EXISTS (
                       SELECT 1 FROM agent_runs r
                       WHERE r.id = design_sets.run_id AND r.job_type = 'plan_ticket'
                   ) AS from_plan_run
            FROM design_sets
            WHERE published_at IS NOT NULL
              AND (
                  ($1::uuid IS NOT NULL AND ticket_id = $1)
                  OR ($2::uuid IS NOT NULL AND chat_session_id = $2)
              )
            ORDER BY created_at DESC, id DESC
            "#,
        )
        .bind(ticket_id)
        .bind(chat_session_id)
        .fetch_all(self.pool)
        .await?;
        rows.iter().map(summary_from_row).collect()
    }

    async fn load(&self, set_id: Uuid) -> Result<sqlx::postgres::PgRow, DesignError> {
        sqlx::query(
            r#"
            SELECT id, title, ticket_id, chat_session_id, run_id, dir, options, chosen_option_id,
                   recommended_option_id,
                   superseded_by IS NOT NULL AS superseded,
                   EXISTS (
                       SELECT 1 FROM agent_runs r
                       WHERE r.id = design_sets.run_id AND r.job_type = 'plan_ticket'
                   ) AS from_plan_run,
                   published_at IS NOT NULL AS published
            FROM design_sets
            WHERE id = $1
            "#,
        )
        .bind(set_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(DesignError::NotFound)
    }
}

/// Store a pick without checking it. Callers validate the option first.
pub(crate) async fn save_choice<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    set_id: Uuid,
    option_id: &str,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE design_sets
        SET chosen_option_id = $1, chosen_by_user_id = $2, chosen_at = now()
        WHERE id = $3
        "#,
    )
    .bind(option_id)
    .bind(user_id)
    .bind(set_id)
    .execute(executor)
    .await?;
    Ok(())
}

fn resolve_set_dir(artifacts_dir: &Path, dir: &str) -> Result<(PathBuf, PathBuf), DesignError> {
    let artifacts = std::fs::canonicalize(artifacts_dir).map_err(invalid_io)?;
    let set_dir = std::fs::canonicalize(artifacts.join(dir)).map_err(invalid_io)?;
    if !set_dir.starts_with(&artifacts) {
        return Err(DesignError::Invalid(
            "path escapes the design folder".into(),
        ));
    }
    Ok((artifacts, set_dir))
}

fn summary_from_row(row: &sqlx::postgres::PgRow) -> Result<DesignSetSummary, DesignError> {
    let options = options_from_row(row)?;
    let find = |id: Option<String>| {
        id.and_then(|id| {
            options
                .iter()
                .find(|option| option.id == id)
                .map(|option| ChosenOption {
                    id: option.id.clone(),
                    title: option.title.clone(),
                })
        })
    };
    let chosen = find(row.get("chosen_option_id"));
    let recommended = find(row.get("recommended_option_id"));
    let selected = chosen.clone().or_else(|| recommended.clone());
    Ok(DesignSetSummary {
        id: row.get("id"),
        title: row.get("title"),
        run_id: row.get("run_id"),
        options,
        chosen,
        recommended,
        selected,
        superseded: row.get("superseded"),
        from_plan_run: row.get("from_plan_run"),
    })
}

fn options_from_row(row: &sqlx::postgres::PgRow) -> Result<Vec<DesignOption>, DesignError> {
    let value: serde_json::Value = row.get("options");
    serde_json::from_value(value).map_err(|err| DesignError::Invalid(err.to_string()))
}

fn dest_under(
    artifacts: &Path,
    set_dir: &Path,
    rel: &str,
    create_parents: bool,
) -> Result<PathBuf, DesignError> {
    if !set_dir.starts_with(artifacts) {
        return Err(DesignError::Invalid(
            "path escapes the design folder".into(),
        ));
    }
    let mut dest = set_dir.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            std::path::Component::CurDir => continue,
            std::path::Component::Normal(part) => dest.push(part),
            _ => {
                return Err(DesignError::Invalid(
                    "path escapes the design folder".into(),
                ));
            }
        }
        if !dest.starts_with(set_dir) {
            return Err(DesignError::Invalid(
                "path escapes the design folder".into(),
            ));
        }
        if std::fs::symlink_metadata(&dest).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(DesignError::Invalid(
                "path escapes the design folder".into(),
            ));
        }
    }
    if dest == set_dir {
        return Err(DesignError::Invalid(
            "path escapes the design folder".into(),
        ));
    }
    let parent = dest
        .parent()
        .ok_or_else(|| DesignError::Invalid("path escapes the design folder".into()))?;
    if !parent.starts_with(set_dir) {
        return Err(DesignError::Invalid(
            "path escapes the design folder".into(),
        ));
    }
    if create_parents {
        std::fs::create_dir_all(parent).map_err(invalid_io)?;
    } else if !parent.exists() {
        return Err(DesignError::Invalid("missing entry file".into()));
    }
    Ok(dest)
}

fn dir_size(root: &Path) -> Result<u64, DesignError> {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(invalid_io)?;
        for entry in entries {
            let entry = entry.map_err(invalid_io)?;
            let kind = entry.file_type().map_err(invalid_io)?;
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                total = total.saturating_add(entry.metadata().map_err(invalid_io)?.len());
            }
        }
    }
    Ok(total)
}

fn invalid_io(err: std::io::Error) -> DesignError {
    DesignError::Invalid(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::{reject_rel_path, resolve_set_dir, DesignError};

    #[test]
    fn reject_rel_path_blocks_escape_and_bad_types() {
        assert_eq!(reject_rel_path("a/index.html"), None);
        assert_eq!(
            reject_rel_path("../x.html"),
            Some("path escapes the design folder")
        );
        assert_eq!(
            reject_rel_path("/tmp/x.html"),
            Some("path must be relative")
        );
        assert_eq!(reject_rel_path("x.svg"), Some("file type not allowed"));
        assert_eq!(reject_rel_path("x.exe"), Some("file type not allowed"));
    }

    #[cfg(unix)]
    #[test]
    fn set_dir_symlink_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let artifacts = tmp.path().join("artifacts");
        std::fs::create_dir_all(artifacts.join("designs")).unwrap();
        let outside = tmp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, artifacts.join("designs").join("set")).unwrap();

        let err = resolve_set_dir(&artifacts, "designs/set").unwrap_err();
        assert!(
            matches!(err, DesignError::Invalid(ref msg) if msg.contains("escapes")),
            "{err}"
        );
    }
}
