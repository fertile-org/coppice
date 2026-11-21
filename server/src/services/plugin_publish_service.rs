//! Push a builder-made plugin into an admin git marketplace repo (PB-12).
//! Host credentials only — never TicketGitService / forge secrets.
use crate::domain::Id;

use crate::plugins::capability::MARKETPLACE_JSON;
use crate::plugins::git_install::{self, validate_git_url};
use crate::services::plugin_service::{PluginError, PluginService};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;
pub const INVALID_URL_MSG: &str = "That doesn't look like a repository URL.";

pub fn unreachable_msg(repo: &str) -> String {
    format!(
        "Couldn't push to {repo}. Check that this computer can push there, then try again."
    )
}

pub fn branch_exists_msg(plugin_name: &str) -> String {
    format!(
        "A branch for this plugin is already there. Coppice will push to {}.",
        publish_branch_suffix(plugin_name)
    )
}

pub fn publish_branch(plugin_name: &str) -> String {
    format!("coppice/plugin-{plugin_name}")
}

pub fn publish_branch_suffix(plugin_name: &str) -> String {
    format!("coppice/plugin-{plugin_name}-2")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishResult {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl PublishResult {
    pub fn skipped() -> Self {
        Self {
            status: "skipped",
            branch: None,
            error: None,
            suggested_branch: None,
            url: None,
        }
    }

    pub fn pushed(branch: String, url: String) -> Self {
        Self {
            status: "pushed",
            branch: Some(branch),
            error: None,
            suggested_branch: None,
            url: Some(url),
        }
    }

    pub fn failed(error: String) -> Self {
        Self {
            status: "failed",
            branch: None,
            error: Some(error),
            suggested_branch: None,
            url: None,
        }
    }

    pub fn branch_exists(plugin_name: &str) -> Self {
        Self {
            status: "failed",
            branch: None,
            error: Some(branch_exists_msg(plugin_name)),
            suggested_branch: Some(publish_branch_suffix(plugin_name)),
            url: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginPublishError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unreachable(String),
    #[error("plugin not found")]
    NotFound,
    #[error("plugin was not made by the builder")]
    NotBuilderMade,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct PluginPublishService<'a> {
    pool: &'a PgPool,
    worktrees_path: PathBuf,
    allow_file: bool,
    timeout: Duration,
    push_enabled: bool,
}

impl<'a> PluginPublishService<'a> {
    pub fn new(
        pool: &'a PgPool,
        worktrees_path: PathBuf,
        allow_file: bool,
        timeout: Duration,
        push_enabled: bool,
    ) -> Self {
        Self {
            pool,
            worktrees_path,
            allow_file,
            timeout,
            push_enabled,
        }
    }

    /// Accepted builder row for this plugin, if any.
    pub async fn accepted_build_for_plugin(
        &self,
        plugin_id: Id,
    ) -> Result<Option<(Id, Option<String>, Option<String>)>, PluginPublishError> {
        let row = sqlx::query_as::<_, (Id, Option<String>, Option<String>)>(
            r#"
            SELECT id, plugin_name, publish_target
            FROM plugin_builds
            WHERE plugin_id = $1 AND status = 'accepted'
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )
        .bind(plugin_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    /// Latest accepted publish target per builder-made plugin id.
    pub async fn builder_made_targets(
        &self,
        plugin_ids: &[Id],
    ) -> Result<std::collections::HashMap<Id, Option<String>>, PluginPublishError> {
        if plugin_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let rows: Vec<(Id, Option<String>)> = sqlx::query_as(
            r#"
            SELECT DISTINCT ON (plugin_id) plugin_id, publish_target
            FROM plugin_builds
            WHERE status = 'accepted' AND plugin_id = ANY($1)
            ORDER BY plugin_id, updated_at DESC
            "#,
        )
        .bind(plugin_ids)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// Publish after Accept. Never fails the Accept — maps errors into `PublishResult`.
    pub async fn publish_after_accept(
        &self,
        plugin_id: Id,
        plugin_name: &str,
        url: &str,
        plugins: &PluginService<'_>,
    ) -> PublishResult {
        match self
            .publish_plugin(plugin_id, plugin_name, url, false, plugins)
            .await
        {
            Ok(result) => result,
            Err(PluginPublishError::Validation(msg)) => PublishResult::failed(msg),
            Err(PluginPublishError::Conflict(_)) => PublishResult::branch_exists(plugin_name),
            Err(PluginPublishError::Unreachable(msg)) => PublishResult::failed(msg),
            Err(_) => PublishResult::failed(unreachable_msg(url)),
        }
    }

    /// Dedicated Publish… / retry / suffix-confirm path.
    pub async fn publish_for_plugin(
        &self,
        plugin_id: Id,
        url: &str,
        confirm_suffix: bool,
        plugins: &PluginService<'_>,
    ) -> Result<PublishResult, PluginPublishError> {
        let Some((build_id, plugin_name, _)) = self.accepted_build_for_plugin(plugin_id).await?
        else {
            return Err(PluginPublishError::NotBuilderMade);
        };
        let name = match plugin_name.filter(|s| !s.is_empty()) {
            Some(n) => n,
            None => plugins.get_plugin(plugin_id).await?.name,
        };
        // Keep last target on the build when the admin edits the URL.
        sqlx::query(
            "UPDATE plugin_builds SET publish_target = $2, updated_at = now() WHERE id = $1",
        )
        .bind(build_id)
        .bind(url)
        .execute(self.pool)
        .await?;

        self.publish_plugin(plugin_id, &name, url, confirm_suffix, plugins)
            .await
    }

    async fn publish_plugin(
        &self,
        plugin_id: Id,
        plugin_name: &str,
        url: &str,
        confirm_suffix: bool,
        plugins: &PluginService<'_>,
    ) -> Result<PublishResult, PluginPublishError> {
        if !self.push_enabled {
            return Err(PluginPublishError::Unreachable(
                "git.push_enabled is false in server config".into(),
            ));
        }
        if validate_git_url(url, self.allow_file).is_err() {
            return Err(PluginPublishError::Validation(INVALID_URL_MSG.into()));
        }

        let source = plugins.plugin_path(plugin_id).await?;
        if !source.is_dir() {
            return Err(PluginPublishError::Validation(
                "installed plugin folder is missing".into(),
            ));
        }

        let temp = self.worktrees_path.join(format!("plugin-publish-{plugin_id}"));
        if temp.exists() {
            let _ = tokio::fs::remove_dir_all(&temp).await;
        }
        let cleanup = TempClone { path: temp.clone() };

        git_install::clone(url, None, &temp, self.allow_file, self.timeout)
            .await
            .map_err(|_| PluginPublishError::Unreachable(unreachable_msg(url)))?;

        upsert_marketplace_entry(&temp, plugin_name).map_err(PluginPublishError::Validation)?;

        let dest = temp.join("plugins").join(plugin_name);
        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        copy_plugin_tree(&source, &dest).map_err(PluginPublishError::Io)?;

        let default_branch = remote_default_branch(&temp, self.allow_file, self.timeout)
            .await
            .map_err(|_| PluginPublishError::Unreachable(unreachable_msg(url)))?;

        let primary = publish_branch(plugin_name);
        let branch = if confirm_suffix {
            publish_branch_suffix(plugin_name)
        } else {
            primary.clone()
        };

        if remote_branch_exists(&temp, &branch, self.allow_file, self.timeout)
            .await
            .map_err(|_| PluginPublishError::Unreachable(unreachable_msg(url)))?
        {
            if !confirm_suffix && branch == primary {
                return Err(PluginPublishError::Conflict(branch_exists_msg(plugin_name)));
            }
            return Err(PluginPublishError::Unreachable(unreachable_msg(url)));
        }

        if branch == default_branch {
            return Err(PluginPublishError::Validation(format!(
                "refusing to push to the default branch ({default_branch})"
            )));
        }

        run_git(
            &temp,
            &[
                OsStr::new("checkout"),
                OsStr::new("-b"),
                OsStr::new(&branch),
            ],
            self.allow_file,
            self.timeout,
        )
        .await
        .map_err(|_| PluginPublishError::Unreachable(unreachable_msg(url)))?;

        run_git(
            &temp,
            &[OsStr::new("add"), OsStr::new("-A")],
            self.allow_file,
            self.timeout,
        )
        .await
        .map_err(|_| PluginPublishError::Unreachable(unreachable_msg(url)))?;

        // Host user.name / user.email — do not override author/committer.
        let commit_msg = format!("Add plugin {plugin_name}");
        match run_git(
            &temp,
            &[
                OsStr::new("commit"),
                OsStr::new("-m"),
                OsStr::new(&commit_msg),
            ],
            self.allow_file,
            self.timeout,
        )
        .await
        {
            Ok(()) => {}
            Err(err) if err.contains("nothing to commit") => {
                // Still push the branch if the tree already matched.
            }
            Err(_) => {
                return Err(PluginPublishError::Unreachable(unreachable_msg(url)));
            }
        }

        let refspec = format!("refs/heads/{branch}");
        run_git(
            &temp,
            &[
                OsStr::new("push"),
                OsStr::new("-u"),
                OsStr::new("origin"),
                OsStr::new(&refspec),
            ],
            self.allow_file,
            self.timeout,
        )
        .await
        .map_err(|_| PluginPublishError::Unreachable(unreachable_msg(url)))?;

        drop(cleanup);
        Ok(PublishResult::pushed(branch, url.to_string()))
    }
}

struct TempClone {
    path: PathBuf,
}

impl Drop for TempClone {
    fn drop(&mut self) {
        if self.path.exists() {
            if let Err(err) = std::fs::remove_dir_all(&self.path) {
                tracing::warn!(
                    path = %self.path.display(),
                    error = %err,
                    "failed to remove plugin publish clone"
                );
            }
        }
    }
}

/// Ensure `.claude-plugin/marketplace.json` and upsert `{ name, source }`.
pub fn upsert_marketplace_entry(repo_root: &Path, plugin_name: &str) -> Result<(), String> {
    let path = repo_root.join(MARKETPLACE_JSON);
    let mut value = if path.is_file() {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())?
    } else {
        let name = repo_root
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("marketplace");
        json!({ "name": name, "plugins": [] })
    };

    let plugins = match value.get_mut("plugins") {
        Some(Value::Array(arr)) => arr,
        Some(_) => return Err("`plugins` must be an array".into()),
        None => {
            value
                .as_object_mut()
                .ok_or_else(|| "marketplace.json must be an object".to_string())?
                .insert("plugins".into(), json!([]));
            value.get_mut("plugins").and_then(Value::as_array_mut).unwrap()
        }
    };

    let source = format!("./plugins/{plugin_name}");
    let entry = json!({ "name": plugin_name, "source": source });
    if let Some(existing) = plugins
        .iter_mut()
        .find(|p| p.get("name").and_then(Value::as_str) == Some(plugin_name))
    {
        *existing = entry;
    } else {
        plugins.push(entry);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let pretty = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("{pretty}\n")).map_err(|e| e.to_string())?;
    Ok(())
}

fn copy_plugin_tree(src: &Path, dest: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dest)?;
    for entry in walkdir::WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != ".git" && name != ".agent"
        })
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(src) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(rel);
        if entry.file_type().is_symlink() {
            continue;
        }
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(path, &target)?;
        }
    }
    Ok(())
}

async fn run_git(
    repo: &Path,
    args: &[&OsStr],
    allow_file: bool,
    timeout: Duration,
) -> Result<(), String> {
    let mut full: Vec<&OsStr> = vec![
        OsStr::new("-c"),
        OsStr::new("protocol.ext.allow=never"),
        OsStr::new("-C"),
        repo.as_os_str(),
    ];
    if !allow_file {
        full.push(OsStr::new("-c"));
        full.push(OsStr::new("protocol.file.allow=never"));
    }
    full.extend_from_slice(args);
    let output = crate::services::git_cmd::output_within(None, &full, timeout)
        .await
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::TimedOut {
                format!("git timed out after {}s", timeout.as_secs())
            } else {
                err.to_string()
            }
        })?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("git exited with {}", output.status)
        } else {
            stderr
        })
    }
}

