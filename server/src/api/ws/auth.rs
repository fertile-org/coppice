use crate::api::auth::AuthUser;
use crate::middleware::session::parse_session_cookie;
use crate::services::auth_service::AuthService;
use crate::AppState;

pub async fn auth_user_from_cookie(state: &AppState, cookies: &str) -> Result<AuthUser, ()> {
    let token = parse_session_cookie(cookies).ok_or(())?;
    let pool = state.db.as_ref().ok_or(())?;
    let auth = AuthService::new(pool, &state.config.auth);
    let (user, session) = auth.user_by_session_token(&token).await.map_err(|_| ())?;
    Ok(AuthUser { user, session })
}
