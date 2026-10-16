//! Per-chat repository copies.
//!
//! Each chat session works in `$WORKTREES_PATH/chat/{session_id}/`. `REPOS.md`
//! lists every linked repository. The board's repository (the one its tickets
//! use most) is copied when the chat starts, in the background, and only when
//! it fits the size and time budgets. Every other repository is copied when
//! `open_repo` is called. Copies are `git clone --local --shared`: the clone's
//! object store borrows the registered checkout through alternates, not
//! hardlinks, and remotes are removed. One chat never reads another chat's
//! copy.
//!
//! Limits (also `[agent]` settings; defaults match
//! [`coppice_config`]):
//!
//! | Limit | Default | Effect |
//! | --- | --- | --- |
//! | `chat_open_repo_cap` | 5 | Opening another copy closes the least recently used one |
//! | `chat_eager_copy_timeout_secs` | 3 | Board-repo copy at chat start; then it stays on demand |
//! | `chat_copy_timeout_secs` | 45 | On-demand `open_repo` deadline |
//! | `chat_eager_copy_max_bytes` | 64 MiB | Larger board repositories are not copied at chat start |
//! | `chat_copy_idle_secs` | 12 hours | Delete copies; keep the opened-repo list and rebuild on return |
//! | `chat_folder_max_age_secs` | 30 days | Delete the chat folder; the session row stays |
//!
//! The idle and age sweeps run at process start and then hourly, so a copy can
//! outlive the idle period by up to an hour. Measured with git 2.43: a one-file
//! clone and checkout was 15 ms, a 32 MiB incompressible blob was 67 ms, and an
//! 80 MiB run of zeros was 62 ms. Reading 40 README first lines was 38 ms.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use coppice_config::{
    default_chat_copy_idle_secs, default_chat_copy_timeout_secs, default_chat_eager_copy_max_bytes,
    default_chat_eager_copy_timeout_secs, default_chat_folder_max_age_secs,
    default_chat_open_repo_cap,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const INDEX_FILE: &str = "REPOS.md";
const STATE_FILE: &str = ".coppice-chat.json";

pub const REPOS_READY_LINE: &str = "Getting your repos ready…";

pub fn opening_line(repo: &str) -> String {
    format!("Opening {repo}…")
}

pub fn opened_line(repo: &str) -> String {
    format!("Opened {repo}.")
}

pub fn open_failed_line(repo: &str) -> String {
    format!("Couldn't open {repo}. Ask again to retry.")
}

pub fn closed_for_room_line(repo: &str) -> String {
    format!("Closed {repo} to make room. It'll reopen if needed.")
}

pub const SLUG_NOT_A_PATH: &str = "Pass a repository slug, not a path.";
pub const SLUG_UNKNOWN: &str = "No repository matches that slug. Slugs are listed in REPOS.md.";

#[derive(Debug, Clone)]
pub struct ChatRepoLimits {
    pub open_repo_cap: u32,
    pub eager_copy_timeout: Duration,
    pub copy_timeout: Duration,
    pub copy_idle: Duration,
    pub chat_folder_max_age: Duration,
    pub eager_copy_max_bytes: u64,
    /// Test-only delay inside the background copy. Production leaves this at zero.
    pub copy_pause: Duration,
}

impl Default for ChatRepoLimits {
    fn default() -> Self {
        Self {
            open_repo_cap: default_chat_open_repo_cap(),
            eager_copy_timeout: Duration::from_secs(default_chat_eager_copy_timeout_secs()),
            copy_timeout: Duration::from_secs(default_chat_copy_timeout_secs()),
            copy_idle: Duration::from_secs(default_chat_copy_idle_secs()),
            chat_folder_max_age: Duration::from_secs(default_chat_folder_max_age_secs()),
            eager_copy_max_bytes: default_chat_eager_copy_max_bytes(),
            copy_pause: Duration::ZERO,
        }
    }
}

impl ChatRepoLimits {
    pub fn cap(&self) -> usize {
        self.open_repo_cap.max(1) as usize
    }
}

#[derive(Debug, Clone)]
pub struct LinkedRepo {
    pub name: String,
    pub slug: String,
    pub local_path: PathBuf,
    pub default_branch: String,
    pub description: String,
    pub boards: Vec<String>,
    pub focus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenOutcome {
    pub slug: String,
    pub name: String,
    pub path: PathBuf,
    pub commit: String,
    pub size_bytes: u64,
    pub size_note: String,
    pub elapsed: Duration,
    pub reused: bool,
    pub closed_name: Option<String>,
}

#[derive(Debug)]
pub enum OpenError {
    NotASlug,
    UnknownSlug,
    Failed { name: String },
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotASlug => write!(f, "{SLUG_NOT_A_PATH}"),
            Self::UnknownSlug => write!(f, "{SLUG_UNKNOWN}"),
            Self::Failed { name } => write!(f, "{}", open_failed_line(name)),
        }
    }
}

#[derive(Debug)]
pub enum EagerFinish {
    Opened(OpenOutcome),
    SkippedTooLarge { name: String, bytes: u64 },
    Failed(OpenError),
}

