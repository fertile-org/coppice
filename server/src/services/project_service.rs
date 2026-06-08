use crate::domain::project::Project;
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

pub struct ProjectService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("project not found")]
    ProjectNotFound,
    #[error("repo not found")]
    RepoNotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl<'a> ProjectService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>, ProjectError> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, slug, created_at
            FROM projects
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_project).collect())
    }

    pub async fn create_project(&self, name: &str) -> Result<Project, ProjectError> {
        let id = Uuid::new_v4();
        let slug = slugify(name);
        let row = sqlx::query(
            r#"
            INSERT INTO projects (id, name, slug)
            VALUES ($1, $2, $3)
            RETURNING id, name, slug, created_at
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(&slug)
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_project(&row))
    }

    pub async fn get_project(&self, project_id: Uuid) -> Result<Project, ProjectError> {
        let row = sqlx::query(
            r#"
            SELECT id, name, slug, created_at
            FROM projects
            WHERE id = $1
            "#,
        )
        .bind(project_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(ProjectError::ProjectNotFound)?;

        Ok(row_to_project(&row))
    }

    pub async fn update_project(
        &self,
        project_id: Uuid,
        name: Option<&str>,
    ) -> Result<Project, ProjectError> {
        let current = self.get_project(project_id).await?;
        let name = name.unwrap_or(&current.name);
        let slug = slugify(name);

        let row = sqlx::query(
            r#"
            UPDATE projects
            SET name = $2, slug = $3
            WHERE id = $1
            RETURNING id, name, slug, created_at
            "#,
        )
        .bind(project_id)
        .bind(name)
        .bind(&slug)
        .fetch_optional(self.pool)
        .await?
        .ok_or(ProjectError::ProjectNotFound)?;

        Ok(row_to_project(&row))
    }
}

fn row_to_project(row: &sqlx::postgres::PgRow) -> Project {
    Project {
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
