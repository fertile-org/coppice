//! Staged copies of a ticket's description and comment attachments.
//!
//! The bytes live in the attachment store, outside the worktree, and
//! `ticket_comments` does not return a path. Sandboxed CLIs (Claude's
//! workspace boundary, Codex `--sandbox`, OpenCode `external_directory`)
//! cannot read that store, and Codex does not treat a path as an image unless
//! it is passed with `-i`. Each run copies the files into
//! `{artifacts}/ticket-attachments/{ticket_id}/`, which is never inside the
//! worktree, lists the absolute paths in the prompt, and grants that directory
//! to the CLI. The directory is removed after a successful merge or when the
//! ticket is deleted. Accept and a failed merge leave it in place: a failed
//! merge returns the ticket to In Progress.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::attachment::Attachment;
use crate::services::comment_service::CommentService;
use crate::storage::attachment_store::sanitize_filename;

/// Why a ticket's staged attachment directory might be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentFolderEnd {
    /// The ticket branch merged. The copies are no longer needed.
    MergeSucceeded,
    /// The ticket row is gone.
    TicketDeleted,
    /// Human review accepted the diff. A later merge can still fail and send
    /// the ticket back to In Progress, so the copies stay.
    Accepted,
    /// Merge aborted. The ticket goes back to In Progress.
    MergeFailed,
}

