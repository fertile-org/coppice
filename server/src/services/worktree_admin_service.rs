//! List and remove ticket worktrees across all repositories (Repositories → Worktrees).
use crate::domain::Id;

use crate::domain::substatus::TicketStatus;
use crate::domain::ticket::{status_from_str, status_to_str};
use crate::services::git_flow::{create_git_action_comment, Actor};
use crate::services::git_state_service::GitStateService;
use crate::services::worktree_service::{compute_paths, worktree_dirty_excluding_agent};
use crate::AppState;
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
const SIZE_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const DIRTY_CHECK_CONCURRENCY: usize = 8;
const SIZE_WALK_CONCURRENCY: usize = 4;

#[derive(Debug, Clone)]
struct CachedSize {
    bytes: u64,
    measured_at: Instant,
}

/// In-process worktree size cache. List returns `null` until a background walk fills it.
pub struct WorktreeSizeCache {
    entries: RwLock<HashMap<String, CachedSize>>,
    in_flight: Mutex<HashSet<String>>,
    walk_slots: std::sync::Arc<tokio::sync::Semaphore>,
}

impl Default for WorktreeSizeCache {
    fn default() -> Self {
        Self::new()
    }
}

impl WorktreeSizeCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            walk_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(SIZE_WALK_CONCURRENCY)),
        }
    }

    /// Cached bytes if any, and whether a background remeasure should start.
    /// Stale entries still return their last value so the SPA does not flash "…".
    fn get_for_list(&self, folder: &str) -> (Option<u64>, bool) {
        let guard = self.entries.read().unwrap_or_else(|e| e.into_inner());
        match guard.get(folder) {
            Some(cached) if cached.measured_at.elapsed() > SIZE_CACHE_TTL => {
                (Some(cached.bytes), true)
            }
            Some(cached) => (Some(cached.bytes), false),
            None => (None, true),
        }
    }

    fn store(&self, folder: String, bytes: u64) {
        self.entries
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                folder,
                CachedSize {
                    bytes,
                    measured_at: Instant::now(),
                },
            );
    }

    pub fn invalidate(&self, folder: &str) {
        self.entries
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(folder);
        self.in_flight
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(folder);
    }

    /// Start a single-flight measurement when the cache misses or is stale.
    pub fn ensure_measuring(self: &std::sync::Arc<Self>, root: PathBuf, folder: String) {
        {
            let mut in_flight = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
            if !in_flight.insert(folder.clone()) {
                return;
            }
        }
        let cache = std::sync::Arc::clone(self);
        let slots = std::sync::Arc::clone(&self.walk_slots);
        tokio::spawn(async move {
            let Ok(_permit) = slots.acquire().await else {
                cache
                    .in_flight
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&folder);
                return;
            };
            let path = root.join(&folder);
            let measured = tokio::task::spawn_blocking(move || dir_size_bytes(&path)).await;
            match measured {
                Ok(bytes) => cache.store(folder.clone(), bytes),
                Err(err) => {
                    tracing::warn!(folder = %folder, error = %err, "worktree size task failed");
                }
            }
            cache
                .in_flight
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&folder);
        });
    }
}