pub struct WorkspaceStart {
    pub chat_dir: PathBuf,
    pub eager: Option<JoinHandle<EagerFinish>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ChatRepoState {
    opened: Vec<OpenedRepo>,
    carried: Vec<String>,
    copies_swept: bool,
    last_activity_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenedRepo {
    slug: String,
    name: String,
    last_used_unix_ms: u64,
}

pub fn chat_dir(worktrees_root: &Path, session_id: Uuid) -> PathBuf {
    worktrees_root.join("chat").join(session_id.to_string())
}

pub fn checkout_size_note(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bytes < 1024 {
        return format!("Checkout is {bytes} bytes.");
    }
    let (value, unit) = if bytes < 1024 * 1024 {
        (bytes as f64 / KIB, "KB")
    } else if (bytes as f64) < GIB {
        (bytes as f64 / MIB, "MB")
    } else {
        (bytes as f64 / GIB, "GB")
    };
    if value < 10.0 {
        format!("Checkout is {value:.1} {unit}.")
    } else {
        format!("Checkout is {value:.0} {unit}.")
    }
}

/// First line of a README, or a branch sentence when the checkout has none.
pub fn one_line_description(repo: &Path, default_branch: &str) -> String {
    for name in ["README.md", "README.markdown", "README.txt", "README"] {
        if let Some(line) = first_line_limited(&repo.join(name)) {
            return truncate_chars(&line, 120);
        }
    }
    format!("Default branch {default_branch}.")
}

pub fn slug_error(slug: &str) -> Option<&'static str> {
    let trimmed = slug.trim();
    if trimmed.is_empty() {
        return Some(SLUG_UNKNOWN);
    }
    if looks_like_path(trimmed) {
        return Some(SLUG_NOT_A_PATH);
    }
    None
}

/// Write `REPOS.md` and remember `carried` slugs. The board repository, when
/// there is one, is copied on a background thread. This returns without waiting
/// for that copy.
pub fn begin_chat_workspace<F>(
    worktrees_root: &Path,
    session_id: Uuid,
    repos: &[LinkedRepo],
    carried: &[String],
    limits: &ChatRepoLimits,
    mut notes: F,
) -> std::io::Result<WorkspaceStart>
where
    F: FnMut(&str) + Send + 'static,
{
    let dir = chat_dir(worktrees_root, session_id);
    fs::create_dir_all(&dir)?;
    let _lock = dir_lock(&dir)?;
    let mut state = read_state(&dir);
    for slug in carried {
        if slug_error(slug).is_some() || !repos.iter().any(|repo| repo.slug == *slug) {
            continue;
        }
        if !state.carried.iter().any(|existing| existing == slug)
            && !state.opened.iter().any(|opened| &opened.slug == slug)
        {
            state.carried.push(slug.clone());
        }
    }
    if state.last_activity_unix_ms == 0 {
        state.last_activity_unix_ms = now_ms();
    }
    write_state(&dir, &state)?;
    write_index(&dir, repos, &state)?;
    drop(_lock);

    let focus = repos.iter().find(|repo| repo.focus).cloned();
    let eager = if let Some(focus) = focus {
        let dir = dir.clone();
        let repos = repos.to_vec();
        let limits = limits.clone();
        let focus_slug = focus.slug;
        let handle = std::thread::Builder::new()
            .name("chat-repo-copy".into())
            .spawn(move || {
                if !limits.copy_pause.is_zero() {
                    std::thread::sleep(limits.copy_pause);
                }
                let Some(focus) = repos.iter().find(|repo| repo.slug == focus_slug) else {
                    return EagerFinish::Failed(OpenError::UnknownSlug);
                };
                match tree_bytes_until(&focus.local_path, limits.eager_copy_max_bytes) {
                    Ok(bytes) if bytes > limits.eager_copy_max_bytes => {
                        EagerFinish::SkippedTooLarge {
                            name: focus.name.clone(),
                            bytes,
                        }
                    }
                    Ok(_) | Err(_) => {
                        let mut eager_limits = limits.clone();
                        eager_limits.copy_timeout = limits.eager_copy_timeout;
                        eager_limits.copy_pause = Duration::ZERO;
                        match open_repo(
                            &dir,
                            session_id,
                            &repos,
                            &focus_slug,
                            &eager_limits,
                            &mut notes,
                        ) {
                            Ok(outcome) => EagerFinish::Opened(outcome),
                            Err(err) => EagerFinish::Failed(err),
                        }
                    }
                }
            })
            .expect("spawn chat repo copy");
        Some(handle)
    } else {
        drop(notes);
        None
    };

    Ok(WorkspaceStart {
        chat_dir: dir,
        eager,
    })
}

pub fn open_repo(
    chat_dir: &Path,
    session_id: Uuid,
    repos: &[LinkedRepo],
    slug: &str,
    limits: &ChatRepoLimits,
    notes: &mut dyn FnMut(&str),
) -> Result<OpenOutcome, OpenError> {
    let slug = slug.trim();
    if let Some(err) = slug_error(slug) {
        return Err(if err == SLUG_NOT_A_PATH {
            OpenError::NotASlug
        } else {
            OpenError::UnknownSlug
        });
    }
    let Some(repo) = repos.iter().find(|repo| repo.slug == slug) else {
        return Err(OpenError::UnknownSlug);
    };
    fs::create_dir_all(chat_dir).map_err(|_| OpenError::Failed {
        name: repo.name.clone(),
    })?;
    let _guard = dir_lock(chat_dir).map_err(|_| OpenError::Failed {
        name: repo.name.clone(),
    })?;
    let dest = chat_dir.join(&repo.slug);
    let started = Instant::now();
    let mut state = read_state(chat_dir);
    if drop_unlinked_slugs(&mut state, repos) {
        let _ = write_state(chat_dir, &state);
    }
    touch_state(&mut state);

    if copy_is_ready(&dest) {
        touch_opened(&mut state, repo);
        state.carried.retain(|carried| carried != &repo.slug);
        let _ = write_state(chat_dir, &state);
        let _ = write_index(chat_dir, repos, &state);
        let commit =
            git_stdout(&dest, &["rev-parse", "HEAD"], Duration::from_secs(5)).unwrap_or_default();
        let size_bytes = tree_bytes_until(&dest, u64::MAX).unwrap_or(0);
        return Ok(OpenOutcome {
            slug: repo.slug.clone(),
            name: repo.name.clone(),
            path: dest,
            commit,
            size_bytes,
            size_note: checkout_size_note(size_bytes),
            elapsed: started.elapsed(),
            reused: true,
            closed_name: None,
        });
    }

    let mut closed_name = None;
    let ready = ready_opened(chat_dir, &state);
    if ready.len() >= limits.cap() && !ready.iter().any(|opened| opened.slug == repo.slug) {
        if let Some(victim) = ready.iter().min_by_key(|opened| opened.last_used_unix_ms) {
            let victim_slug = victim.slug.clone();
            let victim_name = victim.name.clone();
            notes(&closed_for_room_line(&victim_name));
            if let Err(err) = remove_tree_inside(chat_root(chat_dir), &chat_dir.join(&victim_slug))
            {
                tracing::warn!(
                    slug = %victim_slug,
                    error = %err,
                    "refusing to close a chat copy outside the chat folder"
                );
            }
            state.opened.retain(|opened| opened.slug != victim_slug);
            closed_name = Some(victim_name);
        }
    }

    notes(&opening_line(&repo.name));
    let deadline = Instant::now() + limits.copy_timeout;
    if let Err(err) = materialize(chat_root(chat_dir), repo, &dest, deadline) {
        tracing::warn!(
            %session_id,
            slug = %repo.slug,
            error = %err,
            "chat repository copy failed"
        );
        if let Err(err) = remove_tree_inside(chat_root(chat_dir), &dest) {
            tracing::warn!(
                slug = %repo.slug,
                error = %err,
                "refusing to remove a failed chat copy outside the chat folder"
            );
        }
        state.opened.retain(|opened| opened.slug != repo.slug);
        let _ = write_state(chat_dir, &state);
        let _ = write_index(chat_dir, repos, &state);
        notes(&open_failed_line(&repo.name));
        return Err(OpenError::Failed {
            name: repo.name.clone(),
        });
    }

    touch_opened(&mut state, repo);
    state.carried.retain(|carried| carried != &repo.slug);
    let _ = write_state(chat_dir, &state);
    let _ = write_index(chat_dir, repos, &state);
    let commit =
        git_stdout(&dest, &["rev-parse", "HEAD"], Duration::from_secs(5)).unwrap_or_default();
    let size_bytes = tree_bytes_until(&dest, u64::MAX).unwrap_or(0);
    notes(&opened_line(&repo.name));
    Ok(OpenOutcome {
        slug: repo.slug.clone(),
        name: repo.name.clone(),
        path: dest,
        commit,
        size_bytes,
        size_note: checkout_size_note(size_bytes),
        elapsed: started.elapsed(),
        reused: false,
        closed_name,
    })
}

pub fn copies_were_swept(chat_dir: &Path) -> bool {
    read_state(chat_dir).copies_swept
}

pub fn clear_swept_flag(chat_dir: &Path) -> std::io::Result<()> {
    let mut state = read_state(chat_dir);
    state.copies_swept = false;
    write_state(chat_dir, &state)
}

pub fn touch_activity(chat_dir: &Path) -> std::io::Result<()> {
    if !chat_dir.exists() {
        return Ok(());
    }
    let mut state = read_state(chat_dir);
    touch_state(&mut state);
    write_state(chat_dir, &state)
}

pub fn refresh_index(chat_dir: &Path, repos: &[LinkedRepo]) -> std::io::Result<()> {
    let state = read_state(chat_dir);
    write_index(chat_dir, repos, &state)
}

pub fn slugs_to_restore(chat_dir: &Path, repos: &[LinkedRepo]) -> Vec<String> {
    let mut state = read_state(chat_dir);
    if drop_unlinked_slugs(&mut state, repos) {
        let _ = write_state(chat_dir, &state);
    }
    state
        .opened
        .into_iter()
        .filter(|opened| !copy_is_ready(&chat_dir.join(&opened.slug)))
        .map(|opened| opened.slug)
        .collect()
}

pub fn remove_chat_workspace(worktrees_root: &Path, session_id: Uuid) -> std::io::Result<()> {
    let root = worktrees_root.join("chat");
    let path = chat_dir(worktrees_root, session_id);
    remove_tree_inside(&root, &path)
}

/// Delete materialized copies whose chat has been quiet for `idle`. The index
/// and the opened-repo list stay so the next turn can rebuild.
pub fn sweep_idle_copies(
    worktrees_root: &Path,
    idle: Duration,
    now: SystemTime,
    repos: &[LinkedRepo],
) -> std::io::Result<u32> {
    let root = worktrees_root.join("chat");
    if !root.exists() {
        return Ok(0);
    }
    let mut removed = 0u32;
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let dir = entry.path();
        if !is_real_dir(&dir) {
            continue;
        }
        if entry.file_name().to_string_lossy().parse::<Uuid>().is_err() {
            continue;
        }
        let _guard = dir_lock(&dir)?;
        let mut state = read_state(&dir);
        let dropped = drop_unlinked_slugs(&mut state, repos);
        if !activity_older_than(&state, idle, now) {
            if dropped {
                write_state(&dir, &state)?;
            }
            continue;
        }
        let mut changed = false;
        for opened in &state.opened {
            let path = dir.join(&opened.slug);
            let existed = fs::symlink_metadata(&path).is_ok();
            match remove_tree_inside(&root, &path) {
                Ok(()) => {
                    if existed {
                        changed = true;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::InvalidInput => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %err,
                        "skipping a chat copy that is outside its chat folder"
                    );
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
        if changed {
            state.copies_swept = true;
            removed += 1;
        }
        if changed || dropped {
            write_state(&dir, &state)?;
        }
    }
    Ok(removed)
}

/// Delete chat folders that have been quiet for `max_age`.
pub fn sweep_aged_chat_folders(
    worktrees_root: &Path,
    max_age: Duration,
    now: SystemTime,
) -> std::io::Result<u32> {
    let root = worktrees_root.join("chat");
    if !root.exists() {
        return Ok(0);
    }
    let mut removed = 0u32;
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        let dir = entry.path();
        if !is_real_dir(&dir) {
            continue;
        }
        let state = read_state(&dir);
        let old = if state.last_activity_unix_ms == 0 {
            dir_older_than(&dir, max_age, now)
        } else {
            activity_older_than(&state, max_age, now)
        };
        if !old {
            continue;
        }
        match remove_tree_inside(&root, &dir) {
            Ok(()) => removed += 1,
            Err(err) if err.kind() == std::io::ErrorKind::InvalidInput => {
                tracing::warn!(
                    path = %dir.display(),
                    error = %err,
                    "skipping a chat folder that is outside the chat directory"
                );
            }
            Err(err) => return Err(err),
        }
    }
    Ok(removed)
}

/// Most-used repository on a board. Ties break by name, then id.
pub fn choose_focus_repo(counts: &[(Uuid, String, i64)]) -> Option<Uuid> {
    let mut best: Option<&(Uuid, String, i64)> = None;
    for row in counts {
        best = Some(match best {
            None => row,
            Some(current) => {
                let more = row.2 > current.2;
                let same_count_earlier_name = row.2 == current.2 && row.1 < current.1;
                let same_name_earlier_id =
                    row.2 == current.2 && row.1 == current.1 && row.0 < current.0;
                if more || same_count_earlier_name || same_name_earlier_id {
                    row
                } else {
                    current
                }
            }
        });
    }
    best.map(|row| row.0)
}

pub fn assign_slug(name: &str, id: Uuid, used: &mut std::collections::HashSet<String>) -> String {
    let mut slug = crate::domain::slug::slugify(name);
    if slug.is_empty() {
        slug = "repo".into();
    }
    if used.insert(slug.clone()) {
        return slug;
    }
    let short: String = id.to_string().chars().take(8).collect();
    let with_short = format!("{slug}-{short}");
    if used.insert(with_short.clone()) {
        return with_short;
    }
    let full = format!("{slug}-{}", id.simple());
    used.insert(full.clone());
    full
}

fn materialize(
    chat_root: &Path,
    repo: &LinkedRepo,
    dest: &Path,
    deadline: Instant,
) -> std::io::Result<()> {
    remove_tree_inside(chat_root, dest)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let commit = source_commit(&repo.local_path, &repo.default_branch, remaining(deadline))?;
    let src = repo.local_path.display().to_string();
    let dst = dest.display().to_string();
    git(
        None,
        &["clone", "--local", "--shared", "--no-checkout", &src, &dst],
        remaining(deadline),
    )?;
    let alternates = fs::read_to_string(dest.join(".git/objects/info/alternates"))
        .map_err(|_| std::io::Error::other("chat copy is missing alternates"))?;
    if alternates.trim().is_empty() {
        return Err(std::io::Error::other("chat copy alternates are empty"));
    }
    remove_remotes(dest, deadline)?;
    git(
        Some(dest),
        &["checkout", "--detach", &commit],
        remaining(deadline),
    )?;
    let remotes = git_stdout(dest, &["remote"], remaining(deadline))?;
    if !remotes.is_empty() {
        return Err(std::io::Error::other(format!(
            "chat copy still has remotes: {remotes}"
        )));
    }
    Ok(())
}

fn source_commit(repo: &Path, default_branch: &str, timeout: Duration) -> std::io::Result<String> {
    let spec = format!("refs/heads/{default_branch}");
    if let Ok(sha) = git_stdout(
        repo,
        &["rev-parse", "--verify", "--end-of-options", &spec],
        timeout,
    ) {
        if !sha.is_empty() {
            return Ok(sha);
        }
    }
    git_stdout(repo, &["rev-parse", "--verify", "HEAD"], timeout)
}

fn remove_remotes(repo: &Path, deadline: Instant) -> std::io::Result<()> {
    let listed = git_stdout(repo, &["remote"], remaining(deadline))?;
    for name in listed.split_whitespace() {
        git(Some(repo), &["remote", "remove", name], remaining(deadline))?;
    }
    Ok(())
}

fn copy_is_ready(dest: &Path) -> bool {
    let git_dir = dest.join(".git");
    if !git_dir.is_dir() {
        return false;
    }
    let Ok(alternates) = fs::read_to_string(git_dir.join("objects/info/alternates")) else {
        return false;
    };
    if alternates.trim().is_empty() {
        return false;
    }
    if !matches!(
        git_stdout(dest, &["remote"], Duration::from_secs(5)),
        Ok(remotes) if remotes.is_empty()
    ) {
        return false;
    }
    // Source `git gc` can delete objects this copy still names. Reusing that
    // checkout would hide the missing history, so the caller clones again.
    git(
        Some(dest),
        &["cat-file", "-e", "HEAD"],
        Duration::from_secs(5),
    )
    .is_ok()
        && git(
            Some(dest),
            &["cat-file", "-e", "HEAD^{tree}"],
            Duration::from_secs(5),
        )
        .is_ok()
}

fn ready_opened(chat_dir: &Path, state: &ChatRepoState) -> Vec<OpenedRepo> {
    state
        .opened
        .iter()
        .filter(|opened| copy_is_ready(&chat_dir.join(&opened.slug)))
        .cloned()
        .collect()
}

fn touch_opened(state: &mut ChatRepoState, repo: &LinkedRepo) {
    state.opened.retain(|opened| opened.slug != repo.slug);
    state.opened.push(OpenedRepo {
        slug: repo.slug.clone(),
        name: repo.name.clone(),
        last_used_unix_ms: now_ms(),
    });
}

fn touch_state(state: &mut ChatRepoState) {
    state.last_activity_unix_ms = now_ms();
}

fn write_index(
    chat_dir: &Path,
    repos: &[LinkedRepo],
    state: &ChatRepoState,
) -> std::io::Result<()> {
    let mut out = String::from(
        "# Repositories\n\n\
         These repositories are linked to this workspace. This chat has its own copies under this folder. \
         Call `open_repo` with a slug when you need a repository that is not checked out. Pass a slug, not a path.\n\n",
    );
    if let Some(focus) = repos.iter().find(|repo| repo.focus) {
        out.push_str(&format!(
            "Focus: **{}** (`{}`). This is the repository the current board uses most.\n\n",
            cell(&focus.name),
            focus.slug
        ));
    } else {
        out.push_str(
            "Focus: none. This chat has no board repository to check out at the start.\n\n",
        );
    }
    out.push_str("| Name | Folder | Description | Boards |\n| --- | --- | --- | --- |\n");
    let open_slugs: HashMap<&str, ()> = state
        .opened
        .iter()
        .filter(|opened| copy_is_ready(&chat_dir.join(&opened.slug)))
        .map(|opened| (opened.slug.as_str(), ()))
        .collect();
    for repo in repos {
        let checked_out = open_slugs.contains_key(repo.slug.as_str());
        let mut marks = Vec::new();
        if repo.focus {
            marks.push("focus");
        }
        if checked_out {
            marks.push("checked out");
        } else {
            marks.push("call `open_repo`");
        }
        let boards = if repo.boards.is_empty() {
            "None".to_string()
        } else {
            repo.boards
                .iter()
                .map(|board| cell(board))
                .collect::<Vec<_>>()
                .join(", ")
        };
        out.push_str(&format!(
            "| {} | `{}/` ({}) | {} | {} |\n",
            cell(&repo.name),
            repo.slug,
            marks.join(", "),
            cell(&repo.description),
            boards
        ));
    }
    write_atomic(&chat_dir.join(INDEX_FILE), out.as_bytes())
}

fn cell(text: &str) -> String {
    text.replace(['|', '\n', '\r'], " ")
}

fn read_state(chat_dir: &Path) -> ChatRepoState {
    let path = chat_dir.join(STATE_FILE);
    let Ok(text) = fs::read_to_string(path) else {
        return ChatRepoState::default();
    };
    let mut state: ChatRepoState = serde_json::from_str(&text).unwrap_or_default();
    if drop_unsafe_slugs(&mut state) {
        let _ = write_state(chat_dir, &state);
    }
    state
}

fn slug_is_safe(slug: &str) -> bool {
    slug_error(slug).is_none()
}

fn drop_unsafe_slugs(state: &mut ChatRepoState) -> bool {
    let opened_before = state.opened.len();
    let carried_before = state.carried.len();
    state.opened.retain(|opened| slug_is_safe(&opened.slug));
    state.carried.retain(|slug| slug_is_safe(slug));
    state.opened.len() != opened_before || state.carried.len() != carried_before
}

fn drop_unlinked_slugs(state: &mut ChatRepoState, repos: &[LinkedRepo]) -> bool {
    let linked = |slug: &str| repos.iter().any(|repo| repo.slug == slug);
    let opened_before = state.opened.len();
    let carried_before = state.carried.len();
    state.opened.retain(|opened| linked(&opened.slug));
    state.carried.retain(|slug| linked(slug));
    state.opened.len() != opened_before || state.carried.len() != carried_before
}

fn chat_root(chat_dir: &Path) -> &Path {
    chat_dir.parent().unwrap_or(chat_dir)
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.is_dir() && !meta.file_type().is_symlink())
        .unwrap_or(false)
}

/// Delete `target` only when it is a real directory strictly inside `root`.
/// Absolute slugs, `..`, and symlinks that leave the chat folder are refused.
fn remove_tree_inside(root: &Path, target: &Path) -> std::io::Result<()> {
    let root_meta = fs::symlink_metadata(root)?;
    if root_meta.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "refusing to delete inside a symlinked chat folder",
        ));
    }
    let meta = match fs::symlink_metadata(target) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    if meta.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "refusing to delete a symlink in a chat folder",
        ));
    }
    let root = root.canonicalize()?;
    let canonical = target.canonicalize()?;
    if canonical == root || !canonical.starts_with(&root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "refusing to delete a path outside the chat folder",
        ));
    }
    fs::remove_dir_all(canonical)
}

