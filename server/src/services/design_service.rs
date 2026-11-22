use crate::domain::Id;
use sqlx::{PgPool, Row};
use std::path::{Path, PathBuf};

use super::design_sketch;

const ALLOWED_EXT: &[&str] = &["html", "css", "png", "jpg", "jpeg", "gif", "webp"];
pub(crate) const MAX_FILE_BYTES: u64 = 512 * 1024;
pub(crate) const MAX_SET_BYTES: u64 = 2 * 1024 * 1024;
pub const MIN_OPTIONS: usize = 2;
pub const MAX_UI_OPTIONS: usize = 4;
pub const MAX_TECHNICAL_OPTIONS: usize = 3;
pub const MAX_TECHNICAL_SETS_PER_PLAN: usize = 2;

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
    pub ticket_id: Option<Id>,
    pub chat_session_id: Option<Id>,
    pub run_id: Id,
    pub agent_id: Id,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignOption {
    pub id: String,
    pub title: String,
    /// HTML entry for UI sets. Empty for technical sets.
    #[serde(default)]
    pub entry: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reversibility: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_out: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChosenOption {
    pub id: String,
    pub title: String,
    /// Position in the set: A, B, C.
    pub label: String,
    /// The title without a leading label or id the agent wrote into it.
    pub display_title: String,
}

/// An option's label is its position in the set, never derived from its id.
pub fn option_label(index: usize) -> String {
    match u8::try_from(index) {
        Ok(i) if i < 26 => char::from(b'A' + i).to_string(),
        _ => (index + 1).to_string(),
    }
}

/// The title with a leading `label` or `id` prefix removed, when that prefix
/// is followed by a separator and whitespace. Unchanged if nothing is left.
pub fn option_display_title(title: &str, label: &str, id: &str) -> String {
    [label, id]
        .into_iter()
        .find_map(|prefix| strip_option_prefix(title, prefix))
        .unwrap_or(title)
        .to_string()
}

fn strip_option_prefix<'a>(title: &'a str, prefix: &str) -> Option<&'a str> {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return None;
    }
    let mut chars = title.char_indices();
    for expected in prefix.chars() {
        let (_, actual) = chars.next()?;
        if !actual.to_lowercase().eq(expected.to_lowercase()) {
            return None;
        }
    }
    let after = chars.next().map_or(title.len(), |(i, _)| i);
    let rest = title[after..]
        .trim_start()
        .strip_prefix([':', '.', '-', '\u{2013}', '\u{2014}', ')'])?;
    if !rest.chars().next()?.is_whitespace() {
        return None;
    }
    let rest = rest.trim();
    (!rest.is_empty()).then_some(rest)
}

pub fn chosen_option(options: &[DesignOption], index: usize) -> ChosenOption {
    let option = &options[index];
    let label = option_label(index);
    let display_title = option_display_title(&option.title, &label, &option.id);
    ChosenOption {
        id: option.id.clone(),
        title: option.title.clone(),
        label,
        display_title,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedSet {
    pub id: Id,
    /// Relative to the artifacts dir: `designs/{id}`.
    pub dir: String,
}

/// What a design set decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignKind {
    Ui,
    Architecture,
    Module,
    Refactor,
    Dependency,
}

impl DesignKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::Architecture => "architecture",
            Self::Module => "module",
            Self::Refactor => "refactor",
            Self::Dependency => "dependency",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "ui" => Some(Self::Ui),
            "architecture" => Some(Self::Architecture),
            "module" => Some(Self::Module),
            "refactor" => Some(Self::Refactor),
            "dependency" => Some(Self::Dependency),
            _ => None,
        }
    }

    /// Architecture, module, or dependency — not UI, and not `refactor` yet.
    pub fn is_technical(self) -> bool {
        matches!(self, Self::Architecture | Self::Module | Self::Dependency)
    }

    pub fn is_publishable(self) -> bool {
        matches!(self, Self::Ui) || self.is_technical()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignSetSummary {
    pub id: Id,
    pub title: String,
    pub kind: DesignKind,
    pub run_id: Id,
    /// Relative to the artifacts dir, as stored.
    pub dir: String,
    pub options: Vec<DesignOption>,
    pub chosen: Option<ChosenOption>,
    pub recommended: Option<ChosenOption>,
    /// Why the agent recommended the option (technical sets).
    pub recommended_reason: Option<String>,
    /// The human's choice, else the agent's recommendation.
    pub selected: Option<ChosenOption>,
    pub superseded: bool,
    pub from_plan_run: bool,
    /// The set belongs to the parent ticket, not this one.
    pub inherited: bool,
}

/// The design a ticket's work should follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveDesign {
    pub set_id: Id,
    pub title: String,
    pub selected: ChosenOption,
    /// Absolute path of the selected option's entry file. None when the set's
    /// folder can't be resolved: the title still tells the agent what to build.
    pub entry_path: Option<PathBuf>,
}