/// Sum file sizes under `path`. Skip entries that error (unreadable dirs, races).
fn dir_size_bytes(path: &Path) -> u64 {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let Ok(entry) = entry else {
            continue;
        };
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

/// `TICKET-<8 hex>-<slug>` — skips `chat/`, `plan-scratch/`, `plugin-builds/`, `knowledge-compaction/`.
fn is_ticket_worktree_folder(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("TICKET-") else {
        return false;
    };
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return false;
    }
    let Some((short, slug)) = rest.split_once('-') else {
        return false;
    };
    short.len() == 8
        && short.chars().all(|c| c.is_ascii_hexdigit())
        && !slug.is_empty()
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeAdminEntry {
    pub folder_name: String,
    pub path: String,
    pub repo_id: Option<Id>,
    pub repo_name: Option<String>,
    pub ticket_id: Option<Id>,
    pub ticket_title: Option<String>,
    pub ticket_status: Option<String>,
    pub archived: bool,
    pub merged: bool,
    pub has_uncommitted_changes: Option<bool>,
    pub has_active_run: bool,
    pub last_used_at: String,
    pub size_bytes: Option<u64>,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeListResponse {
    pub worktrees: Vec<WorktreeAdminEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoveStatus {
    Removed,
    ActiveRun,
    NotFound,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveResult {
    pub folder_name: String,
    pub status: RemoveStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveResponse {
    pub results: Vec<RemoveResult>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorktreeAdminError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

struct TicketMatch {
    ticket_id: Id,
    title: String,
    status: TicketStatus,
    archived: bool,
    merged: bool,
    repo_id: Id,
    repo_name: String,
    last_run_at: Option<OffsetDateTime>,
    expected_dir: PathBuf,
}

pub struct WorktreeAdminService<'a> {
    state: &'a AppState,
    pool: &'a PgPool,
}

impl<'a> WorktreeAdminService<'a> {
    pub fn new(state: &'a AppState, pool: &'a PgPool) -> Self {
        Self { state, pool }
    }

    pub async fn list(&self) -> Result<WorktreeListResponse, WorktreeAdminError> {
        let root = PathBuf::from(&self.state.config.agent.worktrees_path);
        let folders = scan_ticket_folders(&root).await?;
        let matches = self.load_ticket_matches(&root).await?;
        let active = self.load_active_runs().await?;

        let mut path_to_ticket: HashMap<PathBuf, &TicketMatch> = HashMap::new();
        for m in &matches {
            path_to_ticket.insert(m.expected_dir.clone(), m);
        }

        let mut entries: Vec<(WorktreeAdminEntry, PathBuf)> = Vec::with_capacity(folders.len());
        for folder in &folders {
            let path = root.join(folder);
            let matched = path_to_ticket.get(&path).copied();
            let has_active_run = matched
                .map(|m| active.ticket_ids.contains(&m.ticket_id))
                .unwrap_or(false)
                || active
                    .paths
                    .iter()
                    .any(|p| paths_equal(p.as_path(), path.as_path()));

            let last_used_at = matched
                .and_then(|m| m.last_run_at)
                .map(|t| t.format(&Rfc3339).unwrap_or_else(|_| t.to_string()))
                .unwrap_or_else(|| folder_mtime_rfc3339(&path));

            let (size_bytes, refresh) = self.state.worktree_sizes.get_for_list(folder);
            if refresh {
                self.state
                    .worktree_sizes
                    .ensure_measuring(root.clone(), folder.clone());
            }

            let entry = WorktreeAdminEntry {
                folder_name: folder.clone(),
                path: path.to_string_lossy().into_owned(),
                repo_id: matched.map(|m| m.repo_id),
                repo_name: matched.map(|m| m.repo_name.clone()),
                ticket_id: matched.map(|m| m.ticket_id),
                ticket_title: matched.map(|m| m.title.clone()),
                ticket_status: matched.map(|m| status_to_str(m.status).to_string()),
                archived: matched.map(|m| m.archived).unwrap_or(false),
                merged: matched.map(|m| m.merged).unwrap_or(false),
                has_uncommitted_changes: None,
                has_active_run,
                last_used_at,
                size_bytes,
                finished: false,
            };
            entries.push((entry, path));
        }

        fill_dirty_flags(&mut entries).await;

        let worktrees = entries
            .into_iter()
            .map(|(mut entry, _)| {
                entry.finished = compute_finished(&entry);
                entry
            })
            .collect();

        Ok(WorktreeListResponse { worktrees })
    }

    pub async fn remove(
        &self,
        folder_names: &[String],
        user_id: Id,
    ) -> Result<RemoveResponse, WorktreeAdminError> {
        let root = PathBuf::from(&self.state.config.agent.worktrees_path);
        let matches = self.load_ticket_matches(&root).await?;
        let mut path_to_ticket: HashMap<PathBuf, &TicketMatch> = HashMap::new();
        for m in &matches {
            path_to_ticket.insert(m.expected_dir.clone(), m);
        }

        let mut results = Vec::with_capacity(folder_names.len());
        for name in folder_names {
            results.push(self.remove_one(&root, name, user_id, &path_to_ticket).await);
        }
        Ok(RemoveResponse { results })
    }

    async fn remove_one(
        &self,
        root: &Path,
        folder_name: &str,
        user_id: Id,
        path_to_ticket: &HashMap<PathBuf, &TicketMatch>,
    ) -> RemoveResult {
        if !is_ticket_worktree_folder(folder_name) {
            return RemoveResult {
                folder_name: folder_name.to_string(),
                status: RemoveStatus::Error,
                message: Some("invalid worktree folder name".into()),
            };
        }

        let path = root.join(folder_name);
        let canonical = match canonicalize_under_root(root, &path) {
            Ok(p) => p,
            Err(_) => {
                return RemoveResult {
                    folder_name: folder_name.to_string(),
                    status: RemoveStatus::NotFound,
                    message: None,
                };
            }
        };

        if !canonical.join(".git").exists() {
            return RemoveResult {
                folder_name: folder_name.to_string(),
                status: RemoveStatus::NotFound,
                message: None,
            };
        }

        let matched = path_to_ticket.get(&canonical).copied().or_else(|| {
            path_to_ticket
                .iter()
                .find(|(p, _)| paths_equal(p, &canonical))
                .map(|(_, m)| *m)
        });

        match self.has_active_run_for(&canonical, matched.map(|m| m.ticket_id)).await {
            Ok(true) => {
                return RemoveResult {
                    folder_name: folder_name.to_string(),
                    status: RemoveStatus::ActiveRun,
                    message: Some("an agent is working in this worktree".into()),
                };
            }
            Ok(false) => {}
            Err(err) => {
                return RemoveResult {
                    folder_name: folder_name.to_string(),
                    status: RemoveStatus::Error,
                    message: Some(err.to_string()),
                };
            }
        }

        if let Err(err) = remove_worktree_path(&canonical).await {
            return RemoveResult {
                folder_name: folder_name.to_string(),
                status: RemoveStatus::Error,
                message: Some(err),
            };
        }

        self.state.worktree_sizes.invalidate(folder_name);

        if let Some(matched) = matched {
            let comment_body = format!("**Worktree removed:** `{}`", canonical.display());
            if let Err(err) = create_git_action_comment(
                self.state,
                self.pool,
                matched.ticket_id,
                Actor::User(user_id),
                &comment_body,
            )
            .await
            {
                tracing::warn!(
                    ticket_id = %matched.ticket_id,
                    error = %err,
                    "failed to post worktree-removed comment"
                );
            }
            if let Err(err) = GitStateService::new(self.state, self.pool)
                .recheck(matched.ticket_id)
                .await
            {
                tracing::warn!(
                    ticket_id = %matched.ticket_id,
                    error = %err,
                    "failed to recheck git state after worktree removal"
                );
            }
            crate::events::publish_ticket_state(
                &self.state.event_bus,
                self.pool,
                matched.ticket_id,
            )
            .await;
        }

        RemoveResult {
            folder_name: folder_name.to_string(),
            status: RemoveStatus::Removed,
            message: None,
        }
    }

    async fn has_active_run_for(
        &self,
        path: &Path,
        ticket_id: Option<Id>,
    ) -> Result<bool, sqlx::Error> {
        let path_str = path.to_string_lossy().into_owned();
        let by_path: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM agent_runs
                WHERE status IN ('queued', 'running')
                  AND worktree_path IS NOT NULL
                  AND worktree_path = $1
            )
            "#,
        )
        .bind(&path_str)
        .fetch_one(self.pool)
        .await?;
        if by_path {
            return Ok(true);
        }
        let Some(ticket_id) = ticket_id else {
            return Ok(false);
        };
        sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM agent_runs
                WHERE ticket_id = $1 AND status IN ('queued', 'running')
            )
            "#,
        )
        .bind(ticket_id)
        .fetch_one(self.pool)
        .await
    }

    async fn load_ticket_matches(
        &self,
        root: &Path,
    ) -> Result<Vec<TicketMatch>, WorktreeAdminError> {
        let rows = sqlx::query(
            r#"
            SELECT
                t.id AS ticket_id,
                t.title,
                t.status,
                t.archived_at,
                r.id AS repo_id,
                r.name AS repo_name,
                (SELECT MAX(COALESCE(ar.ended_at, ar.started_at, ar.created_at))
                   FROM agent_runs ar WHERE ar.ticket_id = t.id) AS last_run_at,
                EXISTS(
                    SELECT 1 FROM ticket_human_reviews thr
                    WHERE thr.ticket_id = t.id AND thr.merged_at IS NOT NULL
                ) AS merged
            FROM tickets t
            JOIN repos r ON r.id = t.repo_id
            WHERE t.repo_id IS NOT NULL
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let ticket_id: Id = row.get("ticket_id");
            let title: String = row.get("title");
            let status_raw: String = row.get("status");
            let status = status_from_str(&status_raw).unwrap_or(TicketStatus::Backlog);
            let archived_at: Option<OffsetDateTime> = row.get("archived_at");
            let repo_id: Id = row.get("repo_id");
            let repo_name: String = row.get("repo_name");
            let last_run_at: Option<OffsetDateTime> = row.get("last_run_at");
            let merged: bool = row.get("merged");
            let expected_dir = compute_paths(root, &repo_name, ticket_id).worktree_dir;
            out.push(TicketMatch {
                ticket_id,
                title,
                status,
                archived: archived_at.is_some(),
                merged,
                repo_id,
                repo_name,
                last_run_at,
                expected_dir,
            });
        }
        Ok(out)
    }

    async fn load_active_runs(&self) -> Result<ActiveRuns, WorktreeAdminError> {
        let rows = sqlx::query(
            r#"
            SELECT ticket_id, worktree_path
            FROM agent_runs
            WHERE status IN ('queued', 'running')
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        let mut ticket_ids = HashSet::new();
        let mut paths = HashSet::new();
        for row in rows {
            // Chat runs have ticket_id NULL (migration 020); Row::get panics on NULL.
            let ticket_id: Option<Id> = row.get("ticket_id");
            if let Some(ticket_id) = ticket_id {
                ticket_ids.insert(ticket_id);
            }
            let worktree_path: Option<String> = row.get("worktree_path");
            if let Some(p) = worktree_path {
                if !p.is_empty() {
                    paths.insert(PathBuf::from(p));
                }
            }
        }
        Ok(ActiveRuns { ticket_ids, paths })
    }
}

struct ActiveRuns {
    ticket_ids: HashSet<Id>,
    paths: HashSet<PathBuf>,
}

async fn scan_ticket_folders(root: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    let mut entries = tokio::fs::read_dir(root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_ticket_worktree_folder(&name) {
            continue;
        }
        let path = entry.path();
        if !path.join(".git").exists() {
            continue;
        }
        out.push(name);
    }
    out.sort();
    Ok(out)
}

async fn fill_dirty_flags(entries: &mut [(WorktreeAdminEntry, PathBuf)]) {
    for chunk in entries.chunks_mut(DIRTY_CHECK_CONCURRENCY) {
        let futures = chunk.iter().map(|(_, path)| {
            let path = path.clone();
            async move {
                match worktree_dirty_excluding_agent(&path).await {
                    Ok(dirty) => Some(dirty),
                    Err(err) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %err,
                            "worktree dirty check failed"
                        );
                        None
                    }
                }
            }
        });
        let results = futures_util::future::join_all(futures).await;
        for (slot, flag) in chunk.iter_mut().zip(results) {
            slot.0.has_uncommitted_changes = flag;
        }
    }
}

