use crate::api::auth::AuthUser;
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};

pub struct AdminUser(pub AuthUser);

impl<S> FromRequestParts<S> for AdminUser
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state).await?;
        if auth.user.role != "admin" {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(AdminUser(auth))
    }
}
