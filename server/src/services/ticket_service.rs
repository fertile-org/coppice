use crate::domain::substatus::{
    build_substatus_display, validate_status_substatus_combo, Substatus, SubstatusDisplay,
    TicketStatus,
};
use crate::domain::ticket::{
    priority_from_str, priority_to_str, status_from_str, status_to_str, substatus_from_str,
    substatus_to_str, Ticket, TicketPriority,
};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

pub struct TicketService<'a> {
    pool: &'a PgPool,
}

#[derive(Debug, Default)]
pub struct TicketFilters {
    pub status: Option<TicketStatus>,
    pub assignee_agent_id: Option<Uuid>,
}

#[derive(Debug, thiserror::Error)]
pub enum TicketError {
    #[error("ticket not found")]
    TicketNotFound,
    #[error("project not found")]
    ProjectNotFound,
    #[error("invalid status")]
    InvalidStatus,
    #[error("invalid substatus")]
    InvalidSubstatus,
    #[error("invalid priority")]
    InvalidPriority,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub struct TicketWithDisplay {
    pub ticket: Ticket,
    pub substatus_display: Option<SubstatusDisplay>,
    pub last_activity_at: OffsetDateTime,
}

impl<'a> TicketService<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list_by_project(
        &self,
        project_id: Uuid,
        filters: &TicketFilters,
    ) -> Result<Vec<TicketWithDisplay>, TicketError> {
        self.ensure_project_exists(project_id).await?;

        let mut query = String::from(
            r#"
            SELECT
                t.id, t.project_id, t.repo_id, t.title, t.description,
                t.status, t.substatus, t.substatus_metadata, t.priority,
                t.assignee_agent_id, t.owner_user_id, t.branch_name,
                t.created_by, t.created_by_id, t.created_at, t.updated_at
            FROM tickets t
            WHERE t.project_id = $1
            "#,
        );
        let mut bind_index = 2;

        if filters.status.is_some() {
            query.push_str(&format!(" AND t.status = ${bind_index}"));
            bind_index += 1;
        }
        if filters.assignee_agent_id.is_some() {
            query.push_str(&format!(" AND t.assignee_agent_id = ${bind_index}"));
        }
        query.push_str(" ORDER BY t.created_at ASC");

        let mut q = sqlx::query(&query).bind(project_id);
        if let Some(status) = filters.status {
            q = q.bind(status_to_str(status));
        }
        if let Some(agent_id) = filters.assignee_agent_id {
            q = q.bind(agent_id);
        }

        let rows = q.fetch_all(self.pool).await?;
        let mut results = Vec::with_capacity(rows.len());
        for row in &rows {
            results.push(self.enrich_ticket(row_to_ticket(row)).await?);
        }
        Ok(results)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        project_id: Uuid,
        title: &str,
        description: &str,
        repo_id: Option<Uuid>,
        priority: Option<TicketPriority>,
        created_by: &str,
        created_by_id: Uuid,
    ) -> Result<TicketWithDisplay, TicketError> {
        self.ensure_project_exists(project_id).await?;

        let id = Uuid::new_v4();
        let status = TicketStatus::Backlog;
        let priority_str = priority.map(priority_to_str);

        let row = sqlx::query(
            r#"
            INSERT INTO tickets (
                id, project_id, repo_id, title, description, status, priority,
                created_by, created_by_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING
                id, project_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                created_by, created_by_id, created_at, updated_at
            "#,
        )
        .bind(id)
        .bind(project_id)
        .bind(repo_id)
        .bind(title)
        .bind(description)
        .bind(status_to_str(status))
        .bind(priority_str)
        .bind(created_by)
        .bind(created_by_id)
        .fetch_one(self.pool)
        .await?;

        self.enrich_ticket(row_to_ticket(&row)).await
    }

    pub async fn get(&self, ticket_id: Uuid) -> Result<TicketWithDisplay, TicketError> {
        let row = sqlx::query(
            r#"
            SELECT
                id, project_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                created_by, created_by_id, created_at, updated_at
            FROM tickets
            WHERE id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        self.enrich_ticket(row_to_ticket(&row)).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_fields(
        &self,
        ticket_id: Uuid,
        title: Option<&str>,
        description: Option<&str>,
        repo_id: Option<Option<Uuid>>,
        priority: Option<Option<TicketPriority>>,
        branch_name: Option<Option<&str>>,
        owner_user_id: Option<Option<Uuid>>,
    ) -> Result<TicketWithDisplay, TicketError> {
        let current = self.get(ticket_id).await?;
        let title = title.unwrap_or(&current.ticket.title);
        let description = description.unwrap_or(&current.ticket.description);
        let repo_id = match repo_id {
            Some(value) => value,
            None => current.ticket.repo_id,
        };
        let priority = match priority {
            Some(value) => value,
            None => current.ticket.priority,
        };
        let branch_name = match branch_name {
            Some(value) => value.map(str::to_string),
            None => current.ticket.branch_name.clone(),
        };
        let owner_user_id = match owner_user_id {
            Some(value) => value,
            None => current.ticket.owner_user_id,
        };
        let priority_str = priority.map(priority_to_str);

        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET
                title = $2,
                description = $3,
                repo_id = $4,
                priority = $5,
                branch_name = $6,
                owner_user_id = $7,
                updated_at = now()
            WHERE id = $1
            RETURNING
                id, project_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                created_by, created_by_id, created_at, updated_at
            "#,
        )
        .bind(ticket_id)
        .bind(title)
        .bind(description)
        .bind(repo_id)
        .bind(priority_str)
        .bind(&branch_name)
        .bind(owner_user_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        self.enrich_ticket(row_to_ticket(&row)).await
    }

    pub async fn update_status(
        &self,
        ticket_id: Uuid,
        status: TicketStatus,
        substatus: Option<Option<Substatus>>,
        substatus_metadata: Option<Option<Value>>,
    ) -> Result<TicketWithDisplay, TicketError> {
        let current = self.get(ticket_id).await?;
        let substatus = match substatus {
            Some(value) => value,
            None => current.ticket.substatus,
        };
        let substatus_metadata = match substatus_metadata {
            Some(value) => value,
            None => current.ticket.substatus_metadata.clone(),
        };

        if let Some(msg) =
            validate_status_substatus_combo(status, substatus, &substatus_metadata)
        {
            return Err(TicketError::Validation(msg.to_string()));
        }

        let substatus_str = substatus.map(substatus_to_str);

        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET
                status = $2,
                substatus = $3,
                substatus_metadata = $4,
                updated_at = now()
            WHERE id = $1
            RETURNING
                id, project_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                created_by, created_by_id, created_at, updated_at
            "#,
        )
        .bind(ticket_id)
        .bind(status_to_str(status))
        .bind(substatus_str)
        .bind(&substatus_metadata)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        self.enrich_ticket(row_to_ticket(&row)).await
    }

    pub async fn assign_agent(
        &self,
        ticket_id: Uuid,
        agent_id: Option<Uuid>,
    ) -> Result<TicketWithDisplay, TicketError> {
        let row = sqlx::query(
            r#"
            UPDATE tickets
            SET assignee_agent_id = $2, updated_at = now()
            WHERE id = $1
            RETURNING
                id, project_id, repo_id, title, description,
                status, substatus, substatus_metadata, priority,
                assignee_agent_id, owner_user_id, branch_name,
                created_by, created_by_id, created_at, updated_at
            "#,
        )
        .bind(ticket_id)
        .bind(agent_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        self.enrich_ticket(row_to_ticket(&row)).await
    }

    pub async fn compute_last_activity_at(
        &self,
        ticket_id: Uuid,
    ) -> Result<OffsetDateTime, TicketError> {
        let row = sqlx::query(
            r#"
            SELECT GREATEST(
                t.updated_at,
                COALESCE(
                    (SELECT MAX(c.created_at) FROM ticket_comments c WHERE c.ticket_id = t.id),
                    t.updated_at
                )
            ) AS last_activity_at
            FROM tickets t
            WHERE t.id = $1
            "#,
        )
        .bind(ticket_id)
        .fetch_optional(self.pool)
        .await?
        .ok_or(TicketError::TicketNotFound)?;

        Ok(row.get("last_activity_at"))
    }

    async fn ensure_project_exists(&self, project_id: Uuid) -> Result<(), TicketError> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1)",
        )
        .bind(project_id)
        .fetch_one(self.pool)
        .await?;

        if !exists {
            return Err(TicketError::ProjectNotFound);
        }
        Ok(())
    }

    async fn enrich_ticket(&self, ticket: Ticket) -> Result<TicketWithDisplay, TicketError> {
        let agent_name = if let Some(agent_id) = ticket.assignee_agent_id {
            sqlx::query_scalar::<_, Option<String>>("SELECT name FROM agents WHERE id = $1")
                .bind(agent_id)
                .fetch_optional(self.pool)
                .await?
                .flatten()
        } else {
            None
        };

        let substatus_display = build_substatus_display(
            ticket.substatus,
            &ticket.substatus_metadata,
            agent_name.as_deref(),
        );
        let last_activity_at = self.compute_last_activity_at(ticket.id).await?;

        Ok(TicketWithDisplay {
            ticket,
            substatus_display,
            last_activity_at,
        })
    }
}

fn row_to_ticket(row: &sqlx::postgres::PgRow) -> Ticket {
    let status_str: String = row.get("status");
    let status = status_from_str(&status_str).unwrap_or(TicketStatus::Backlog);

    let substatus: Option<String> = row.get("substatus");
    let substatus = substatus
        .as_deref()
        .and_then(substatus_from_str);

    let priority: Option<String> = row.get("priority");
    let priority = priority.as_deref().and_then(priority_from_str);

    Ticket {
        id: row.get("id"),
        project_id: row.get("project_id"),
        repo_id: row.get("repo_id"),
        title: row.get("title"),
        description: row.get("description"),
        status,
        substatus,
        substatus_metadata: row.get("substatus_metadata"),
        priority,
        assignee_agent_id: row.get("assignee_agent_id"),
        owner_user_id: row.get("owner_user_id"),
        branch_name: row.get("branch_name"),
        created_by: row.get("created_by"),
        created_by_id: row.get("created_by_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
