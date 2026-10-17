//! Refresh models must rerun provider discovery. A normal load keeps the cache.
mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use axum::http::StatusCode;
use tower::ServiceExt;

fn install_fake_opencode(dir: &Path) {
    let script = dir.join("opencode");
    std::fs::write(
        &script,
        "#!/bin/sh\n\
         dir=$(CDPATH= cd -- \"$(dirname \"$0\")\" && pwd)\n\
         printf '%s\\n' \"$*\" >> \"$dir/calls\"\n\
         if [ \"$1\" = models ] && [ -n \"$2\" ]; then\n\
           grep \"^$2/\" \"$dir/models.txt\" || true\n\
           exit 0\n\
         fi\n\
         cat \"$dir/models.txt\"\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    std::fs::write(dir.join("models.txt"), "anthropic/claude-sonnet\n").unwrap();
}

fn call_count(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("calls"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty())
        .count()
}

fn provider_ids(body: &serde_json::Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect()
}

async fn get_providers(app: &axum::Router, cookie: &str, csrf: &str, refresh: bool) -> Vec<String> {
    let path = if refresh {
        "/api/connectors/opencode/model-providers?refresh=true"
    } else {
        "/api/connectors/opencode/model-providers"
    };
    let res = app
        .clone()
        .oneshot(common::json_request("GET", path, "", cookie, csrf))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "{path}");
    provider_ids(&common::json_body(res).await)
}

#[tokio::test]
async fn refresh_request_sees_a_provider_added_after_the_first_load() {
    let _guard = common::DB_TEST_LOCK.lock().await;
    if !common::db_available().await {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    install_fake_opencode(dir.path());
    let command = dir.path().join("opencode");
    let (_state, app, cookie, csrf) = common::bootstrap_and_login_with_state_config(|config| {
        config.agent.connectors.opencode.enabled = Some(true);
        config.agent.connectors.opencode.command = command.display().to_string();
        config.agent.connectors.opencode.model_providers.clear();
    })
    .await;

    let first = get_providers(&app, &cookie, &csrf, false).await;
    assert_eq!(first, vec!["anthropic".to_string()]);
    assert_eq!(call_count(dir.path()), 1);

    std::fs::write(
        dir.path().join("models.txt"),
        "anthropic/claude-sonnet\nopenai/gpt-5\n",
    )
    .unwrap();

    let cached = get_providers(&app, &cookie, &csrf, false).await;
    assert_eq!(cached, vec!["anthropic".to_string()]);
    assert_eq!(
        call_count(dir.path()),
        1,
        "a normal load must keep the discovery cache"
    );

    let refreshed = get_providers(&app, &cookie, &csrf, true).await;
    assert_eq!(
        refreshed,
        vec!["anthropic".to_string(), "openai".to_string()]
    );
    assert_eq!(call_count(dir.path()), 2);

    let after = get_providers(&app, &cookie, &csrf, false).await;
    assert_eq!(after, vec!["anthropic".to_string(), "openai".to_string()]);
    assert_eq!(
        call_count(dir.path()),
        2,
        "the refreshed list is what later loads reuse"
    );
}
