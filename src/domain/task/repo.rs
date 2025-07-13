use std::future::Future;

use super::model::{Task, TaskFilter, TaskId, TaskUpdate};
use crate::{app::error::RepositoryResult, domain::comment::model::Comment};

pub trait TaskRepository: Send + Sync + 'static {
    async fn get_task(&self, id: &TaskId) -> RepositoryResult<Task>;
    fn list_tasks(
        &self,
        filter: &TaskFilter,
    ) -> impl Future<Output = RepositoryResult<Vec<Task>>> + Send;
    async fn update_task(&self, id: &TaskId, updates: &TaskUpdate) -> RepositoryResult<Task>;
    async fn get_task_comments(&self, task_id: &TaskId) -> RepositoryResult<Vec<Comment>>;
    async fn create_comment(&self, task_id: &TaskId, content: &str) -> RepositoryResult<Comment>;
}
