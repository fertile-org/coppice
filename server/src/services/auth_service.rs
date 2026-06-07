use crate::config::AuthConfig;
use crate::domain::session::{Session, SessionBundle};
use crate::domain::user::User;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use sqlx::Row;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub struct AuthService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("bootstrap not allowed")]
    BootstrapNotAllowed,
    #[error("session not found")]
    SessionNotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("password hash error")]
    PasswordHash,
}

impl<'a> AuthService<'a> {
    pub fn new(pool: &'a PgPool, _auth: &'a AuthConfig) -> Self {
        Self { pool }
    }

    pub async fn bootstrap_admin(&self, email: &str, password: &str) -> Result<User, AuthError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(self.pool)
            .await?;

        if count > 0 {
            return Err(AuthError::BootstrapNotAllowed);
        }

        let id = Uuid::new_v4();
        let password_hash = hash_password(password).map_err(|_| AuthError::PasswordHash)?;
        let row = sqlx::query(
            r#"
            INSERT INTO users (id, email, password_hash, role)
            VALUES ($1, $2, $3, 'admin')
            RETURNING id, email, role, created_at
            "#,
        )
        .bind(id)
        .bind(email)
        .bind(&password_hash)
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_user(&row))
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<SessionBundle, AuthError> {
        let row = sqlx::query(
            r#"
            SELECT id, email, password_hash, role, created_at
            FROM users
            WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(self.pool)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

        let password_hash: String = row.get("password_hash");
        if !verify_password(password, &password_hash).map_err(|_| AuthError::PasswordHash)? {
            return Err(AuthError::InvalidCredentials);
        }

        let user = row_to_user(&row);
        self.create_session(user).await
    }

    pub async fn logout(&self, session_id: Uuid) -> Result<(), AuthError> {
        sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(session_id)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn user_by_session_token(
        &self,
        session_token: &str,
    ) -> Result<(User, Session), AuthError> {
        let token_hash = hash_token(session_token);
        let row = sqlx::query(
            r#"
            SELECT
                s.id AS session_id,
                s.user_id,
                s.csrf_token,
                s.expires_at AS session_expires_at,
                u.id,
                u.email,
                u.role,
                u.created_at
            FROM sessions s
            INNER JOIN users u ON u.id = s.user_id
            WHERE s.token_hash = $1
            "#,
        )
        .bind(&token_hash)
        .fetch_optional(self.pool)
        .await?
        .ok_or(AuthError::SessionNotFound)?;

        let session_expires_at: OffsetDateTime = row.get("session_expires_at");
        if session_expires_at < OffsetDateTime::now_utc() {
            let session_id: Uuid = row.get("session_id");
            sqlx::query("DELETE FROM sessions WHERE id = $1")
                .bind(session_id)
                .execute(self.pool)
                .await?;
            return Err(AuthError::SessionNotFound);
        }

        let user = User {
            id: row.get("id"),
            email: row.get("email"),
            role: row.get("role"),
            created_at: row.get("created_at"),
        };
        let session = Session {
            id: row.get("session_id"),
            user_id: row.get("user_id"),
            csrf_token: row.get("csrf_token"),
            expires_at: session_expires_at,
        };

        Ok((user, session))
    }

    async fn create_session(&self, user: User) -> Result<SessionBundle, AuthError> {
        let session_id = Uuid::new_v4();
        let session_token = generate_token();
        let csrf_token = generate_token();
        let token_hash = hash_token(&session_token);
        let expires_at = OffsetDateTime::now_utc() + Duration::days(7);

        let row = sqlx::query(
            r#"
            INSERT INTO sessions (id, user_id, token_hash, csrf_token, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, user_id, csrf_token, expires_at
            "#,
        )
        .bind(session_id)
        .bind(user.id)
        .bind(&token_hash)
        .bind(&csrf_token)
        .bind(expires_at)
        .fetch_one(self.pool)
        .await?;

        Ok(SessionBundle {
            session: Session {
                id: row.get("id"),
                user_id: row.get("user_id"),
                csrf_token: row.get("csrf_token"),
                expires_at: row.get("expires_at"),
            },
            session_token,
            user,
        })
    }
}

fn row_to_user(row: &sqlx::postgres::PgRow) -> User {
    User {
        id: row.get("id"),
        email: row.get("email"),
        role: row.get("role"),
        created_at: row.get("created_at"),
    }
}

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_password() {
        let hash = hash_password("secret").unwrap();
        assert!(verify_password("secret", &hash).unwrap());
        assert!(!verify_password("wrong", &hash).unwrap());
    }
}