fn compute_finished(entry: &WorktreeAdminEntry) -> bool {
    if entry.has_active_run {
        return false;
    }
    match entry.has_uncommitted_changes {
        Some(false) => {}
        // Unknown or dirty → not finished.
        _ => return false,
    }
    if entry.ticket_id.is_none() {
        return true;
    }
    if entry.archived || entry.merged {
        return true;
    }
    matches!(
        entry.ticket_status.as_deref(),
        Some("done")
    )
}

fn folder_mtime_rfc3339(path: &Path) -> String {
    let fallback = OffsetDateTime::now_utc();
    let ts = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(OffsetDateTime::from)
        .unwrap_or(fallback);
    ts.format(&Rfc3339).unwrap_or_else(|_| ts.to_string())
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a.to_string_lossy() == b.to_string_lossy(),
    }
}

fn canonicalize_under_root(root: &Path, path: &Path) -> Result<PathBuf, ()> {
    let canonical_root = root.canonicalize().map_err(|_| ())?;
    let canonical = path.canonicalize().map_err(|_| ())?;
    if !canonical.starts_with(&canonical_root) {
        return Err(());
    }
    Ok(canonical)
}

async fn remove_worktree_path(worktree: &Path) -> Result<(), String> {
    let path_str = worktree
        .to_str()
        .ok_or_else(|| "worktree path is not valid UTF-8".to_string())?
        .to_string();

    if let Some(main) = resolve_main_checkout(worktree).await {
        let remove = crate::services::git_cmd::output(
            Some(&main),
            &["worktree", "remove", "--force", &path_str],
        )
        .await;
        match remove {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                tracing::warn!(
                    path = %worktree.display(),
                    stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                    "git worktree remove failed; falling back to remove_dir_all"
                );
            }
            Err(err) => {
                tracing::warn!(
                    path = %worktree.display(),
                    error = %err,
                    "git worktree remove failed; falling back to remove_dir_all"
                );
            }
        }
        let _ = crate::services::git_cmd::output(Some(&main), &["worktree", "prune"]).await;
    }

    if worktree.exists() {
        tokio::fs::remove_dir_all(worktree)
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

async fn resolve_main_checkout(worktree: &Path) -> Option<PathBuf> {
    let output = crate::services::git_cmd::output(
        Some(worktree),
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let common = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if common.file_name().and_then(|s| s.to_str()) == Some(".git") {
        common.parent().map(Path::to_path_buf)
    } else {
        Some(common)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_name_pattern_accepts_ticket_dirs() {
        assert!(is_ticket_worktree_folder("TICKET-fd82310c-coppice"));
        assert!(is_ticket_worktree_folder("TICKET-550e8400-my-repo"));
        assert!(!is_ticket_worktree_folder("chat"));
        assert!(!is_ticket_worktree_folder("plan-scratch"));
        assert!(!is_ticket_worktree_folder("plugin-builds"));
        assert!(!is_ticket_worktree_folder("knowledge-compaction"));
        assert!(!is_ticket_worktree_folder("TICKET-short-coppice"));
        assert!(!is_ticket_worktree_folder("../TICKET-fd82310c-coppice"));
        assert!(!is_ticket_worktree_folder("TICKET-fd82310c-coppice/../x"));
    }

    #[test]
    fn finished_requires_clean_terminal_or_unmatched() {
        let base = WorktreeAdminEntry {
            folder_name: "TICKET-aaaaaaaa-repo".into(),
            path: "/tmp/x".into(),
            repo_id: None,
            repo_name: None,
            ticket_id: None,
            ticket_title: None,
            ticket_status: None,
            archived: false,
            merged: false,
            has_uncommitted_changes: Some(false),
            has_active_run: false,
            last_used_at: "2026-01-01T00:00:00Z".into(),
            size_bytes: None,
            finished: false,
        };
        assert!(compute_finished(&base));

        let mut dirty = base.clone();
        dirty.has_uncommitted_changes = Some(true);
        assert!(!compute_finished(&dirty));

        let mut unknown = base.clone();
        unknown.has_uncommitted_changes = None;
        assert!(!compute_finished(&unknown));

        let mut active = base.clone();
        active.has_active_run = true;
        assert!(!compute_finished(&active));

        let mut done = base.clone();
        done.ticket_id = Some(Id::nil());
        done.ticket_status = Some("done".into());
        assert!(compute_finished(&done));

        let mut progress = done.clone();
        progress.ticket_status = Some("in_progress".into());
        assert!(!compute_finished(&progress));

        let mut archived = progress.clone();
        archived.archived = true;
        assert!(compute_finished(&archived));
    }

    #[test]
    fn dir_size_skips_unreadable_subdirectory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("ok.txt"), b"hi").expect("write");
        let secret = tmp.path().join("secret");
        std::fs::create_dir(&secret).expect("mkdir");
        std::fs::write(secret.join("hidden.bin"), b"xx").expect("hidden");

        // Missing root: walk errors must not panic or fail the call.
        assert_eq!(dir_size_bytes(Path::new("/no/such/worktree-admin-size")), 0);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o000))
                .expect("chmod");
            let size = dir_size_bytes(tmp.path());
            let still_blocked = std::fs::read_dir(&secret).is_err();
            std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o755))
                .expect("chmod restore");
            if still_blocked {
                assert_eq!(size, 2, "should count only the readable file");
            } else {
                // Root can still read mode 000 dirs; at least the walk completed.
                assert!(size >= 2);
            }
        }
    }

    #[test]
    fn size_cache_returns_stale_bytes_while_refreshing() {
        let cache = WorktreeSizeCache::new();
        cache.store("TICKET-aaaaaaaa-repo".into(), 42);
        {
            let mut guard = cache.entries.write().unwrap();
            guard.get_mut("TICKET-aaaaaaaa-repo").unwrap().measured_at =
                Instant::now() - SIZE_CACHE_TTL - Duration::from_secs(1);
        }
        let (bytes, refresh) = cache.get_for_list("TICKET-aaaaaaaa-repo");
        assert_eq!(bytes, Some(42));
        assert!(refresh);
    }
}
