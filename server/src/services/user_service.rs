use crate::domain::user::User;
use crate::services::auth_service::hash_password;
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

pub struct UserService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("email already taken")]
    EmailTaken,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("password hash error")]
    PasswordHash,
}

impl<'a> UserService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_users(&self) -> Result<Vec<User>, UserError> {
        let rows = sqlx::query(
            r#"
            SELECT id, email, role, created_at
            FROM users
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_user).collect())
    }

    pub async fn create_member(&self, email: &str, password: &str) -> Result<User, UserError> {
        let id = Uuid::new_v4();
        let password_hash = hash_password(password).map_err(|_| UserError::PasswordHash)?;

        let row = sqlx::query(
            r#"
            INSERT INTO users (id, email, password_hash, role)
            VALUES ($1, $2, $3, 'member')
            RETURNING id, email, role, created_at
            "#,
        )
        .bind(id)
        .bind(email)
        .bind(&password_hash)
        .fetch_one(self.pool)
        .await
        .map_err(|err| {
            if let sqlx::Error::Database(db_err) = &err {
                if db_err.constraint() == Some("users_email_key") {
                    return UserError::EmailTaken;
                }
            }
            UserError::Database(err)
        })?;

        Ok(row_to_user(&row))
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
