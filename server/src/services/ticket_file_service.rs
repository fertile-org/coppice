//! Read-only file previews for paths mentioned in a ticket.
//!
//! A ticket's files come from its worktree when one exists (the agent's work in
//! progress), otherwise from the registered repo checkout. Only previewable
//! kinds are served; add a variant to [`FilePreviewKind`] to support more.

use std::path::{Path, PathBuf};

use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::repo::VerificationStatus;
use crate::services::code_review_service::validate_diff_file_path;
use crate::services::repo_service::{RepoError, RepoService};
use crate::services::ticket_service::{TicketError, TicketService};
use crate::services::worktree_service::compute_paths;

pub const MAX_TEXT_PREVIEW_BYTES: u64 = 1024 * 1024;
pub const MAX_IMAGE_PREVIEW_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum TicketFileError {
    #[error("ticket not found")]
    TicketNotFound,
    #[error("ticket has no repository")]
    NoRepo,
    #[error("repository is not ready")]
    RepoNotReady,
    #[error("invalid file path")]
    InvalidPath,
    #[error("file not found")]
    NotFound,
    #[error("path matches several files")]
    Ambiguous,
    #[error("file type cannot be previewed")]
    Unsupported,
    #[error("file too large to preview")]
    TooLarge,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePreviewKind {
    Image,
    Text,
}

impl FilePreviewKind {
    fn max_bytes(self) -> u64 {
        match self {
            FilePreviewKind::Image => MAX_IMAGE_PREVIEW_BYTES,
            FilePreviewKind::Text => MAX_TEXT_PREVIEW_BYTES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSource {
    Worktree,
    Repo,
}

impl FileSource {
    pub fn as_str(self) -> &'static str {
        match self {
            FileSource::Worktree => "worktree",
            FileSource::Repo => "repo",
        }
    }
}

#[derive(Debug)]
pub struct TicketFile {
    pub kind: FilePreviewKind,
    pub content_type: &'static str,
    pub source: FileSource,
    /// Repo-relative path that was actually served (may be a suffix match).
    pub resolved_path: String,
    pub bytes: Vec<u8>,
}

/// Image content type by extension; `None` means "try it as text".
pub fn image_content_type(path: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        _ => return None,
    })
}

/// Normalizes a mentioned path to a repo-relative one. Rejects absolute paths,
/// traversal, and anything inside `.git` (its config can hold remote credentials).
pub fn normalize_preview_path(path: &str) -> Result<String, TicketFileError> {
    let trimmed = path.trim();
    let relative = trimmed.strip_prefix("./").unwrap_or(trimmed);
    validate_diff_file_path(relative).map_err(|_| TicketFileError::InvalidPath)?;
    if relative
        .split('/')
        .any(|seg| seg == ".git" || seg.is_empty())
    {
        return Err(TicketFileError::InvalidPath);
    }
    Ok(relative.to_string())
}

fn looks_like_text(bytes: &[u8]) -> bool {
    !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

fn is_path_suffix(candidate: &str, requested: &str) -> bool {
    candidate == requested || candidate.ends_with(&format!("/{requested}"))
}

fn has_git_segment(path: &str) -> bool {
    path.split('/').any(|seg| seg == ".git" || seg.is_empty())
}

enum ExactLookup {
    Found(PathBuf),
    Escaped,
    Missing,
}

fn try_exact(canonical_root: &Path, relative: &str) -> ExactLookup {
    match canonical_root.join(relative).canonicalize() {
        Ok(file) if !file.starts_with(canonical_root) => ExactLookup::Escaped,
        Ok(file) if file.is_file() => ExactLookup::Found(file),
        Ok(_) | Err(_) => ExactLookup::Missing,
    }
}

async fn resolve_file(
    canonical_root: &Path,
    git_cwd: &Path,
    relative: &str,
) -> Result<(PathBuf, String), TicketFileError> {
    match try_exact(canonical_root, relative) {
        ExactLookup::Found(file) => Ok((file, relative.to_string())),
        ExactLookup::Escaped => Err(TicketFileError::InvalidPath),
        ExactLookup::Missing => resolve_suffix(canonical_root, git_cwd, relative).await,
    }
}

async fn resolve_suffix(
    canonical_root: &Path,
    git_cwd: &Path,
    relative: &str,
) -> Result<(PathBuf, String), TicketFileError> {
    let output = match crate::services::git_cmd::output(
        Some(git_cwd),
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )
    .await
    {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            tracing::warn!(
                code = ?output.status.code(),
                "git ls-files failed for file preview"
            );
            return Err(TicketFileError::NotFound);
        }
        Err(err) => {
            tracing::warn!(error = %err, "git ls-files failed for file preview");
            return Err(TicketFileError::NotFound);
        }
    };

    let mut matches = Vec::new();
    for raw in output.stdout.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let Ok(candidate) = std::str::from_utf8(raw) else {
            continue;
        };
        if !is_path_suffix(candidate, relative) || has_git_segment(candidate) {
            continue;
        }
        let Ok(file) = canonical_root.join(candidate).canonicalize() else {
            continue;
        };
        if !file.starts_with(canonical_root) {
            continue;
        }
        let Ok(metadata) = tokio::fs::metadata(&file).await else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        matches.push((file, candidate.to_string()));
    }

    match matches.len() {
        0 => Err(TicketFileError::NotFound),
        1 => Ok(matches.remove(0)),
        _ => Err(TicketFileError::Ambiguous),
    }
}

