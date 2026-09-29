//! Per-run gateway access: mint a scoped token when a run starts and revoke it
//! when the run ends (explicitly, or via the `Drop` backstop on early exits).

use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use super::token::{NewRunToolScope, TokenService};
use crate::AppState;

/// What a connector needs to reach the Coppice MCP gateway for one run.
#[derive(Clone)]
pub struct McpAccess {
    pub url: String,
    pub token: String,
}

impl std::fmt::Debug for McpAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpAccess")
            .field("url", &self.url)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl McpAccess {
    /// Environment variables handed to the agent CLI process.
    pub fn env(&self) -> [(&'static str, String); 2] {
        [
            ("COPPICE_MCP_URL", self.url.clone()),
            ("COPPICE_MCP_TOKEN", self.token.clone()),
        ]
    }
}

/// A live run token. Call [`RunToolGrant::revoke`] once the connector returns;
/// if the grant is dropped first (early `?` return, panic) a background task
/// revokes it instead.
pub struct RunToolGrant {
    pub access: McpAccess,
    run_id: Uuid,
    pool: PgPool,
    revoked: bool,
}

pub async fn grant_for_run(
    state: &AppState,
    pool: &PgPool,
    scope: NewRunToolScope,
) -> anyhow::Result<RunToolGrant> {
    let ttl = Duration::from_secs(state.config.mcp.token_ttl_secs);
    let token = TokenService::new(pool).mint(&scope, ttl).await?;
    Ok(RunToolGrant {
        access: McpAccess {
            url: state.config.mcp.gateway_url(state.config.server.port),
            token,
        },
        run_id: scope.run_id,
        pool: pool.clone(),
        revoked: false,
    })
}

impl RunToolGrant {
    pub async fn revoke(mut self) {
        self.revoked = true;
        if let Err(err) = TokenService::new(&self.pool).revoke_for_run(self.run_id).await {
            tracing::warn!(run_id = %self.run_id, error = %err, "failed to revoke run tool token");
        }
    }
}

impl Drop for RunToolGrant {
    fn drop(&mut self) {
        if self.revoked {
            return;
        }
        let pool = self.pool.clone();
        let run_id = self.run_id;
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Err(err) = TokenService::new(&pool).revoke_for_run(run_id).await {
                    tracing::warn!(%run_id, error = %err, "failed to revoke run tool token on drop");
                }
            });
        }
    }
}
