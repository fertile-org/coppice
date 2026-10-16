//! Load linked repositories and start, restore, or sweep per-chat copies.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use coppice_config::AgentConfig;
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::services::chat_repos::{
    self, assign_slug, begin_chat_workspace, chat_dir, choose_focus_repo, clear_swept_flag,
    copies_were_swept, one_line_description, open_repo, opened_slugs_for_continue, refresh_index,
    slugs_to_restore, touch_activity, ChatRepoLimits, LinkedRepo, INDEX_FILE, REPOS_READY_LINE,
};
use crate::services::chat_service::ChatService;
use crate::AppState;

pub fn limits_from_config(agent: &AgentConfig) -> ChatRepoLimits {
    ChatRepoLimits {
        open_repo_cap: agent.chat_open_repo_cap,
        eager_copy_timeout: Duration::from_secs(agent.chat_eager_copy_timeout_secs),
        copy_timeout: Duration::from_secs(agent.chat_copy_timeout_secs),
        copy_idle: Duration::from_secs(agent.chat_copy_idle_secs),
        chat_folder_max_age: Duration::from_secs(agent.chat_folder_max_age_secs),
        eager_copy_max_bytes: agent.chat_eager_copy_max_bytes,
        copy_pause: Duration::ZERO,
    }
}

pub async fn load_linked_repos(
    pool: &PgPool,
    board_id: Option<Uuid>,
) -> Result<Vec<LinkedRepo>, sqlx::Error> {
    let repo_rows = sqlx::query(
        r#"
        SELECT id, name, local_path, default_branch
        FROM repos
        ORDER BY name, id
        "#,
    )
    .fetch_all(pool)
    .await?;

    let board_rows = sqlx::query(
        r#"
        SELECT t.repo_id, b.name AS board_name
        FROM tickets t
        JOIN boards b ON b.id = t.board_id
        WHERE t.repo_id IS NOT NULL
        GROUP BY t.repo_id, b.name
        ORDER BY b.name
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut boards_by_repo: std::collections::HashMap<Uuid, Vec<String>> =
        std::collections::HashMap::new();
    for row in &board_rows {
        let repo_id: Uuid = row.get("repo_id");
        let name: String = row.get("board_name");
        boards_by_repo.entry(repo_id).or_default().push(name);
    }

    let focus = match board_id {
        Some(board_id) => {
            let counts = sqlx::query(
                r#"
                SELECT t.repo_id, r.name, COUNT(*) AS ticket_count
                FROM tickets t
                JOIN repos r ON r.id = t.repo_id
                WHERE t.board_id = $1 AND t.repo_id IS NOT NULL
                GROUP BY t.repo_id, r.name
                "#,
            )
            .bind(board_id)
            .fetch_all(pool)
            .await?;
            let counts: Vec<(Uuid, String, i64)> = counts
                .iter()
                .map(|row| (row.get("repo_id"), row.get("name"), row.get("ticket_count")))
                .collect();
            choose_focus_repo(&counts)
        }
        None => None,
    };

    let mut used = std::collections::HashSet::new();
    let mut repos = Vec::with_capacity(repo_rows.len());
    for row in &repo_rows {
        let id: Uuid = row.get("id");
        let name: String = row.get("name");
        let local_path: String = row.get("local_path");
        let default_branch: String = row.get("default_branch");
        let path = Path::new(&local_path).to_path_buf();
        let description = one_line_description(&path, &default_branch);
        repos.push(LinkedRepo {
            slug: assign_slug(&name, id, &mut used),
            name,
            local_path: path,
            default_branch,
            description,
            boards: boards_by_repo.remove(&id).unwrap_or_default(),
            focus: focus == Some(id),
        });
    }
    Ok(repos)
}

/// Write the index and copy the board repository in the background.
pub fn start_session(
    state: &AppState,
    pool: PgPool,
    session_id: Uuid,
    board_id: Option<Uuid>,
    carried: &[String],
) {
    let worktrees = state.config.agent.worktrees_path.clone();
    let limits = limits_from_config(&state.config.agent);
    let carried = carried.to_vec();
    tokio::spawn(async move {
        let repos = match load_linked_repos(&pool, board_id).await {
            Ok(repos) => repos,
            Err(err) => {
                tracing::warn!(%session_id, error = %err, "failed to list repositories for a new chat");
                return;
            }
        };
        let notes = note_sender(pool, session_id);
        if let Err(err) = begin_chat_workspace(
            Path::new(&worktrees),
            session_id,
            &repos,
            &carried,
            &limits,
            notes,
        ) {
            tracing::warn!(%session_id, error = %err, "failed to prepare chat repositories");
        }
    });
}

/// On a chat turn: rebuild copies that idle cleanup removed, and recreate a
/// missing index without waiting on a large checkout.
pub async fn on_chat_turn(
    state: &AppState,
    pool: &PgPool,
    session_id: Uuid,
    board_id: Option<Uuid>,
) {
    let root = Path::new(&state.config.agent.worktrees_path);
    let dir = chat_dir(root, session_id);
    let limits = limits_from_config(&state.config.agent);
    let _ = touch_activity(&dir);

    let repos = match load_linked_repos(pool, board_id).await {
        Ok(repos) => repos,
        Err(err) => {
            tracing::warn!(%session_id, error = %err, "failed to list repositories for chat");
            return;
        }
    };

    if copies_were_swept(&dir) {
        let _ = ChatService::new(pool)
            .append_system_note(session_id, REPOS_READY_LINE)
            .await;
        let slugs = slugs_to_restore(&dir);
        let _ = clear_swept_flag(&dir);
        let _ = refresh_index(&dir, &repos);
        spawn_restore(pool.clone(), dir, session_id, repos, slugs, limits);
        return;
    }

    if !dir.join(INDEX_FILE).is_file() {
        // A brand-new chat has a single human message by the time its first
        // turn runs. More than that means the folder was swept and the person
        // came back.
        let returning = message_count(pool, session_id).await.unwrap_or(0) > 1;
        if returning {
            let _ = ChatService::new(pool)
                .append_system_note(session_id, REPOS_READY_LINE)
                .await;
        }
        let notes = note_sender(pool.clone(), session_id);
        if let Err(err) = begin_chat_workspace(root, session_id, &repos, &[], &limits, notes) {
            tracing::warn!(%session_id, error = %err, "failed to rebuild chat folder");
        }
    }
}

pub fn continue_slugs(worktrees: &Path, session_id: Uuid) -> Vec<String> {
    opened_slugs_for_continue(&chat_dir(worktrees, session_id))
}

pub fn spawn_sweep(state: Arc<AppState>) {
    tokio::spawn(async move {
        sweep(&state);
        loop {
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            sweep(&state);
        }
    });
}

fn sweep(state: &AppState) {
    let root = Path::new(&state.config.agent.worktrees_path);
    let limits = limits_from_config(&state.config.agent);
    let now = SystemTime::now();
    if let Err(err) = chat_repos::sweep_idle_copies(root, limits.copy_idle, now) {
        tracing::warn!(error = %err, "failed to sweep idle chat copies");
    }
    if let Err(err) = chat_repos::sweep_aged_chat_folders(root, limits.chat_folder_max_age, now) {
        tracing::warn!(error = %err, "failed to sweep old chat folders");
    }
}

fn spawn_restore(
    pool: PgPool,
    dir: std::path::PathBuf,
    session_id: Uuid,
    repos: Vec<LinkedRepo>,
    slugs: Vec<String>,
    limits: ChatRepoLimits,
) {
    let mut notes = note_sender(pool, session_id);
    std::thread::Builder::new()
        .name("chat-repo-restore".into())
        .spawn(move || {
            for slug in slugs {
                if let Err(err) = open_repo(&dir, session_id, &repos, &slug, &limits, &mut notes) {
                    tracing::info!(%session_id, slug = %slug, error = %err, "chat copy was not restored");
                }
            }
        })
        .ok();
}

fn note_sender(pool: PgPool, session_id: Uuid) -> impl FnMut(&str) + Send + 'static {
    let handle = tokio::runtime::Handle::current();
    move |line: &str| {
        let line = line.to_string();
        let pool = pool.clone();
        handle.block_on(async move {
            if let Err(err) = ChatService::new(&pool)
                .append_system_note(session_id, &line)
                .await
            {
                tracing::warn!(%session_id, error = %err, "failed to record chat repository note");
            }
        });
    }
}

async fn message_count(pool: &PgPool, session_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE session_id = $1")
        .bind(session_id)
        .fetch_one(pool)
        .await
}