async fn run_git_stdout(
    repo: &Path,
    args: &[&OsStr],
    allow_file: bool,
    timeout: Duration,
) -> Result<String, String> {
    let mut full: Vec<&OsStr> = vec![
        OsStr::new("-c"),
        OsStr::new("protocol.ext.allow=never"),
        OsStr::new("-C"),
        repo.as_os_str(),
    ];
    if !allow_file {
        full.push(OsStr::new("-c"));
        full.push(OsStr::new("protocol.file.allow=never"));
    }
    full.extend_from_slice(args);
    let output = crate::services::git_cmd::output_within(None, &full, timeout)
        .await
        .map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("git exited with {}", output.status)
        } else {
            stderr
        })
    }
}

async fn remote_default_branch(
    repo: &Path,
    allow_file: bool,
    timeout: Duration,
) -> Result<String, String> {
    // After shallow clone, origin/HEAD usually points at the default branch.
    let sym = run_git_stdout(
        repo,
        &[
            OsStr::new("symbolic-ref"),
            OsStr::new("--short"),
            OsStr::new("refs/remotes/origin/HEAD"),
        ],
        allow_file,
        timeout,
    )
    .await;
    if let Ok(s) = sym {
        if let Some(name) = s.strip_prefix("origin/") {
            return Ok(name.to_string());
        }
        if !s.is_empty() {
            return Ok(s);
        }
    }
    // Fallback: current HEAD name (detached clones still have a branch from --depth 1).
    let head = run_git_stdout(
        repo,
        &[
            OsStr::new("rev-parse"),
            OsStr::new("--abbrev-ref"),
            OsStr::new("HEAD"),
        ],
        allow_file,
        timeout,
    )
    .await?;
    if head == "HEAD" {
        Ok("main".into())
    } else {
        Ok(head)
    }
}

