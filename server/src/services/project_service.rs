use crate::domain::project::Project;
use crate::domain::repo::Repo;
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

    pub async fn list_repos(&self, project_id: Uuid) -> Result<Vec<Repo>, ProjectError> {
        self.get_project(project_id).await?;

        let rows = sqlx::query(
            r#"
            SELECT id, project_id, name, remote_url, default_branch, created_at
            FROM repos
            WHERE project_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(project_id)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.iter().map(row_to_repo).collect())
    }

    pub async fn create_repo(
        &self,
        project_id: Uuid,
        name: &str,
        remote_url: Option<&str>,
        default_branch: &str,
    ) -> Result<Repo, ProjectError> {
        self.get_project(project_id).await?;

        let id = Uuid::new_v4();
        let row = sqlx::query(
            r#"
            INSERT INTO repos (id, project_id, name, remote_url, default_branch)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, project_id, name, remote_url, default_branch, created_at
            "#,
        )
        .bind(id)
        .bind(project_id)
        .bind(name)
        .bind(remote_url)
        .bind(default_branch)
        .fetch_one(self.pool)
        .await?;

        Ok(row_to_repo(&row))
    }

    pub async fn get_repo(&self, repo_id: Uuid) -> Result<Repo, ProjectError> {
        let row = sqlx::query(
            r#"
            SELECT id, project_id, name, remote_url, default_branch, created_at
            FROM repos
            WHERE id = $1
            "#,
        )
        .bind(repo_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(ProjectError::RepoNotFound)?;

        Ok(row_to_repo(&row))
    }

    pub async fn update_repo(
        &self,
        repo_id: Uuid,
        name: Option<&str>,
        remote_url: Option<Option<&str>>,
        default_branch: Option<&str>,
    ) -> Result<Repo, ProjectError> {
        let current = self.get_repo(repo_id).await?;
        let name = name.unwrap_or(&current.name);
        let remote_url = match remote_url {
            Some(value) => value.map(str::to_string),
            None => current.remote_url.clone(),
        };
        let default_branch = default_branch.unwrap_or(&current.default_branch);

        let row = sqlx::query(
            r#"
            UPDATE repos
            SET name = $2, remote_url = $3, default_branch = $4
            WHERE id = $1
            RETURNING id, project_id, name, remote_url, default_branch, created_at
            "#,
        )
        .bind(repo_id)
        .bind(name)
        .bind(&remote_url)
        .bind(default_branch)
        .fetch_optional(self.pool)
        .await?
        .ok_or(ProjectError::RepoNotFound)?;

        Ok(row_to_repo(&row))
    }

    pub async fn delete_repo(&self, repo_id: Uuid) -> Result<(), ProjectError> {
        let result = sqlx::query("DELETE FROM repos WHERE id = $1")
            .bind(repo_id)
            .execute(self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(ProjectError::RepoNotFound);
        }

        Ok(())
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

fn row_to_repo(row: &sqlx::postgres::PgRow) -> Repo {
    Repo {
        id: row.get("id"),
        project_id: row.get("project_id"),
        name: row.get("name"),
        remote_url: row.get("remote_url"),
        default_branch: row.get("default_branch"),
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