pub struct DesignSetRecord {
    pub summary: DesignSetSummary,
    pub ticket_id: Option<Id>,
    pub chat_session_id: Option<Id>,
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

    pub async fn create(
        &self,
        title: &str,
        kind: DesignKind,
        scope: CreateScope,
    ) -> Result<CreatedSet, DesignError> {
        if !kind.is_publishable() {
            return Err(DesignError::Invalid(format!(
                "design kind `{}` cannot be created yet",
                kind.as_str()
            )));
        }
        let (ticket_id, chat_session_id) = match (scope.ticket_id, scope.chat_session_id) {
            (Some(ticket_id), None) => (Some(ticket_id), None),
            (None, Some(chat_session_id)) => (None, Some(chat_session_id)),
            _ => {
                return Err(DesignError::Invalid(
                    "design set needs a ticket or a chat, not both".into(),
                ));
            }
        };
        let id = Id::new();
        let dir = format!("designs/{id}");
        std::fs::create_dir_all(self.artifacts_dir.join(&dir)).map_err(invalid_io)?;
        sqlx::query(
            r#"
            INSERT INTO design_sets (
                id, title, kind, ticket_id, chat_session_id, run_id, agent_id, dir
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(title)
        .bind(kind.as_str())
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
        set_id: Id,
        run_id: Id,
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
        let row_run: Id = row.get("run_id");
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
        set_id: Id,
        run_id: Id,
        options: Vec<DesignOption>,
        recommended_option_id: &str,
        recommended_reason: Option<&str>,
    ) -> Result<Id, DesignError> {
        let row = self.load(set_id).await?;
        let row_run: Id = row.get("run_id");
        if row_run != run_id {
            return Err(DesignError::Invalid("run does not match design set".into()));
        }
        let kind_raw: String = row.get("kind");
        let kind = DesignKind::parse(&kind_raw).ok_or_else(|| {
            DesignError::Invalid(format!("unknown design kind {kind_raw}"))
        })?;
        if !kind.is_publishable() {
            return Err(DesignError::Invalid(format!(
                "design kind `{}` cannot be published yet",
                kind.as_str()
            )));
        }
        if !options
            .iter()
            .any(|option| option.id == recommended_option_id)
        {
            return Err(DesignError::Invalid(
                "recommendedOptionId must be one of the options".into(),
            ));
        }

        let reason = match kind {
            DesignKind::Ui => {
                validate_ui_options(&self.artifacts_dir, row.get("dir"), &options)?;
                None
            }
            _ => {
                let reason = validate_technical_options(&options, recommended_reason)?;
                Some(reason)
            }
        };
        let options_json =
            serde_json::to_value(&options).map_err(|err| DesignError::Invalid(err.to_string()))?;

        let ticket_id: Option<Id> = row.get("ticket_id");
        let chat_session_id: Option<Id> = row.get("chat_session_id");
        let mut tx = self.pool.begin().await?;
        // One published row per (run, kind). A later publish replaces that card.
        sqlx::query(
            r#"
            UPDATE design_sets
            SET published_at = NULL
            WHERE run_id = $1 AND kind = $3 AND published_at IS NOT NULL AND id <> $2
            "#,
        )
        .bind(run_id)
        .bind(set_id)
        .bind(kind.as_str())
        .execute(&mut *tx)
        .await?;
        let updated = sqlx::query(
            r#"
            UPDATE design_sets
            SET options = $1, recommended_option_id = $3, recommended_reason = $4,
                published_at = COALESCE(published_at, now())
            WHERE id = $2
            "#,
        )
        .bind(&options_json)
        .bind(set_id)
        .bind(recommended_option_id)
        .bind(reason.as_deref())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            tx.rollback().await?;
            return Err(DesignError::NotFound);
        }
        // UI still supersedes by kind. Technical sets supersede by plan marker
        // when the plan is saved (see PlanningService::insert_plan).
        if kind == DesignKind::Ui {
            sqlx::query(
                r#"
                UPDATE design_sets
                SET superseded_by = $1
                WHERE id <> $1
                  AND published_at IS NOT NULL
                  AND kind = $5
                  AND created_at < (SELECT created_at FROM design_sets WHERE id = $1)
                  AND (EXISTS (
                          SELECT 1 FROM agent_runs r
                          WHERE r.id = design_sets.run_id AND r.job_type = 'plan_ticket'
                      )) = (EXISTS (
                          SELECT 1 FROM agent_runs r
                          WHERE r.id = $4 AND r.job_type = 'plan_ticket'
                      ))
                  AND (
                      ($2::varchar IS NOT NULL AND ticket_id = $2)
                      OR ($3::varchar IS NOT NULL AND chat_session_id = $3)
                  )
                "#,
            )
            .bind(set_id)
            .bind(ticket_id)
            .bind(chat_session_id)
            .bind(run_id)
            .bind(kind.as_str())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(set_id)
    }

    /// Count technical sets published on this run (for the plan set cap).
    pub async fn technical_sets_on_run(&self, run_id: Id) -> Result<usize, DesignError> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM design_sets
            WHERE run_id = $1
              AND published_at IS NOT NULL
              AND kind IN ('architecture', 'module', 'dependency')
            "#,
        )
        .bind(run_id)
        .fetch_one(self.pool)
        .await?;
        Ok(count as usize)
    }