async fn remote_branch_exists(
    repo: &Path,
    branch: &str,
    allow_file: bool,
    timeout: Duration,
) -> Result<bool, String> {
    let out = run_git_stdout(
        repo,
        &[
            OsStr::new("ls-remote"),
            OsStr::new("--heads"),
            OsStr::new("origin"),
            OsStr::new(branch),
        ],
        allow_file,
        timeout,
    )
    .await?;
    Ok(!out.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn branch_names() {
        assert_eq!(publish_branch("hello"), "coppice/plugin-hello");
        assert_eq!(publish_branch_suffix("hello"), "coppice/plugin-hello-2");
    }

    #[test]
    fn upsert_creates_and_replaces() {
        let dir = tempdir().unwrap();
        upsert_marketplace_entry(dir.path(), "alpha").unwrap();
        let text = std::fs::read_to_string(dir.path().join(MARKETPLACE_JSON)).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["plugins"][0]["name"], "alpha");
        assert_eq!(v["plugins"][0]["source"], "./plugins/alpha");

        upsert_marketplace_entry(dir.path(), "beta").unwrap();
        upsert_marketplace_entry(dir.path(), "alpha").unwrap();
        let text = std::fs::read_to_string(dir.path().join(MARKETPLACE_JSON)).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        let plugins = v["plugins"].as_array().unwrap();
        assert_eq!(plugins.len(), 2);
        let alpha = plugins
            .iter()
            .find(|p| p["name"] == "alpha")
            .unwrap();
        assert_eq!(alpha["source"], "./plugins/alpha");
    }

    #[test]
    fn never_equals_default_name_shape() {
        // Publish branches are namespaced away from typical defaults.
        for name in ["main", "master", "develop"] {
            assert_ne!(publish_branch(name), name);
            assert_ne!(publish_branch_suffix(name), name);
        }
    }

    #[test]
    fn invalid_url_constant_matches_spec() {
        assert_eq!(INVALID_URL_MSG, "That doesn't look like a repository URL.");
        assert!(unreachable_msg("https://example.test/r.git").contains("Couldn't push to"));
    }
}