pub fn remove_folder_on(end: AttachmentFolderEnd) -> bool {
    matches!(
        end,
        AttachmentFolderEnd::MergeSucceeded | AttachmentFolderEnd::TicketDeleted
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StagedTicketFile {
    pub filename: String,
    pub content_type: String,
    pub absolute_path: String,
    /// `description` or `comment`.
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TicketAttachmentLaunch {
    pub dir: Option<String>,
    pub files: Vec<StagedTicketFile>,
}

pub struct SourceAttachment {
    pub id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub storage_path: PathBuf,
    pub origin: &'static str,
}

pub fn ticket_attachment_dir(artifacts_dir: &str, ticket_id: Uuid) -> io::Result<PathBuf> {
    crate::providers::artifacts_path(
        artifacts_dir,
        &["ticket-attachments", &ticket_id.to_string()],
    )
    .map_err(io::Error::other)
}

pub fn is_vision_image(content_type: &str) -> bool {
    let base = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim();
    matches!(
        base,
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp"
    )
}

/// Copy `sources` into the per-ticket directory outside `worktree` and write
/// `manifest.json` for the connector launch. An empty source list removes a
/// directory left by an earlier run.
pub fn stage_files(
    artifacts_dir: &str,
    ticket_id: Uuid,
    worktree: &Path,
    sources: &[SourceAttachment],
) -> io::Result<TicketAttachmentLaunch> {
    let dir = ticket_attachment_dir(artifacts_dir, ticket_id)?;
    ensure_outside_worktree(&dir, worktree)?;
    if sources.is_empty() {
        clear_dir(&dir)?;
        return Ok(TicketAttachmentLaunch::default());
    }

    std::fs::create_dir_all(&dir)?;
    let mut files = Vec::with_capacity(sources.len());
    let mut keep_ids = HashSet::new();
    for source in sources {
        let safe = sanitize_filename(&source.filename);
        let dest_dir = dir.join(source.id.to_string());
        std::fs::create_dir_all(&dest_dir)?;
        let dest = dest_dir.join(&safe);
        match std::fs::copy(&source.storage_path, &dest) {
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                tracing::warn!(
                    attachment_id = %source.id,
                    path = %source.storage_path.display(),
                    "ticket attachment file is missing; skipping"
                );
                continue;
            }
            Err(err) => return Err(err),
        }
        keep_ids.insert(source.id.to_string());
        let absolute = std::path::absolute(&dest)?;
        files.push(StagedTicketFile {
            filename: safe,
            content_type: source.content_type.clone(),
            absolute_path: absolute.display().to_string(),
            origin: source.origin.to_string(),
        });
    }
    remove_stale_children(&dir, &keep_ids)?;
    if files.is_empty() {
        clear_dir(&dir)?;
        return Ok(TicketAttachmentLaunch::default());
    }
    let launch = TicketAttachmentLaunch {
        dir: Some(std::path::absolute(&dir)?.display().to_string()),
        files,
    };
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(&launch).map_err(io::Error::other)?,
    )?;
    Ok(launch)
}

pub async fn stage_ticket_attachments(
    pool: &PgPool,
    artifacts_dir: &str,
    worktree: &Path,
    ticket_id: Uuid,
) -> anyhow::Result<TicketAttachmentLaunch> {
    let sources = load_sources(pool, ticket_id).await?;
    Ok(stage_files(artifacts_dir, ticket_id, worktree, &sources)?)
}

pub fn load_for_run(
    artifacts_dir: Option<&str>,
    ticket_id: Option<&str>,
) -> TicketAttachmentLaunch {
    let (Some(artifacts_dir), Some(ticket_id)) = (artifacts_dir, ticket_id) else {
        return TicketAttachmentLaunch::default();
    };
    let Ok(ticket_id) = Uuid::parse_str(ticket_id) else {
        return TicketAttachmentLaunch::default();
    };
    let Ok(dir) = ticket_attachment_dir(artifacts_dir, ticket_id) else {
        return TicketAttachmentLaunch::default();
    };
    let Ok(raw) = std::fs::read(dir.join("manifest.json")) else {
        return TicketAttachmentLaunch::default();
    };
    serde_json::from_slice(&raw).unwrap_or_default()
}

pub fn section(launch: &TicketAttachmentLaunch) -> Option<String> {
    if launch.files.is_empty() {
        return None;
    }
    let mut out = String::from(
        "# Ticket attachments\n\n\
         Read these files by absolute path. They are outside the worktree.\n\n",
    );
    for file in &launch.files {
        out.push_str(&format!(
            "- {} ({}): {}\n",
            file.filename, file.origin, file.absolute_path
        ));
    }
    Some(out)
}

pub fn append_section(markdown: &mut String, launch: &TicketAttachmentLaunch) {
    let Some(section) = section(launch) else {
        return;
    };
    if !markdown.ends_with('\n') {
        markdown.push('\n');
    }
    markdown.push('\n');
    markdown.push_str(&section);
}

pub fn launch_prompt(base: &str, launch: &TicketAttachmentLaunch) -> String {
    let Some(section) = section(launch) else {
        return base.to_string();
    };
    let mut out = base.trim_end().to_string();
    out.push_str("\n\n");
    out.push_str(&section);
    out
}

/// `--add-dir` for a CLI whose sandbox otherwise cannot read outside the worktree.
/// Empty when this run staged no files.
pub fn add_dir_args(launch: &TicketAttachmentLaunch) -> Vec<String> {
    match external_read_dir(launch) {
        Some(dir) => vec!["--add-dir".to_string(), dir],
        None => Vec::new(),
    }
}

/// `-i` once per vision image. Text files stay in the prompt only.
pub fn image_input_args(launch: &TicketAttachmentLaunch) -> Vec<String> {
    let mut args = Vec::new();
    for file in &launch.files {
        if is_vision_image(&file.content_type) {
            args.push("-i".to_string());
            args.push(file.absolute_path.clone());
        }
    }
    args
}

/// Directory an OpenCode-family config must allow (`external_directory`).
pub fn external_read_dir(launch: &TicketAttachmentLaunch) -> Option<String> {
    launch.dir.clone().filter(|_| !launch.files.is_empty())
}

/// Allow `dir` in an OpenCode or Kilo config. Other keys are left as they are.
pub fn with_attachment_read(config: &str, dir: Option<&str>) -> String {
    let Some(dir) = dir.map(str::trim).filter(|dir| !dir.is_empty()) else {
        return config.to_string();
    };
    let mut value: serde_json::Value = match serde_json::from_str::<serde_json::Value>(config) {
        Ok(value) if value.is_object() => value,
        _ => return config.to_string(),
    };
    let pattern = format!("{}/**", dir.trim_end_matches('/'));
    let permission = value
        .as_object_mut()
        .expect("object")
        .entry("permission")
        .or_insert_with(|| serde_json::json!({}));
    if !permission.is_object() {
        *permission = serde_json::json!({});
    }
    let external = permission
        .as_object_mut()
        .expect("permission")
        .entry("external_directory")
        .or_insert_with(|| serde_json::json!({}));
    if !external.is_object() {
        *external = serde_json::json!({});
    }
    external
        .as_object_mut()
        .expect("external_directory")
        .insert(pattern, serde_json::json!("allow"));
    value.to_string()
}

/// Point `env_key` (for example `KILO_CONFIG`) at a config that allows `dir`.
pub fn apply_external_read_env(
    env: &mut Vec<(String, String)>,
    env_key: &str,
    dir: &str,
    artifacts_dir: Option<&str>,
    run_id: Option<&str>,
) -> io::Result<()> {
    if let Some((_, path)) = env.iter().find(|(key, _)| key == env_key) {
        let path = PathBuf::from(path);
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
        std::fs::write(&path, with_attachment_read(&raw, Some(dir)))?;
        return Ok(());
    }
    let (Some(artifacts_dir), Some(run_id)) = (artifacts_dir, run_id) else {
        return Ok(());
    };
    let path =
        crate::providers::artifacts_path(artifacts_dir, &["runs", run_id, "kilo-config.json"])
            .map_err(io::Error::other)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, with_attachment_read("{}", Some(dir)))?;
    env.push((env_key.to_string(), path.display().to_string()));
    Ok(())
}