    pub async fn get(&self, set_id: Id) -> Result<DesignSetRecord, DesignError> {
        let row = self.load(set_id).await?;
        Ok(DesignSetRecord {
            summary: summary_from_row(&row)?,
            ticket_id: row.get("ticket_id"),
            chat_session_id: row.get("chat_session_id"),
            published: row.get("published"),
        })
    }

    pub async fn read_file(&self, set_id: Id, rel: &str) -> Result<Vec<u8>, DesignError> {
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
        ticket_id: Id,
    ) -> Result<Vec<DesignSetSummary>, DesignError> {
        let mut sets = self.list_scoped(Some(ticket_id), None).await?;
        if let Some(parent_id) = self.parent_of(ticket_id).await? {
            if let Some(mut parent_set) = self.own_effective(parent_id).await? {
                parent_set.inherited = true;
                sets.push(parent_set);
            }
        }
        Ok(sets)
    }

    /// The design a ticket's work follows: its own, else its parent's.
    pub async fn effective_for_ticket(
        &self,
        ticket_id: Id,
    ) -> Result<Option<EffectiveDesign>, DesignError> {
        let mut set = self.own_effective(ticket_id).await?;
        if set.is_none() {
            if let Some(parent_id) = self.parent_of(ticket_id).await? {
                set = self.own_effective(parent_id).await?;
            }
        }
        let Some(set) = set else {
            return Ok(None);
        };
        let Some(selected) = set.selected.clone() else {
            return Ok(None);
        };
        let Some(option) = set.options.iter().find(|option| option.id == selected.id) else {
            return Ok(None);
        };
        let entry_path = match resolve_set_dir(&self.artifacts_dir, &set.dir) {
            Ok((_, root)) => Some(root.join(&option.entry)),
            Err(err) => {
                tracing::warn!(
                    set_id = %set.id,
                    dir = %set.dir,
                    error = %err,
                    "design set folder unavailable"
                );
                None
            }
        };
        Ok(Some(EffectiveDesign {
            set_id: set.id,
            title: set.title,
            selected,
            entry_path,
        }))
    }

    async fn parent_of(&self, ticket_id: Id) -> Result<Option<Id>, DesignError> {
        let parent: Option<Option<Id>> =
            sqlx::query_scalar("SELECT parent_ticket_id FROM tickets WHERE id = $1")
                .bind(ticket_id)
                .fetch_optional(self.pool)
                .await?;
        Ok(parent.flatten())
    }

    /// Newest live set a person picked from outside a plan run, else the newest
    /// live plan-run set with a selection. Sets are newest first.
    async fn own_effective(
        &self,
        ticket_id: Id,
    ) -> Result<Option<DesignSetSummary>, DesignError> {
        let sets = self.list_scoped(Some(ticket_id), None).await?;
        let live = |set: &&DesignSetSummary| !set.superseded;
        let picked_in_work = sets
            .iter()
            .filter(live)
            .find(|set| !set.from_plan_run && set.chosen.is_some());
        let found = picked_in_work.or_else(|| {
            sets.iter()
                .filter(live)
                .find(|set| set.from_plan_run && set.selected.is_some())
        });
        Ok(found.cloned())
    }