fn write_state(chat_dir: &Path, state: &ChatRepoState) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?;
    write_atomic(&chat_dir.join(STATE_FILE), &bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(tmp, path)
}

fn looks_like_path(slug: &str) -> bool {
    slug.contains('/')
        || slug.contains('\\')
        || slug.contains("..")
        || slug.starts_with('.')
        || Path::new(slug).is_absolute()
        || !slug
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

fn first_line_limited(path: &Path) -> Option<String> {
    let meta = fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let mut buf = [0u8; 512];
    let n = file.read(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf[..n]);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = line.trim_start_matches('#').trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn tree_bytes_until(root: &Path, stop_after: u64) -> std::io::Result<u64> {
    if !root.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "repository path is missing",
        ));
    }
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) if dir == root => {
                return Err(std::io::Error::other("repository path is not a directory"));
            }
            Err(_) => continue,
        };
        for entry in entries {
            let entry = entry?;
            if entry.file_name() == ".git" || entry.file_name() == ".coppice-chat.json" {
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(entry.metadata()?.len());
                if total > stop_after {
                    return Ok(total);
                }
            }
        }
    }
    Ok(total)
}

fn activity_older_than(state: &ChatRepoState, age: Duration, now: SystemTime) -> bool {
    let Some(last) = UNIX_EPOCH.checked_add(Duration::from_millis(state.last_activity_unix_ms))
    else {
        return false;
    };
    now.duration_since(last).unwrap_or_default() > age
}

