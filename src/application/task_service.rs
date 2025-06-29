use super::AppResult;
use crate::domain::*;
use crate::ports::{Cache, TaskRepository};
use std::sync::Arc;

pub struct TaskService {
    repository: Arc<dyn TaskRepository>,
    cache: Arc<dyn Cache<TaskId, Task>>,
    comment_cache: Arc<dyn Cache<TaskId, Vec<Comment>>>,
}

impl TaskService {
    pub fn new(
        repository: Arc<dyn TaskRepository>,
        cache: Arc<dyn Cache<TaskId, Task>>,
        comment_cache: Arc<dyn Cache<TaskId, Vec<Comment>>>,
    ) -> Self {
        Self {
            repository,
            cache,
            comment_cache,
        }
    }

    pub async fn get_task(&self, id: &TaskId, use_cache: bool) -> AppResult<Task> {
        if use_cache {
            if let Some(task) = self.cache.get(id).await {
                return Ok(task);
            }
        }

        let task = self.repository.get_task(id).await?;
        self.cache.insert(id.clone(), task.clone()).await;
        Ok(task)
    }

    pub async fn list_tasks(&self, filter: &TaskFilter, _use_cache: bool) -> AppResult<Vec<Task>> {
        // For list operations, we don't cache the entire list but we do cache individual tasks
        let tasks = self.repository.list_tasks(filter).await?;

        // Cache individual tasks for future single-task lookups
        for task in &tasks {
            self.cache.insert(task.id.clone(), task.clone()).await;
        }

        Ok(tasks)
    }

    pub async fn update_task(&self, id: &TaskId, updates: &TaskUpdate) -> AppResult<Task> {
        let updated_task = self.repository.update_task(id, updates).await?;

        // Update cache with new data
        self.cache.insert(id.clone(), updated_task.clone()).await;

        Ok(updated_task)
    }

    pub async fn toggle_task_completion(&self, id: &TaskId) -> AppResult<Task> {
        let task = self.get_task(id, true).await?;

        let update = TaskUpdate {
            completed: Some(!task.completed),
            ..Default::default()
        };

        self.update_task(id, &update).await
    }

    pub async fn get_task_comments(
        &self,
        task_id: &TaskId,
        use_cache: bool,
    ) -> AppResult<Vec<Comment>> {
        if use_cache {
            if let Some(comments) = self.comment_cache.get(task_id).await {
                return Ok(comments);
            }
        }

        let comments = self.repository.get_task_comments(task_id).await?;
        self.comment_cache
            .insert(task_id.clone(), comments.clone())
            .await;
        Ok(comments)
    }

    #[allow(dead_code)] // Might be used when comment creation is added to TUI
    pub async fn create_comment(&self, task_id: &TaskId, content: &str) -> AppResult<Comment> {
        let comment = self.repository.create_comment(task_id, content).await?;

        // Invalidate comment cache for this task to force refresh
        self.comment_cache.remove(task_id).await;

        Ok(comment)
    }
}