    pub async fn list_for_chat(
        &self,
        chat_session_id: Id,
    ) -> Result<Vec<DesignSetSummary>, DesignError> {
        self.list_scoped(None, Some(chat_session_id)).await
    }

    pub async fn choose(
        &self,
        set_id: Id,
        option_id: &str,
        user_id: Id,
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
        ticket_id: Option<Id>,
        chat_session_id: Option<Id>,
    ) -> Result<Vec<DesignSetSummary>, DesignError> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, kind, run_id, dir, options, chosen_option_id, recommended_option_id,
                   recommended_reason,
                   (superseded_by IS NOT NULL OR retired_at IS NOT NULL) AS superseded,
                   EXISTS (
                       SELECT 1 FROM agent_runs r
                       WHERE r.id = design_sets.run_id AND r.job_type = 'plan_ticket'
                   ) AS from_plan_run
            FROM design_sets
            WHERE published_at IS NOT NULL
              AND (
                  ($1::varchar IS NOT NULL AND ticket_id = $1)
                  OR ($2::varchar IS NOT NULL AND chat_session_id = $2)
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

    async fn load(&self, set_id: Id) -> Result<sqlx::postgres::PgRow, DesignError> {
        sqlx::query(
            r#"
            SELECT id, title, kind, ticket_id, chat_session_id, run_id, dir, options, chosen_option_id,
                   recommended_option_id, recommended_reason,
                   (superseded_by IS NOT NULL OR retired_at IS NOT NULL) AS superseded,
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
    set_id: Id,
    option_id: &str,
    user_id: Id,
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
                .position(|option| option.id == id)
                .map(|index| chosen_option(&options, index))
        })
    };
    let chosen = find(row.get("chosen_option_id"));
    let recommended = find(row.get("recommended_option_id"));
    let selected = chosen.clone().or_else(|| recommended.clone());
    let kind_raw: String = row.get("kind");
    let kind = DesignKind::parse(&kind_raw).ok_or_else(|| {
        DesignError::Invalid(format!("unknown design kind {kind_raw}"))
    })?;
    let recommended_reason: Option<String> = row.try_get("recommended_reason").ok().flatten();
    Ok(DesignSetSummary {
        id: row.get("id"),
        title: row.get("title"),
        kind,
        run_id: row.get("run_id"),
        dir: row.get("dir"),
        options,
        chosen,
        recommended,
        recommended_reason,
        selected,
        superseded: row.get("superseded"),
        from_plan_run: row.get("from_plan_run"),
        inherited: false,
    })
}

fn require_field(label: &str, value: Option<&str>) -> Result<String, DesignError> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DesignError::Invalid(format!("{label} is required")))
}