pub fn clear_staged_ticket_attachments(artifacts_dir: &str, ticket_id: Uuid) -> io::Result<()> {
    let dir = ticket_attachment_dir(artifacts_dir, ticket_id)?;
    clear_dir(&dir)
}

pub fn clear_if_ended(
    artifacts_dir: &str,
    ticket_id: Uuid,
    end: AttachmentFolderEnd,
) -> io::Result<bool> {
    if !remove_folder_on(end) {
        return Ok(false);
    }
    clear_staged_ticket_attachments(artifacts_dir, ticket_id)?;
    Ok(true)
}

/// Staged copies go away with the ticket. Accept does not call this.
pub fn on_ticket_deleted(artifacts_dir: &str, ticket_id: Uuid) -> io::Result<()> {
    clear_if_ended(artifacts_dir, ticket_id, AttachmentFolderEnd::TicketDeleted)?;
    Ok(())
}

async fn load_sources(pool: &PgPool, ticket_id: Uuid) -> anyhow::Result<Vec<SourceAttachment>> {
    let description_ids =
        sqlx::query_scalar::<_, Vec<Uuid>>("SELECT attachment_ids FROM tickets WHERE id = $1")
            .bind(ticket_id)
            .fetch_optional(pool)
            .await?
            .unwrap_or_default();

    let comment_rows = sqlx::query_scalar::<_, Vec<Uuid>>(
        r#"
        SELECT attachment_ids FROM ticket_comments
        WHERE ticket_id = $1 AND cardinality(attachment_ids) > 0
        ORDER BY created_at ASC
        "#,
    )
    .bind(ticket_id)
    .fetch_all(pool)
    .await?;

    let mut ordered: Vec<(Uuid, &'static str)> = Vec::new();
    let mut seen = HashSet::new();
    for id in description_ids {
        if seen.insert(id) {
            ordered.push((id, "description"));
        }
    }
    for ids in comment_rows {
        for id in ids {
            if seen.insert(id) {
                ordered.push((id, "comment"));
            }
        }
    }
    if ordered.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<Uuid> = ordered.iter().map(|(id, _)| *id).collect();
    let attachments = CommentService::new(pool)
        .list_attachments_by_ids(&ids)
        .await?;
    let by_id: std::collections::HashMap<Uuid, Attachment> =
        attachments.into_iter().map(|att| (att.id, att)).collect();

    let mut sources = Vec::new();
    for (id, origin) in ordered {
        let Some(att) = by_id.get(&id) else {
            continue;
        };
        sources.push(SourceAttachment {
            id: att.id,
            filename: att.filename.clone(),
            content_type: att.content_type.clone(),
            storage_path: PathBuf::from(&att.storage_path),
            origin,
        });
    }
    Ok(sources)
}

fn ensure_outside_worktree(dest: &Path, worktree: &Path) -> io::Result<()> {
    let dest = std::path::absolute(dest)?;
    let worktree = std::path::absolute(worktree)?;
    if dest.starts_with(&worktree) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ticket attachments must be staged outside the worktree",
        ));
    }
    Ok(())
}