fn dir_older_than(path: &Path, age: Duration, now: SystemTime) -> bool {
    let Ok(modified) = fs::metadata(path).and_then(|meta| meta.modified()) else {
        return false;
    };
    now.duration_since(modified).unwrap_or_default() > age
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

fn dir_lock(chat_dir: &Path) -> std::io::Result<File> {
    fs::create_dir_all(chat_dir)?;
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(chat_dir.join(".coppice-chat.lock"))?;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(file)
}

fn git(cwd: Option<&Path>, args: &[&str], timeout: Duration) -> std::io::Result<String> {
    run_git(cwd, args, timeout)
}

fn git_stdout(cwd: &Path, args: &[&str], timeout: Duration) -> std::io::Result<String> {
    run_git(Some(cwd), args, timeout)
}

fn run_git(cwd: Option<&Path>, args: &[&str], timeout: Duration) -> std::io::Result<String> {
    if timeout.is_zero() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("git {} timed out", args.join(" ")),
        ));
    }
    let mut command = Command::new("git");
    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command.spawn().map_err(|err| {
        std::io::Error::other(format!("failed to run git {}: {err}", args.join(" ")))
    })?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > timeout => {
                kill_child(&mut child);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("git {} timed out", args.join(" ")),
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(err) => return Err(err),
        }
    };
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut stdout);
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(stdout.trim().to_string())
}