pub struct TicketFileService<'a> {
    pool: &'a PgPool,
    worktrees_root: PathBuf,
}

impl<'a> TicketFileService<'a> {
    pub fn new(pool: &'a PgPool, worktrees_root: PathBuf) -> Self {
        Self {
            pool,
            worktrees_root,
        }
    }

    pub async fn read(&self, ticket_id: Uuid, path: &str) -> Result<TicketFile, TicketFileError> {
        let relative = normalize_preview_path(path)?;
        let (root, source) = self.resolve_root(ticket_id).await?;
        let canonical_root = root.canonicalize()?;
        let (file, resolved_path) = resolve_file(&canonical_root, &root, &relative).await?;
        let metadata = tokio::fs::metadata(&file)
            .await
            .map_err(|_| TicketFileError::NotFound)?;
        if !metadata.is_file() {
            return Err(TicketFileError::NotFound);
        }

        let image_type = image_content_type(&resolved_path);
        let kind = if image_type.is_some() {
            FilePreviewKind::Image
        } else {
            FilePreviewKind::Text
        };
        if metadata.len() > kind.max_bytes() {
            return Err(TicketFileError::TooLarge);
        }
        let bytes = tokio::fs::read(&file).await?;
        let content_type = match image_type {
            Some(content_type) => content_type,
            None if looks_like_text(&bytes) => "text/plain; charset=utf-8",
            None => return Err(TicketFileError::Unsupported),
        };
        Ok(TicketFile {
            kind,
            content_type,
            source,
            resolved_path,
            bytes,
        })
    }

    async fn resolve_root(
        &self,
        ticket_id: Uuid,
    ) -> Result<(PathBuf, FileSource), TicketFileError> {
        let ticket =
            TicketService::new(self.pool)
                .get(ticket_id)
                .await
                .map_err(|err| match err {
                    TicketError::Database(err) => TicketFileError::Database(err),
                    _ => TicketFileError::TicketNotFound,
                })?;
        let repo_id = ticket.ticket.repo_id.ok_or(TicketFileError::NoRepo)?;
        let repo = RepoService::new(self.pool)
            .get(repo_id)
            .await
            .map_err(|err| match err {
                RepoError::Database(err) => TicketFileError::Database(err),
                _ => TicketFileError::NoRepo,
            })?;
        if repo.verification_status != VerificationStatus::Ready {
            return Err(TicketFileError::RepoNotReady);
        }
        let worktree = compute_paths(&self.worktrees_root, &repo.name, ticket_id).worktree_dir;
        if worktree.join(".git").exists() {
            return Ok((worktree, FileSource::Worktree));
        }
        Ok((PathBuf::from(repo.local_path), FileSource::Repo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_preview_path_accepts_relative_paths() {
        assert_eq!(
            normalize_preview_path("src/main.rs").unwrap(),
            "src/main.rs"
        );
        assert_eq!(normalize_preview_path("./README.md").unwrap(), "README.md");
    }

    #[test]
    fn normalize_preview_path_rejects_unsafe_paths() {
        for path in [
            "",
            "/etc/passwd",
            "../secret",
            "a/../../b",
            ".git/config",
            "a//b",
            "a\\b",
        ] {
            assert!(
                normalize_preview_path(path).is_err(),
                "{path} should be rejected"
            );
        }
    }

    #[test]
    fn image_content_type_by_extension() {
        assert_eq!(image_content_type("docs/shot.PNG"), Some("image/png"));
        assert_eq!(image_content_type("logo.svg"), Some("image/svg+xml"));
        assert_eq!(image_content_type("src/lib.rs"), None);
        assert_eq!(image_content_type("Makefile"), None);
    }

    #[test]
    fn looks_like_text_rejects_binary() {
        assert!(looks_like_text(b"fn main() {}\n"));
        assert!(!looks_like_text(&[0x89, b'P', b'N', b'G', 0, 1]));
        assert!(!looks_like_text(&[0xff, 0xfe, 0xfd]));
    }

    #[test]
    fn path_suffix_matches_whole_segments_only() {
        assert!(is_path_suffix(
            "web/src/components/AppShell.test.tsx",
            "AppShell.test.tsx"
        ));
        assert!(is_path_suffix("src/app.rs", "app.rs"));
        assert!(is_path_suffix("app.rs", "app.rs"));
        assert!(is_path_suffix("web/src/app.rs", "src/app.rs"));
        assert!(!is_path_suffix("src/app.rs", "pp.rs"));
        assert!(!is_path_suffix("src/mapper.rs", "per.rs"));
    }
}
