use crate::domain::board::Board;
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

pub struct BoardService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum BoardError {
    #[error("board not found")]
    BoardNotFound,
    #[error("repo not found")]
    RepoNotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl<'a> BoardService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_boards(&self) -> Result<Vec<Board>, BoardError> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, slug, created_at
            FROM boards
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_board).collect())
    }

    pub async fn create_board(&self, name: &str) -> Result<Board, BoardError> {
        let id = Uuid::new_v4();
        let slug = slugify(name);
        let row = sqlx::query(
            r#"
            INSERT INTO boards (id, name, slug)
            VALUES ($1, $2, $3)
            RETURNING id, name, slug, created_at
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(&slug)
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_board(&row))
    }

    pub async fn get_board(&self, board_id: Uuid) -> Result<Board, BoardError> {
        let row = sqlx::query(
            r#"
            SELECT id, name, slug, created_at
            FROM boards
            WHERE id = $1
            "#,
        )
        .bind(board_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(BoardError::BoardNotFound)?;

        Ok(row_to_board(&row))
    }

    pub async fn update_board(
        &self,
        board_id: Uuid,
        name: Option<&str>,
    ) -> Result<Board, BoardError> {
        let current = self.get_board(board_id).await?;
        let name = name.unwrap_or(&current.name);
        let slug = slugify(name);

        let row = sqlx::query(
            r#"
            UPDATE boards
            SET name = $2, slug = $3
            WHERE id = $1
            RETURNING id, name, slug, created_at
            "#,
        )
        .bind(board_id)
        .bind(name)
        .bind(&slug)
        .fetch_optional(self.pool)
        .await?
        .ok_or(BoardError::BoardNotFound)?;

        Ok(row_to_board(&row))
    }
}

fn row_to_board(row: &sqlx::postgres::PgRow) -> Board {
    Board {
        id: row.get("id"),
        name: row.get("name"),
        slug: row.get("slug"),
        created_at: row.get("created_at"),
    }
}

pub fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() {
            c
        } else if c.is_whitespace() {
            '-'
        } else {
            '\0'
        })
        .filter(|c| *c != '\0')
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_normalizes_name() {
        assert_eq!(slugify("Coppice Demo"), "coppice-demo");
        assert_eq!(slugify("Hello World!"), "hello-world");
    }
}