fn kill_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use std::sync::{Arc, Barrier, Mutex as StdMutex};

    fn git_ok(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Coppice Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Coppice Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .expect("git");
        assert!(status.success(), "git {} failed", args.join(" "));
    }

    fn init_repo(root: &Path, name: &str) -> PathBuf {
        let repo = root.join(name);
        fs::create_dir_all(&repo).unwrap();
        git_ok(&repo, &["init", "-b", "main"]);
        fs::write(repo.join("README.md"), format!("{name} does one thing.\n")).unwrap();
        git_ok(&repo, &["add", "README.md"]);
        git_ok(&repo, &["commit", "-m", "one"]);
        git_ok(
            &repo,
            &["remote", "add", "origin", "https://example.com/repo.git"],
        );
        repo
    }

    fn linked(name: &str, path: PathBuf, focus: bool) -> LinkedRepo {
        LinkedRepo {
            slug: crate::domain::slug::slugify(name),
            name: name.to_string(),
            local_path: path,
            default_branch: "main".into(),
            description: format!("{name} does one thing."),
            boards: vec!["Alpha".into()],
            focus,
        }
    }

    fn limits_cap(cap: u32) -> ChatRepoLimits {
        ChatRepoLimits {
            open_repo_cap: cap,
            ..ChatRepoLimits::default()
        }
    }

    fn open_notes(
        dir: &Path,
        session: Uuid,
        repos: &[LinkedRepo],
        slug: &str,
        limits: &ChatRepoLimits,
    ) -> (Result<OpenOutcome, OpenError>, Vec<String>) {
        let notes = Arc::new(StdMutex::new(Vec::new()));
        let notes_cb = Arc::clone(&notes);
        let result = open_repo(dir, session, repos, slug, limits, &mut |line| {
            notes_cb.lock().unwrap().push(line.to_string());
        });
        let recorded = notes.lock().unwrap().clone();
        (result, recorded)
    }

    fn loose_object(repo: &Path, sha: &str) -> PathBuf {
        repo.join(".git")
            .join("objects")
            .join(&sha[..2])
            .join(&sha[2..])
    }

    #[test]
    fn transcript_lines_are_exact() {
        assert_eq!(opening_line("Demo"), "Opening Demo…");
        assert_eq!(opened_line("Demo"), "Opened Demo.");
        assert_eq!(
            open_failed_line("Demo"),
            "Couldn't open Demo. Ask again to retry."
        );
        assert_eq!(
            closed_for_room_line("Demo"),
            "Closed Demo to make room. It'll reopen if needed."
        );
        assert_eq!(REPOS_READY_LINE, "Getting your repos ready…");
    }

    #[test]
    fn slug_rejects_paths_and_unknowns() {
        let root = tempfile::tempdir().unwrap();
        let repo = init_repo(root.path(), "demo");
        let session = Uuid::new_v4();
        let dir = chat_dir(root.path(), session);
        fs::create_dir_all(&dir).unwrap();
        let repos = vec![linked("demo", repo, false)];
        let limits = ChatRepoLimits::default();

        for slug in [
            "../demo",
            "demo/src",
            "/tmp/demo",
            "demo\\src",
            "..",
            ".demo",
        ] {
            let (err, notes) = open_notes(&dir, session, &repos, slug, &limits);
            assert!(matches!(err, Err(OpenError::NotASlug)), "{slug}");
            assert!(notes.is_empty(), "{slug}");
        }
        let (err, notes) = open_notes(&dir, session, &repos, "missing", &limits);
        assert!(matches!(err, Err(OpenError::UnknownSlug)));
        assert!(notes.is_empty());
        assert_eq!(err.unwrap_err().to_string(), SLUG_UNKNOWN);
    }

    #[test]
    fn clone_uses_alternates_and_has_no_remotes() {
        let root = tempfile::tempdir().unwrap();
        let repo = init_repo(root.path(), "demo");
        fs::write(repo.join("README.md"), "two\n").unwrap();
        git_ok(&repo, &["add", "README.md"]);
        git_ok(&repo, &["commit", "-m", "two"]);
        git_ok(&repo, &["checkout", "--detach", "HEAD~1"]);
        let blob = git_stdout(
            &repo,
            &["rev-parse", "HEAD:README.md"],
            Duration::from_secs(5),
        )
        .unwrap();
        let src_object = loose_object(&repo, &blob);
        let before = fs::metadata(&src_object).unwrap().nlink();
        let main = git_stdout(
            &repo,
            &["rev-parse", "refs/heads/main"],
            Duration::from_secs(5),
        )
        .unwrap();

        let session = Uuid::new_v4();
        let repos = vec![linked("demo", repo.clone(), true)];
        let (result, notes) = open_notes(
            &chat_dir(root.path(), session),
            session,
            &repos,
            "demo",
            &ChatRepoLimits::default(),
        );
        let outcome = result.expect("open");
        assert_eq!(notes, vec![opening_line("demo"), opened_line("demo")]);
        assert_eq!(outcome.commit, main);
        assert_eq!(
            fs::read_to_string(outcome.path.join("README.md")).unwrap(),
            "two\n"
        );
        assert_eq!(
            git_stdout(&outcome.path, &["remote"], Duration::from_secs(5)).unwrap(),
            ""
        );
        let alternates =
            fs::read_to_string(outcome.path.join(".git/objects/info/alternates")).unwrap();
        assert!(alternates.contains(repo.join(".git/objects").to_str().unwrap()));
        assert!(!loose_object(&outcome.path, &blob).exists());
        assert_eq!(fs::metadata(&src_object).unwrap().nlink(), before);
        assert!(!outcome.size_note.is_empty());
        assert!(outcome.size_note.starts_with("Checkout is "));
    }

    #[test]
    fn second_open_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let repo = init_repo(root.path(), "demo");
        let session = Uuid::new_v4();
        let dir = chat_dir(root.path(), session);
        let repos = vec![linked("demo", repo, false)];
        let limits = ChatRepoLimits::default();
        open_notes(&dir, session, &repos, "demo", &limits)
            .0
            .unwrap();
        let (again, notes) = open_notes(&dir, session, &repos, "demo", &limits);
        let again = again.unwrap();
        assert!(again.reused);
        assert!(notes.is_empty());
    }

    #[test]
    fn cap_closes_the_least_recently_used_copy() {
        let root = tempfile::tempdir().unwrap();
        let session = Uuid::new_v4();
        let dir = chat_dir(root.path(), session);
        let mut repos = Vec::new();
        for name in ["alpha", "beta", "gamma"] {
            repos.push(linked(name, init_repo(root.path(), name), false));
        }
        let limits = limits_cap(2);
        open_notes(&dir, session, &repos, "alpha", &limits)
            .0
            .unwrap();
        std::thread::sleep(Duration::from_millis(5));
        open_notes(&dir, session, &repos, "beta", &limits)
            .0
            .unwrap();
        std::thread::sleep(Duration::from_millis(5));
        let (third, notes) = open_notes(&dir, session, &repos, "gamma", &limits);
        third.unwrap();
        assert_eq!(notes[0], closed_for_room_line("alpha"));
        assert!(notes.contains(&opening_line("gamma")));
        assert!(notes.contains(&opened_line("gamma")));
        assert!(!dir.join("alpha").exists());
        assert!(dir.join("beta").exists());
        assert!(dir.join("gamma").exists());

        let (back, notes) = open_notes(&dir, session, &repos, "alpha", &limits);
        back.unwrap();
        assert_eq!(notes[0], closed_for_room_line("beta"));
        assert!(dir.join("alpha").exists());
        assert!(!dir.join("beta").exists());
    }

    #[test]
    fn two_chats_get_separate_copies() {
        let root = tempfile::tempdir().unwrap();
        let repo = init_repo(root.path(), "demo");
        let repos = vec![linked("demo", repo, false)];
        let limits = ChatRepoLimits::default();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let first = open_notes(&chat_dir(root.path(), a), a, &repos, "demo", &limits)
            .0
            .unwrap();
        let second = open_notes(&chat_dir(root.path(), b), b, &repos, "demo", &limits)
            .0
            .unwrap();
        assert_ne!(first.path, second.path);
        fs::write(first.path.join("README.md"), "changed\n").unwrap();
        assert_eq!(
            fs::read_to_string(second.path.join("README.md")).unwrap(),
            "demo does one thing.\n"
        );
    }

    #[test]
    fn concurrent_opens_on_one_chat_both_succeed() {
        let root = tempfile::tempdir().unwrap();
        let session = Uuid::new_v4();
        let dir = chat_dir(root.path(), session);
        fs::create_dir_all(&dir).unwrap();
        let repos = vec![
            linked("alpha", init_repo(root.path(), "alpha"), false),
            linked("beta", init_repo(root.path(), "beta"), false),
        ];
        let barrier = Arc::new(Barrier::new(2));
        let limits = ChatRepoLimits::default();
        let mut handles = Vec::new();
        for slug in ["alpha", "beta"] {
            let barrier = Arc::clone(&barrier);
            let dir = dir.clone();
            let repos = repos.clone();
            let limits = limits.clone();
            let slug = slug.to_string();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                open_repo(&dir, session, &repos, &slug, &limits, &mut |_| {})
            }));
        }
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        assert!(dir.join("alpha").join(".git").is_dir());
        assert!(dir.join("beta").join(".git").is_dir());
    }

    #[test]
    fn idle_sweep_drops_copies_and_delete_removes_the_folder() {
        let root = tempfile::tempdir().unwrap();
        let session = Uuid::new_v4();
        let repo = init_repo(root.path(), "demo");
        let repos = vec![linked("demo", repo, false)];
        let dir = chat_dir(root.path(), session);
        open_notes(&dir, session, &repos, "demo", &ChatRepoLimits::default())
            .0
            .unwrap();
        let mut state = read_state(&dir);
        state.last_activity_unix_ms = 1;
        write_state(&dir, &state).unwrap();
        let removed = sweep_idle_copies(
            root.path(),
            Duration::from_secs(1),
            SystemTime::now(),
            &repos,
        )
        .unwrap();
        assert_eq!(removed, 1);
        assert!(!dir.join("demo").exists());
        assert!(dir.join(INDEX_FILE).exists());
        assert!(copies_were_swept(&dir));
        assert_eq!(slugs_to_restore(&dir, &repos), vec!["demo".to_string()]);

        remove_chat_workspace(root.path(), session).unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn hostile_state_slugs_do_not_delete_outside_the_chat_dir() {
        let root = tempfile::tempdir().unwrap();
        let session = Uuid::new_v4();
        let dir = chat_dir(root.path(), session);
        fs::create_dir_all(&dir).unwrap();
        let outside_abs = root.path().join("outside-abs");
        let outside_rel = root.path().join("outside-rel");
        fs::create_dir_all(&outside_abs).unwrap();
        fs::create_dir_all(&outside_rel).unwrap();
        fs::write(outside_abs.join("keep.txt"), "keep-abs").unwrap();
        fs::write(outside_rel.join("keep.txt"), "keep-rel").unwrap();

        let abs_slug = outside_abs.to_string_lossy().to_string();
        let rel_slug = "../../outside-rel";
        let planted = serde_json::json!({
            "opened": [
                {"slug": abs_slug, "name": "Absolute", "last_used_unix_ms": 1},
                {"slug": rel_slug, "name": "Relative", "last_used_unix_ms": 1}
            ],
            "carried": [abs_slug, rel_slug],
            "copies_swept": false,
            "last_activity_unix_ms": 1
        });
        let write_hostile = |dir: &Path| {
            fs::write(
                dir.join(STATE_FILE),
                serde_json::to_vec_pretty(&planted).unwrap(),
            )
            .unwrap();
        };
        write_hostile(&dir);

        let repos = vec![linked("demo", init_repo(root.path(), "demo"), false)];
        sweep_idle_copies(
            root.path(),
            Duration::from_secs(1),
            SystemTime::now(),
            &repos,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(outside_abs.join("keep.txt")).unwrap(),
            "keep-abs"
        );
        assert_eq!(
            fs::read_to_string(outside_rel.join("keep.txt")).unwrap(),
            "keep-rel"
        );
        let loaded = read_state(&dir);
        assert!(loaded.opened.is_empty());
        assert!(loaded.carried.is_empty());

        write_hostile(&dir);
        let restore = slugs_to_restore(&dir, &repos);
        assert!(
            restore.iter().all(|slug| slug_error(slug).is_none()),
            "{restore:?}"
        );
        assert!(!restore
            .iter()
            .any(|slug| slug == &abs_slug || slug == rel_slug));

        write_hostile(&dir);
        open_notes(&dir, session, &repos, "demo", &limits_cap(1))
            .0
            .unwrap();
        assert_eq!(
            fs::read_to_string(outside_abs.join("keep.txt")).unwrap(),
            "keep-abs"
        );
        assert_eq!(
            fs::read_to_string(outside_rel.join("keep.txt")).unwrap(),
            "keep-rel"
        );
        assert!(dir.join("demo").join("README.md").is_file());

        let chat_root = root.path().join("chat");
        let abs_err = remove_tree_inside(&chat_root, &outside_abs).unwrap_err();
        let rel_err = remove_tree_inside(&chat_root, &dir.join(rel_slug)).unwrap_err();
        assert_eq!(abs_err.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(rel_err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(outside_abs.join("keep.txt").is_file());
        assert!(outside_rel.join("keep.txt").is_file());
    }

    #[test]
    fn symlink_readme_is_not_followed() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("demo");
        fs::create_dir_all(&repo).unwrap();
        let secret = root.path().join("secret.txt");
        fs::write(&secret, "TOP SECRET checkout notes\n").unwrap();
        std::os::unix::fs::symlink(&secret, repo.join("README.md")).unwrap();
        let description = one_line_description(&repo, "main");
        assert_eq!(description, "Default branch main.");
        assert!(!description.contains("SECRET"));
    }

    #[test]
    fn broken_copy_is_rebuilt() {
        let root = tempfile::tempdir().unwrap();
        let session = Uuid::new_v4();
        let repo = init_repo(root.path(), "demo");
        let repos = vec![linked("demo", repo, false)];
        let dir = chat_dir(root.path(), session);
        let limits = ChatRepoLimits::default();
        let first = open_notes(&dir, session, &repos, "demo", &limits)
            .0
            .unwrap();
        assert!(!first.reused);
        fs::write(
            first.path.join(".git/HEAD"),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        )
        .unwrap();
        assert!(!copy_is_ready(&first.path));
        let (second, notes) = open_notes(&dir, session, &repos, "demo", &limits);
        let second = second.unwrap();
        assert!(!second.reused);
        assert!(notes.contains(&opening_line("demo")));
        assert!(notes.contains(&opened_line("demo")));
        assert_eq!(
            fs::read_to_string(dir.join("demo").join("README.md")).unwrap(),
            "demo does one thing.\n"
        );
    }

    #[test]
    fn age_sweep_deletes_quiet_chat_folders() {
        let root = tempfile::tempdir().unwrap();
        let session = Uuid::new_v4();
        let dir = chat_dir(root.path(), session);
        fs::create_dir_all(&dir).unwrap();
        let state = ChatRepoState {
            last_activity_unix_ms: 1,
            ..ChatRepoState::default()
        };
        write_state(&dir, &state).unwrap();
        let removed =
            sweep_aged_chat_folders(root.path(), Duration::from_secs(1), SystemTime::now())
                .unwrap();
        assert_eq!(removed, 1);
        assert!(!dir.exists());
    }

    #[test]
    fn missing_branch_falls_back_to_the_checked_out_commit() {
        let root = tempfile::tempdir().unwrap();
        let repo = init_repo(root.path(), "demo");
        git_ok(&repo, &["checkout", "--detach", "HEAD"]);
        git_ok(&repo, &["update-ref", "-d", "refs/heads/main"]);
        let head = git_stdout(&repo, &["rev-parse", "HEAD"], Duration::from_secs(5)).unwrap();
        let mut repo_row = linked("demo", repo, false);
        repo_row.default_branch = "main".into();
        let session = Uuid::new_v4();
        let outcome = open_notes(
            &chat_dir(root.path(), session),
            session,
            &[repo_row],
            "demo",
            &ChatRepoLimits::default(),
        )
        .0
        .unwrap();
        assert_eq!(outcome.commit, head);
    }

    #[test]
    fn broken_repo_uses_the_failure_line() {
        let root = tempfile::tempdir().unwrap();
        let session = Uuid::new_v4();
        let repos = vec![linked("demo", root.path().join("missing-checkout"), true)];
        let (err, notes) = open_notes(
            &chat_dir(root.path(), session),
            session,
            &repos,
            "demo",
            &ChatRepoLimits::default(),
        );
        assert!(matches!(err, Err(OpenError::Failed { .. })));
        assert_eq!(notes, vec![opening_line("demo"), open_failed_line("demo")]);
    }

    #[test]
    fn focus_choice_prefers_the_most_used_repo() {
        let alpha = Uuid::from_u128(1);
        let beta = Uuid::from_u128(2);
        let chosen = choose_focus_repo(&[(beta, "Beta".into(), 1), (alpha, "Alpha".into(), 3)]);
        assert_eq!(chosen, Some(alpha));
        let tie = choose_focus_repo(&[(beta, "Beta".into(), 2), (alpha, "Alpha".into(), 2)]);
        assert_eq!(tie, Some(alpha));
    }

    #[test]
    fn duplicate_names_get_distinct_slugs() {
        let mut used = std::collections::HashSet::new();
        let first = assign_slug("Demo", Uuid::from_u128(1), &mut used);
        let second = assign_slug("Demo", Uuid::from_u128(2), &mut used);
        assert_eq!(first, "demo");
        assert_ne!(first, second);
        assert!(!second.contains('/'));
    }

    #[test]
    fn eager_copy_returns_before_the_copy_finishes() {
        let root = tempfile::tempdir().unwrap();
        let repo = init_repo(root.path(), "demo");
        let repos = vec![linked("demo", repo, true)];
        let limits = ChatRepoLimits {
            copy_pause: Duration::from_millis(400),
            ..ChatRepoLimits::default()
        };
        let started = Instant::now();
        let workspace =
            begin_chat_workspace(root.path(), Uuid::new_v4(), &repos, &[], &limits, |_| {})
                .unwrap();
        let returned = started.elapsed();
        assert!(
            returned < Duration::from_millis(250),
            "chat start waited {returned:?}"
        );
        let finish = workspace.eager.unwrap().join().unwrap();
        assert!(started.elapsed() >= Duration::from_millis(400));
        assert!(matches!(finish, EagerFinish::Opened(_)));
    }

    #[test]
    fn startup_many_large_and_broken_repos() {
        let root = tempfile::tempdir().unwrap();
        let mut many = Vec::new();
        let describe_started = Instant::now();
        for index in 0..40 {
            let path = root.path().join(format!("repo-{index}"));
            fs::create_dir_all(&path).unwrap();
            fs::write(
                path.join("README.md"),
                format!("Repository {index} is a small library.\n"),
            )
            .unwrap();
            let description = one_line_description(&path, "main");
            many.push(LinkedRepo {
                name: format!("Repo {index}"),
                slug: format!("repo-{index}"),
                local_path: path,
                default_branch: "main".into(),
                description,
                boards: vec!["Board".into()],
                focus: false,
            });
        }
        let described = describe_started.elapsed();
        let started = Instant::now();
        let workspace = begin_chat_workspace(
            root.path(),
            Uuid::new_v4(),
            &many,
            &[],
            &ChatRepoLimits::default(),
            |_| {},
        )
        .unwrap();
        let many_ms = started.elapsed().as_millis();
        assert!(workspace.eager.is_none());
        assert!(workspace.chat_dir.join(INDEX_FILE).exists());
        eprintln!(
            "CHAT_STARTUP many_repos_ms={many_ms} describe_ms={}",
            described.as_millis()
        );
        assert!(started.elapsed() < Duration::from_secs(2));

        let large_root = tempfile::tempdir().unwrap();
        let large_repo = large_root.path().join("huge");
        fs::create_dir_all(&large_repo).unwrap();
        git_ok(&large_repo, &["init", "-b", "main"]);
        let blob = large_repo.join("blob.bin");
        {
            let file = File::create(&blob).unwrap();
            file.set_len(70 * 1024 * 1024).unwrap();
        }
        fs::write(large_repo.join("README.md"), "Huge checkout.\n").unwrap();
        git_ok(&large_repo, &["add", "."]);
        git_ok(&large_repo, &["commit", "-m", "huge"]);
        let large = vec![linked("huge", large_repo, true)];
        let started = Instant::now();
        let workspace = begin_chat_workspace(
            large_root.path(),
            Uuid::new_v4(),
            &large,
            &[],
            &ChatRepoLimits::default(),
            |_| {},
        )
        .unwrap();
        let large_ms = started.elapsed().as_millis();
        eprintln!("CHAT_STARTUP large_repo_ms={large_ms}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "large start {large_ms} ms"
        );
        let finish = workspace.eager.unwrap().join().unwrap();
        assert!(
            matches!(finish, EagerFinish::SkippedTooLarge { .. }),
            "70 MiB board repo should stay on demand"
        );
        assert!(!workspace.chat_dir.join("huge").exists());

        let broken_root = tempfile::tempdir().unwrap();
        let notes = Arc::new(StdMutex::new(Vec::new()));
        let notes_cb = Arc::clone(&notes);
        let broken = vec![linked(
            "gone",
            broken_root.path().join("no-such-repo"),
            true,
        )];
        let started = Instant::now();
        let workspace = begin_chat_workspace(
            broken_root.path(),
            Uuid::new_v4(),
            &broken,
            &[],
            &ChatRepoLimits::default(),
            move |line| notes_cb.lock().unwrap().push(line.to_string()),
        )
        .unwrap();
        let broken_ms = started.elapsed().as_millis();
        eprintln!("CHAT_STARTUP broken_repo_ms={broken_ms}");
        assert!(started.elapsed() < Duration::from_secs(2));
        let finish = workspace.eager.unwrap().join().unwrap();
        assert!(matches!(finish, EagerFinish::Failed(_)));
        let lines = notes.lock().unwrap().clone();
        assert_eq!(lines, vec![opening_line("gone"), open_failed_line("gone")]);
    }

    #[test]
    fn git_timeout_kills_a_slow_command() {
        let err = run_git(None, &["--exec-path"], Duration::ZERO).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        #[cfg(unix)]
        {
            // `run_git` puts git in its own group; this checks the kill helper.
            unsafe {
                libc::setpgid(child.id() as i32, child.id() as i32);
            }
        }
        let started = Instant::now();
        kill_child(&mut child);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
