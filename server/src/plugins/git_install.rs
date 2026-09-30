use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::Instant;

const NETWORK_SCHEMES: [&str; 4] = ["https://", "http://", "ssh://", "git://"];

/// Accepts `https`/`http`/`ssh`/`git` URLs and scp-like `user@host:path`;
/// `file://` only when `allow_file`. Anything else (options, `ext::`
/// transports, bare local paths) is rejected.
pub fn validate_git_url(url: &str, allow_file: bool) -> Result<(), String> {
    if url.is_empty() {
        return Err("git URL is required".into());
    }
    if url.starts_with('-') {
        return Err("git URL must not start with '-'".into());
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("git URL must not contain whitespace or control characters".into());
    }
    if url.contains("::") {
        return Err("git transport helpers (`<transport>::`) are not allowed".into());
    }
    if let Some(rest) = url.strip_prefix("file://") {
        if !allow_file {
            return Err("file:// git URLs are not allowed".into());
        }
        return if rest.len() > 1 && rest.starts_with('/') {
            Ok(())
        } else {
            Err("file:// git URL must have an absolute path".into())
        };
    }
    if let Some(scheme) = NETWORK_SCHEMES.iter().find(|s| url.starts_with(*s)) {
        let authority = url[scheme.len()..].split('/').next().unwrap_or_default();
        let host = authority.rsplit('@').next().unwrap_or_default();
        return if host.is_empty() || host.starts_with('-') {
            Err("git URL must include a host".into())
        } else {
            Ok(())
        };
    }
    if url.contains("://") {
        return Err("unsupported git URL scheme".into());
    }
    if is_scp_like(url) {
        Ok(())
    } else {
        Err("git URL must be https://, ssh://, git:// or user@host:path".into())
    }
}

fn is_scp_like(url: &str) -> bool {
    let Some((user_host, path)) = url.split_once(':') else {
        return false;
    };
    let host = user_host.split_once('@').map_or(
        user_host,
        |(user, host)| if user.is_empty() { "" } else { host },
    );
    !host.is_empty()
        && !host.starts_with('-')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !path.is_empty()
}

pub fn validate_ref(git_ref: &str) -> Result<(), String> {
    if git_ref.is_empty() {
        return Err("git ref must not be empty".into());
    }
    if git_ref.starts_with('-') {
        return Err("git ref must not start with '-'".into());
    }
    if git_ref
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == ':' || c == '\\')
    {
        return Err("git ref contains invalid characters".into());
    }
    if git_ref.contains("..") {
        return Err("git ref must not contain '..'".into());
    }
    Ok(())
}

/// The folder name a clone of `url` gets inside a plugin dir.
pub fn repo_dir_name(url: &str) -> Result<String, String> {
    let path = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/').map(|(_, path)| path).unwrap_or(""),
        None => url.split_once(':').map(|(_, path)| path).unwrap_or(""),
    };
    let segment = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let name = segment.strip_suffix(".git").unwrap_or(segment);
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(format!("cannot derive a plugin folder name from {url}"));
    }
    Ok(name.to_string())
}

/// Shallow-clones `url` into `dest` (removed again on any failure) and
/// returns the checked-out commit.
pub async fn clone(
    url: &str,
    git_ref: Option<&str>,
    dest: &Path,
    allow_file: bool,
    timeout: Duration,
) -> Result<String, String> {
    let git = Git::new(allow_file, timeout);
    let existed = dest.exists();
    let result = async {
        let mut args: Vec<&OsStr> =
            vec![OsStr::new("clone"), OsStr::new("--depth"), OsStr::new("1")];
        if let Some(git_ref) = git_ref {
            args.extend([OsStr::new("--branch"), OsStr::new(git_ref)]);
        }
        args.extend([OsStr::new("--"), OsStr::new(url), dest.as_os_str()]);
        git.run(&args).await?;
        git.head(dest).await
    }
    .await;
    if result.is_err() && !existed && dest.exists() {
        if let Err(err) = std::fs::remove_dir_all(dest) {
            tracing::warn!(dest = %dest.display(), error = %err, "failed to remove partial plugin clone");
        }
    }
    result
}

