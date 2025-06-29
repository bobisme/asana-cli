use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::domain::*;

// Asana API response wrapper
#[derive(Debug, Deserialize)]
pub struct AsanaResponse<T> {
    pub data: T,
}

#[derive(Debug, Deserialize)]
pub struct AsanaListResponse<T> {
    pub data: Vec<T>,
    // Pagination not implemented - field kept for API compatibility
    #[allow(dead_code)]
    pub next_page: Option<serde_json::Value>,
}

// DTOs for API communication
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskDto {
    pub gid: String,
    pub name: String,
    pub notes: Option<String>,
    pub html_notes: Option<String>,
    pub completed: bool,
    pub due_on: Option<String>, // YYYY-MM-DD format
    pub due_at: Option<String>, // ISO 8601 format
    pub assignee: Option<UserDto>,
    pub projects: Vec<ProjectDto>,
    pub tags: Vec<TagDto>,
    pub created_at: String,
    pub modified_at: String,
    pub workspace: Option<WorkspaceDto>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserDto {
    pub gid: String,
    pub name: String,
    pub email: Option<String>,
    pub photo: Option<PhotoDto>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PhotoDto {
    pub image_60x60: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectDto {
    pub gid: String,
    pub name: String,
    pub notes: Option<String>,
    pub color: Option<String>,
    pub archived: Option<bool>,
    pub workspace: Option<WorkspaceDto>,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceDto {
    pub gid: String,
    pub name: String,
    pub is_organization: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagDto {
    pub gid: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommentDto {
    pub gid: String,
    pub text: String,
    pub created_by: Option<UserDto>,
    pub created_at: String,
    #[serde(rename = "type")]
    pub story_type: Option<String>,
    pub resource_subtype: Option<String>,
}

// Request DTOs
#[derive(Debug, Serialize)]
pub struct TaskUpdateDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_on: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<Option<String>>,
}

#[derive(Debug, Serialize)]
pub struct CommentCreateDto {
    pub text: String,
}

// Conversion implementations
impl From<TaskDto> for Task {
    fn from(dto: TaskDto) -> Self {
        // Parse due date - prefer due_at over due_on
        let due_date = if let Some(due_at) = dto.due_at {
            DateTime::parse_from_rfc3339(&due_at)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        } else if let Some(due_on) = dto.due_on {
            // Parse YYYY-MM-DD and assume UTC midnight
            chrono::NaiveDate::parse_from_str(&due_on, "%Y-%m-%d")
                .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_utc())
                .ok()
        } else {
            None
        };

        Self {
            id: TaskId(dto.gid),
            name: dto.name,
            description: dto.html_notes.or(dto.notes),
            completed: dto.completed,
            due_date,
            assignee: dto.assignee.as_ref().map(|u| UserId(u.gid.clone())),
            assignee_name: dto.assignee.map(|u| u.name),
            projects: dto.projects.into_iter().map(|p| ProjectId(p.gid)).collect(),
            tags: dto.tags.into_iter().map(|t| t.name).collect(),
            created_at: DateTime::parse_from_rfc3339(&dto.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            modified_at: DateTime::parse_from_rfc3339(&dto.modified_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            workspace: WorkspaceId(dto.workspace.map(|w| w.gid).unwrap_or_else(|| "unknown".to_string())),
        }
    }
}

impl From<UserDto> for User {
    fn from(dto: UserDto) -> Self {
        Self {
            id: UserId(dto.gid),
            name: dto.name,
            email: dto.email.unwrap_or_default(),
            photo: dto.photo.and_then(|p| p.image_60x60),
        }
    }
}

impl From<ProjectDto> for Project {
    fn from(dto: ProjectDto) -> Self {
        Self {
            id: ProjectId(dto.gid),
            name: dto.name,
            description: dto.notes,
            color: dto.color,
            archived: dto.archived.unwrap_or(false),
            workspace: dto.workspace
                .map(|w| WorkspaceId(w.gid))
                .unwrap_or_else(|| WorkspaceId("unknown".to_string())),
            created_at: dto.created_at
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            modified_at: dto.modified_at
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
        }
    }
}

impl From<WorkspaceDto> for Workspace {
    fn from(dto: WorkspaceDto) -> Self {
        Self {
            id: WorkspaceId(dto.gid),
            name: dto.name,
            is_organization: dto.is_organization.unwrap_or(false),
        }
    }
}

impl From<CommentDto> for Comment {
    fn from(dto: CommentDto) -> Self {
        Self {
            id: CommentId(dto.gid),
            text: dto.text,
            author: dto.created_by.map(|author| author.into()),
            created_at: DateTime::parse_from_rfc3339(&dto.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            task_id: TaskId("unknown".to_string()), // Will be set by caller
            story_type: dto.story_type,
            resource_subtype: dto.resource_subtype,
        }
    }
}

impl From<TaskUpdate> for TaskUpdateDto {
    fn from(update: TaskUpdate) -> Self {
        Self {
            name: update.name,
            notes: update.description,
            completed: update.completed,
            due_on: update.due_date.map(|opt_date| 
                opt_date.map(|date| date.format("%Y-%m-%d").to_string())
            ),
            assignee: update.assignee.map(|opt_user| 
                opt_user.map(|user| user.0)
            ),
        }
    }
}