fn validate_ui_options(
    artifacts_dir: &Path,
    dir: String,
    options: &[DesignOption],
) -> Result<(), DesignError> {
    if !(MIN_OPTIONS..=MAX_UI_OPTIONS).contains(&options.len()) {
        return Err(DesignError::Invalid("publish 2 to 4 options".into()));
    }
    let (artifacts, root) = resolve_set_dir(artifacts_dir, &dir)?;
    for option in options {
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
    Ok(())
}

fn validate_technical_options(
    options: &[DesignOption],
    recommended_reason: Option<&str>,
) -> Result<String, DesignError> {
    if !(MIN_OPTIONS..=MAX_TECHNICAL_OPTIONS).contains(&options.len()) {
        return Err(DesignError::Invalid(
            "publish 2 or 3 technical options".into(),
        ));
    }
    let reason = require_field("recommendedReason", recommended_reason)?;
    for option in options {
        if !option.entry.trim().is_empty() {
            return Err(DesignError::Invalid(
                "technical options must not set entry".into(),
            ));
        }
        require_field("changes", option.changes.as_deref())?;
        require_field("effort", option.effort.as_deref())?;
        require_field("risk", option.risk.as_deref())?;
        require_field("reversibility", option.reversibility.as_deref())?;
        require_field("rulesOut", option.rules_out.as_deref())?;
        design_sketch::validate_sketch(option.sketch.as_deref())
            .map_err(DesignError::Invalid)?;
    }
    Ok(reason)
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
    use super::{
        chosen_option, option_display_title, option_label, reject_rel_path, resolve_set_dir,
        validate_technical_options, DesignError, DesignKind, DesignOption,
    };

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

    #[test]
    fn labels_follow_position_not_id() {
        assert_eq!(option_label(0), "A");
        assert_eq!(option_label(1), "B");
        assert_eq!(option_label(3), "D");
    }

    #[test]
    fn display_title_strips_its_own_label_or_id() {
        let t = option_display_title;
        assert_eq!(t("A: Quiet button", "A", "option-a"), "Quiet button");
        assert_eq!(t("a. Quiet", "A", "x"), "Quiet");
        assert_eq!(t("A) Quiet", "A", "x"), "Quiet");
        assert_eq!(t("A - Quiet", "A", "x"), "Quiet");
        assert_eq!(t("A \u{2013} Quiet", "A", "x"), "Quiet");
        assert_eq!(t("A \u{2014} Quiet", "A", "x"), "Quiet");
        assert_eq!(t("option-a - Quiet", "A", "option-a"), "Quiet");
        assert_eq!(t("Option-A: Quiet", "B", "option-a"), "Quiet");
        assert_eq!(t("A/B testing panel", "A", "option-a"), "A/B testing panel");
        assert_eq!(t("Apple: red", "A", "option-a"), "Apple: red");
        assert_eq!(t("A:Quiet", "A", "x"), "A:Quiet");
        assert_eq!(t("A: ", "A", "x"), "A: ");
        assert_eq!(t("A:", "A", "x"), "A:");
        assert_eq!(t("Quiet button", "A", "a"), "Quiet button");
        assert_eq!(t("B: Quiet", "A", "a"), "B: Quiet");
    }

    #[test]
    fn chosen_option_carries_label_and_display_title() {
        let options = vec![
            DesignOption {
                id: "option-a".into(),
                title: "A: Quiet".into(),
                entry: "a/index.html".into(),
                ..Default::default()
            },
            DesignOption {
                id: "option-b".into(),
                title: "B: Full".into(),
                entry: "b/index.html".into(),
                ..Default::default()
            },
        ];
        let b = chosen_option(&options, 1);
        assert_eq!((b.label.as_str(), b.display_title.as_str()), ("B", "Full"));
        assert_eq!(b.title, "B: Full");
    }

    fn tech_option(id: &str) -> DesignOption {
        DesignOption {
            id: id.into(),
            title: id.to_uppercase(),
            changes: Some("server/src/x.rs".into()),
            effort: Some("S".into()),
            risk: Some("Low".into()),
            reversibility: Some("Easy".into()),
            rules_out: Some("Nothing".into()),
            sketch: Some("flowchart LR\n  A --> B".into()),
            ..Default::default()
        }
    }

    #[test]
    fn technical_publish_requires_fields_reason_and_two_or_three_options() {
        let ok = validate_technical_options(&[tech_option("a"), tech_option("b")], Some("fits"));
        assert_eq!(ok.unwrap(), "fits");

        let four = vec![
            tech_option("a"),
            tech_option("b"),
            tech_option("c"),
            tech_option("d"),
        ];
        assert!(validate_technical_options(&four, Some("x"))
            .unwrap_err()
            .to_string()
            .contains("2 or 3"));

        assert!(validate_technical_options(&[tech_option("a"), tech_option("b")], None)
            .unwrap_err()
            .to_string()
            .contains("recommendedReason"));

        let mut with_entry = tech_option("a");
        with_entry.entry = "a.html".into();
        assert!(validate_technical_options(&[with_entry, tech_option("b")], Some("x"))
            .unwrap_err()
            .to_string()
            .contains("must not set entry"));

        let mut bad_sketch = tech_option("a");
        bad_sketch.sketch = Some("%%{init: {}}%%\nflowchart LR\nA-->B".into());
        assert!(validate_technical_options(&[bad_sketch, tech_option("b")], Some("x"))
            .unwrap_err()
            .to_string()
            .contains("init"));
    }

    #[test]
    fn only_ui_and_technical_kinds_are_publishable() {
        assert!(DesignKind::Ui.is_publishable());
        assert!(DesignKind::Architecture.is_publishable());
        assert!(DesignKind::Module.is_publishable());
        assert!(DesignKind::Dependency.is_publishable());
        assert!(!DesignKind::Refactor.is_publishable());
        assert!(!DesignKind::Refactor.is_technical());
        assert!(DesignKind::Architecture.is_technical());
    }
}
