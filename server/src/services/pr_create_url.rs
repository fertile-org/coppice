//! Build hosted-git "create pull/merge request" URLs from a repository remote.

/// Returns a browser URL that opens the host's PR/MR creation page, or `None` if
/// `remote_url` is missing or not a recognized host.
pub fn build_pr_create_url(
    remote_url: Option<&str>,
    base_branch: &str,
    head_branch: &str,
) -> Option<String> {
    let remote = remote_url?.trim();
    if remote.is_empty() {
        return None;
    }
    let (host, repo_path) = parse_git_remote(remote)?;
    let web_base = format!("https://{host}/{repo_path}");
    let base = percent_encode(base_branch);
    let head = percent_encode(head_branch);

    if is_github_host(&host) {
        return Some(format!("{web_base}/compare/{base}...{head}?expand=1"));
    }
    if is_gitlab_host(&host) {
        return Some(format!(
            "{web_base}/-/merge_requests/new?merge_request%5Bsource_branch%5D={head}&merge_request%5Btarget_branch%5D={base}"
        ));
    }
    if host == "bitbucket.org" {
        return Some(format!(
            "{web_base}/pull-requests/new?source={head}&dest={base}"
        ));
    }

    None
}

/// HTTPS remote suitable for token auth: `https://github.com/owner/repo.git`
pub fn https_remote_url(remote_url: &str) -> Option<String> {
    let (host, repo_path) = parse_git_remote(remote_url.trim())?;
    Some(format!("https://{host}/{repo_path}.git"))
}

/// GitHub `owner` / `repo` for the REST API (exactly one slash in path).
pub fn github_owner_repo(remote_url: &str) -> Option<(String, String)> {
    let (host, repo_path) = parse_git_remote(remote_url.trim())?;
    if !is_github_host(&host) {
        return None;
    }
    let (owner, repo) = repo_path.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

pub fn is_github_host(host: &str) -> bool {
    host == "github.com" || host.ends_with(".github.com") || host.contains("github.")
}

fn is_gitlab_host(host: &str) -> bool {
    host == "gitlab.com" || host.ends_with(".gitlab.com") || host.contains("gitlab")
}

pub fn parse_git_remote(remote: &str) -> Option<(String, String)> {
    if let Some(scp) = remote.strip_prefix("git@") {
        let (host, path) = scp.split_once(':')?;
        let path = path.trim_end_matches(".git").trim();
        if host.is_empty() || path.is_empty() {
            return None;
        }
        return Some((host.to_string(), path.to_string()));
    }

    if remote.starts_with("ssh://")
        || remote.starts_with("https://")
        || remote.starts_with("http://")
    {
        let without_scheme = remote
            .trim_start_matches("ssh://")
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let path_start = without_scheme.find('/')?;
        let authority = &without_scheme[..path_start];
        let path = without_scheme[path_start + 1..]
            .trim_end_matches(".git")
            .trim();
        let host = authority
            .strip_prefix("git@")
            .unwrap_or(authority)
            .split('@')
            .next_back()?;
        if host.is_empty() || path.is_empty() {
            return None;
        }
        return Some((host.to_string(), path.to_string()));
    }

    None
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_https_compare_url() {
        let url = build_pr_create_url(
            Some("https://github.com/acme/coppice.git"),
            "main",
            "agent/TICKET-abc",
        )
        .expect("url");
        assert_eq!(
            url,
            "https://github.com/acme/coppice/compare/main...agent%2FTICKET-abc?expand=1"
        );
    }

    #[test]
    fn github_ssh_compare_url() {
        let url = build_pr_create_url(
            Some("git@github.com:acme/coppice.git"),
            "develop",
            "feature/x",
        )
        .expect("url");
        assert!(url.contains("github.com/acme/coppice/compare/develop...feature%2Fx"));
    }

    #[test]
    fn gitlab_merge_request_url() {
        let url = build_pr_create_url(
            Some("https://gitlab.com/group/coppice.git"),
            "main",
            "feat",
        )
        .expect("url");
        assert!(url.contains("gitlab.com/group/coppice/-/merge_requests/new"));
    }

    #[test]
    fn bitbucket_pr_url() {
        let url = build_pr_create_url(
            Some("https://bitbucket.org/acme/repo.git"),
            "main",
            "feat",
        )
        .expect("url");
        assert!(url.contains("bitbucket.org/acme/repo/pull-requests/new"));
    }

    #[test]
    fn missing_or_unknown_remote() {
        assert!(build_pr_create_url(None, "main", "feature").is_none());
        assert!(build_pr_create_url(Some(""), "main", "feature").is_none());
        assert!(build_pr_create_url(Some("git@codeberg.org:acme/repo.git"), "main", "feat").is_none());
    }

    #[test]
    fn github_owner_repo_from_ssh() {
        assert_eq!(
            github_owner_repo("git@github.com:acme/coppice.git"),
            Some(("acme".into(), "coppice".into()))
        );
    }
}