fn clear_dir(dir: &Path) -> io::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn remove_stale_children(dir: &Path, keep_ids: &HashSet<String>) -> io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "manifest.json" || keep_ids.contains(name.as_ref()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::opencode_events::coppice_run_prompt;

    const NOTES: &str = "hello from the ticket";
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-ticket-image";

    struct Sample {
        _artifacts: tempfile::TempDir,
        _worktree: tempfile::TempDir,
        artifacts_path: PathBuf,
        launch: TicketAttachmentLaunch,
    }

    fn sample() -> Sample {
        let artifacts = tempfile::tempdir().expect("artifacts");
        let worktree = tempfile::tempdir().expect("worktree");
        let store = tempfile::tempdir().expect("store");
        let notes_id = Uuid::new_v4();
        let image_id = Uuid::new_v4();
        let notes = store.path().join("notes.txt");
        let image = store.path().join("shot.png");
        std::fs::write(&notes, NOTES).unwrap();
        std::fs::write(&image, PNG).unwrap();
        let launch = stage_files(
            artifacts.path().to_str().unwrap(),
            Uuid::new_v4(),
            worktree.path(),
            &[
                SourceAttachment {
                    id: image_id,
                    filename: "shot.png".into(),
                    content_type: "image/png".into(),
                    storage_path: image,
                    origin: "description",
                },
                SourceAttachment {
                    id: notes_id,
                    filename: "notes.txt".into(),
                    content_type: "text/plain".into(),
                    storage_path: notes,
                    origin: "comment",
                },
            ],
        )
        .expect("stage");
        Sample {
            artifacts_path: artifacts.path().to_path_buf(),
            _artifacts: artifacts,
            _worktree: worktree,
            launch,
        }
    }

    fn path_listed_for(prompt: &str, filename: &str) -> String {
        let line = prompt
            .lines()
            .find(|line| line.contains(filename))
            .unwrap_or_else(|| panic!("prompt missing {filename}: {prompt}"));
        line.split(": ")
            .nth(1)
            .unwrap_or_else(|| panic!("no path on {line}"))
            .trim()
            .to_string()
    }

    #[test]
    fn staged_files_sit_outside_the_worktree_and_the_prompt_path_is_readable() {
        let sample = sample();
        let worktree = sample._worktree.path();
        let dir = Path::new(sample.launch.dir.as_deref().unwrap());
        assert!(!dir.starts_with(worktree), "{dir:?} inside {worktree:?}");
        assert!(dir.starts_with(&sample.artifacts_path));
        let prompt = launch_prompt(coppice_run_prompt(), &sample.launch);
        let notes = path_listed_for(&prompt, "notes.txt");
        let image = path_listed_for(&prompt, "shot.png");
        assert!(prompt.contains("notes.txt (comment)"));
        assert!(prompt.contains("shot.png (description)"));
        assert_eq!(std::fs::read_to_string(&notes).unwrap(), NOTES);
        assert_eq!(std::fs::read(&image).unwrap(), PNG);
        assert!(!notes.starts_with(worktree.to_str().unwrap()));
    }

    #[test]
    fn staging_inside_the_worktree_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let err = stage_files(
            root.path().join("inside").to_str().unwrap(),
            Uuid::new_v4(),
            root.path(),
            &[],
        )
        .expect_err("inside worktree");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn accept_and_a_failed_merge_keep_the_folder() {
        let sample = sample();
        let dir = sample.launch.dir.clone().unwrap();
        let ticket_id = dir
            .rsplit('/')
            .next()
            .and_then(|id| Uuid::parse_str(id).ok())
            .unwrap();
        let artifacts = sample.artifacts_path.to_str().unwrap();
        assert!(!clear_if_ended(artifacts, ticket_id, AttachmentFolderEnd::Accepted).unwrap());
        assert!(Path::new(&dir).exists());
        assert!(!clear_if_ended(artifacts, ticket_id, AttachmentFolderEnd::MergeFailed).unwrap());
        assert!(Path::new(&dir).exists());
        assert!(clear_if_ended(artifacts, ticket_id, AttachmentFolderEnd::MergeSucceeded).unwrap());
        assert!(!Path::new(&dir).exists());
    }

    #[test]
    fn deleting_the_ticket_removes_the_staged_folder() {
        let sample = sample();
        let dir = sample.launch.dir.clone().unwrap();
        let ticket_id =
            Uuid::parse_str(Path::new(&dir).file_name().unwrap().to_str().unwrap()).unwrap();
        on_ticket_deleted(sample.artifacts_path.to_str().unwrap(), ticket_id).unwrap();
        assert!(!Path::new(&dir).exists());
    }

    fn assert_readable_prompt(sample: &Sample) {
        let prompt = launch_prompt(coppice_run_prompt(), &sample.launch);
        let notes = path_listed_for(&prompt, "notes.txt");
        assert_eq!(std::fs::read_to_string(notes).unwrap(), NOTES);
        assert!(prompt.contains("shot.png"));
        let loaded = load_for_run(
            Some(sample.artifacts_path.to_str().unwrap()),
            Some(
                Path::new(sample.launch.dir.as_deref().unwrap())
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap(),
            ),
        );
        assert_eq!(loaded.files.len(), sample.launch.files.len());
    }

    fn assert_add_dir(sample: &Sample, args: &[String]) {
        assert!(args.windows(2).any(|pair| {
            pair[0] == "--add-dir" && pair[1] == sample.launch.dir.as_deref().unwrap()
        }));
    }

    #[test]
    fn claude_code_prompt_and_args_include_the_readable_path() {
        let sample = sample();
        assert_readable_prompt(&sample);
        let args = add_dir_args(&sample.launch);
        assert_add_dir(&sample, &args);
        assert!(!args.iter().any(|arg| arg == "-i"));
    }

    #[test]
    fn codex_prompt_args_include_the_path_and_native_image_flag() {
        let sample = sample();
        assert_readable_prompt(&sample);
        let mut args = add_dir_args(&sample.launch);
        args.extend(image_input_args(&sample.launch));
        assert_add_dir(&sample, &args);
        let image = sample
            .launch
            .files
            .iter()
            .find(|file| file.filename == "shot.png")
            .unwrap();
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "-i" && pair[1] == image.absolute_path));
        assert!(!args.iter().any(|arg| arg.contains("notes.txt")));
    }

    #[test]
    fn opencode_prompt_lists_the_path_and_config_allows_the_directory() {
        let sample = sample();
        assert_readable_prompt(&sample);
        let dir = external_read_dir(&sample.launch).unwrap();
        let read_only = crate::sessions::opencode_run_server::opencode_run_config(None, true);
        let patched = with_attachment_read(&read_only, Some(&dir));
        let value: serde_json::Value = serde_json::from_str(&patched).unwrap();
        assert_eq!(value["permission"]["edit"], "deny");
        assert_eq!(value["permission"]["bash"], "deny");
        assert_eq!(
            value["permission"]["external_directory"][format!("{dir}/**")],
            "allow"
        );
        let open = crate::sessions::opencode_run_server::opencode_run_config(None, false);
        let patched = with_attachment_read(&open, Some(&dir));
        let value: serde_json::Value = serde_json::from_str(&patched).unwrap();
        assert_eq!(
            value["permission"]["external_directory"][format!("{dir}/**")],
            "allow"
        );
    }

    #[test]
    fn cursor_prompt_and_args_include_the_readable_path() {
        let sample = sample();
        assert_readable_prompt(&sample);
        assert_add_dir(&sample, &add_dir_args(&sample.launch));
    }

    #[test]
    fn kilo_prompt_lists_the_path_and_config_allows_the_directory() {
        let sample = sample();
        assert_readable_prompt(&sample);
        let dir = external_read_dir(&sample.launch).unwrap();
        let raw = r#"{"mcp":{"coppice":{"type":"remote","url":"http://127.0.0.1:9/mcp"}}}"#;
        let patched = with_attachment_read(raw, Some(&dir));
        let value: serde_json::Value = serde_json::from_str(&patched).unwrap();
        assert_eq!(value["mcp"]["coppice"]["type"], "remote");
        assert_eq!(
            value["permission"]["external_directory"][format!("{dir}/**")],
            "allow"
        );
    }

    #[tokio::test]
    async fn description_and_comment_attachments_are_both_staged() {
        let pool = match crate::db::shared_test_pool().await {
            Ok(pool) => pool,
            Err(_) => return,
        };
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role) VALUES ($1, $2, 'x', 'admin')",
        )
        .bind(user_id)
        .bind(format!("attach-{user_id}@example.com"))
        .execute(&pool)
        .await
        .unwrap();
        let board = crate::services::board_service::BoardService::new(&pool)
            .create_board(&format!("board-{user_id}"))
            .await
            .unwrap();
        let ticket = crate::services::ticket_service::TicketService::new(&pool)
            .create(board.id, "With files", "", None, None, "test", user_id)
            .await
            .unwrap();

        let store = tempfile::tempdir().unwrap();
        let image_id = Uuid::new_v4();
        let notes_id = Uuid::new_v4();
        let image_src = store.path().join("shot.png");
        let notes_src = store.path().join("notes.txt");
        std::fs::write(&image_src, PNG).unwrap();
        std::fs::write(&notes_src, NOTES).unwrap();
        let comments = CommentService::new(&pool);
        comments
            .create_attachment(
                image_id,
                "shot.png",
                "image/png",
                PNG.len() as i64,
                image_src.to_str().unwrap(),
                user_id,
            )
            .await
            .unwrap();
        comments
            .create_attachment(
                notes_id,
                "notes.txt",
                "text/plain",
                NOTES.len() as i64,
                notes_src.to_str().unwrap(),
                user_id,
            )
            .await
            .unwrap();
        sqlx::query("UPDATE tickets SET attachment_ids = $1 WHERE id = $2")
            .bind(vec![image_id])
            .bind(ticket.ticket.id)
            .execute(&pool)
            .await
            .unwrap();
        comments
            .create(
                ticket.ticket.id,
                crate::domain::comment::AuthorType::Human,
                Some(user_id),
                "see the notes",
                crate::domain::comment::CommentIntent::ProgressUpdate,
                &[notes_id],
                &[],
            )
            .await
            .unwrap();

        let artifacts = tempfile::tempdir().unwrap();
        let worktree = tempfile::tempdir().unwrap();
        let launch = stage_ticket_attachments(
            &pool,
            artifacts.path().to_str().unwrap(),
            worktree.path(),
            ticket.ticket.id,
        )
        .await
        .unwrap();
        let prompt = launch_prompt(coppice_run_prompt(), &launch);
        let mut args = add_dir_args(&launch);
        args.extend(image_input_args(&launch));
        let notes = path_listed_for(&prompt, "notes.txt");
        let image = path_listed_for(&prompt, "shot.png");
        assert!(notes.starts_with(artifacts.path().to_str().unwrap()));
        assert!(!notes.starts_with(worktree.path().to_str().unwrap()));
        assert_eq!(std::fs::read_to_string(&notes).unwrap(), NOTES);
        assert_eq!(std::fs::read(&image).unwrap(), PNG);
        assert!(prompt.contains("shot.png (description)"));
        assert!(prompt.contains("notes.txt (comment)"));
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "-i" && pair[1] == image));
        assert!(args.windows(2).any(|pair| pair[0] == "--add-dir"));
    }
}