/// Fetches `git_ref` (or the remote HEAD) into the clone at `root`, checks it
/// out detached and returns the new commit.
pub async fn update(
    root: &Path,
    git_ref: Option<&str>,
    allow_file: bool,
    timeout: Duration,
) -> Result<String, String> {
    let git = Git::new(allow_file, timeout);
    let target = git_ref.unwrap_or("HEAD");
    git.run(&[
        OsStr::new("-C"),
        root.as_os_str(),
        OsStr::new("fetch"),
        OsStr::new("--depth"),
        OsStr::new("1"),
        OsStr::new("--"),
        OsStr::new("origin"),
        OsStr::new(target),
    ])
    .await?;
    git.run(&[
        OsStr::new("-C"),
        root.as_os_str(),
        OsStr::new("checkout"),
        OsStr::new("--detach"),
        OsStr::new("FETCH_HEAD"),
    ])
    .await?;
    git.head(root).await
}

/// All commands of one operation share a single deadline.
struct Git {
    allow_file: bool,
    timeout: Duration,
    deadline: Instant,
}

impl Git {
    fn new(allow_file: bool, timeout: Duration) -> Self {
        Self {
            allow_file,
            timeout,
            deadline: Instant::now() + timeout,
        }
    }

    async fn head(&self, repo: &Path) -> Result<String, String> {
        self.run(&[
            OsStr::new("-C"),
            repo.as_os_str(),
            OsStr::new("rev-parse"),
            OsStr::new("HEAD"),
        ])
        .await
    }

    async fn run(&self, args: &[&OsStr]) -> Result<String, String> {
        let mut cmd = Command::new("git");
        cmd.args(["-c", "protocol.ext.allow=never"]);
        if !self.allow_file {
            cmd.args(["-c", "protocol.file.allow=never"]);
        }
        cmd.args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let output = tokio::time::timeout_at(self.deadline, cmd.output())
            .await
            .map_err(|_| format!("git timed out after {}s", self.timeout.as_secs()))?
            .map_err(|err| format!("failed to run git: {err}"))?;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_git_url_accepts_https_ssh_scp() {
        for url in [
            "https://github.com/a/b.git",
            "ssh://git@host/a/b",
            "git@github.com:a/b.git",
        ] {
            assert_eq!(validate_git_url(url, false), Ok(()), "{url}");
        }
    }

    #[test]
    fn validate_git_url_rejects_injection() {
        for url in [
            "--upload-pack=touch x",
            "-oProxyCommand=x",
            "ext::sh -c x",
            "file:///tmp/x",
            "",
        ] {
            assert!(validate_git_url(url, false).is_err(), "{url}");
        }
        assert_eq!(validate_git_url("file:///tmp/x", true), Ok(()));
    }

    #[test]
    fn validate_ref_rejects_option_like() {
        for git_ref in ["--output=x", "-b", "a b", "a..b", ""] {
            assert!(validate_ref(git_ref).is_err(), "{git_ref}");
        }
        for git_ref in ["main", "v1.2.0"] {
            assert_eq!(validate_ref(git_ref), Ok(()), "{git_ref}");
        }
    }

    #[test]
    fn repo_dir_name_strips_git_suffix() {
        assert_eq!(
            repo_dir_name("https://github.com/obra/superpowers.git").as_deref(),
            Ok("superpowers")
        );
        assert_eq!(repo_dir_name("git@h:a/b").as_deref(), Ok("b"));
        assert!(repo_dir_name("https://h/").is_err());
    }

    #[tokio::test]
    async fn clone_blocks_file_protocol_unless_allowed() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("README.md"), "x\n").unwrap();
        for args in [
            &["init", "-q"][..],
            &["add", "-A"],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@localhost",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                "x",
            ],
        ] {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&src)
                .status()
                .unwrap();
            assert!(status.success());
        }
        let url = format!("file://{}", src.display());
        let timeout = Duration::from_secs(30);

        let blocked = tmp.path().join("blocked");
        let err = clone(&url, None, &blocked, false, timeout)
            .await
            .unwrap_err();
        assert!(!err.is_empty());
        assert!(!blocked.exists());

        let allowed = tmp.path().join("allowed");
        let commit = clone(&url, None, &allowed, true, timeout).await.unwrap();
        assert_eq!(commit.len(), 40);
        assert!(allowed.join("README.md").is_file());
    }
}
