use crate::domain::{Comment, Task, TaskFilter, TaskId, TaskUpdate, User, Workspace};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RepositoryError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limited: retry after {0} seconds")]
    RateLimit(u64),

    #[error("API error: {0}")]
    Api(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

pub type RepositoryResult<T> = Result<T, RepositoryError>;

#[async_trait]
pub trait TaskRepository: Send + Sync {
    async fn get_task(&self, id: &TaskId) -> RepositoryResult<Task>;
    async fn list_tasks(&self, filter: &TaskFilter) -> RepositoryResult<Vec<Task>>;
    async fn update_task(&self, id: &TaskId, updates: &TaskUpdate) -> RepositoryResult<Task>;
    async fn get_task_comments(&self, task_id: &TaskId) -> RepositoryResult<Vec<Comment>>;
    #[allow(dead_code)] // Might be used when comment creation is added to TUI
    async fn create_comment(&self, task_id: &TaskId, content: &str) -> RepositoryResult<Comment>;
}

#[async_trait]
pub trait WorkspaceRepository: Send + Sync {
    async fn list_workspaces(&self) -> RepositoryResult<Vec<Workspace>>;
    async fn get_current_user(&self) -> RepositoryResult<User>;
}
