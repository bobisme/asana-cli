use super::{AppError, AppResult, TaskService};
use crate::domain::*;
use crate::ports::{ConfigStore, WorkspaceRepository};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CachedList<T> {
    pub items: Vec<T>,
    pub fetched_at: DateTime<Utc>,
}

pub struct StateManager {
    task_service: Arc<TaskService>,
    workspace_repo: Arc<dyn WorkspaceRepository>,
    config_store: Arc<dyn ConfigStore>,

    // List caches
    task_list_cache: DashMap<String, CachedList<Task>>,

    // Application state
    current_workspace: tokio::sync::RwLock<Option<WorkspaceId>>,
    current_user: tokio::sync::RwLock<Option<User>>,
}

impl StateManager {
    pub fn new(
        task_service: Arc<TaskService>,
        workspace_repo: Arc<dyn WorkspaceRepository>,
        config_store: Arc<dyn ConfigStore>,
    ) -> Self {
        Self {
            task_service,
            workspace_repo,
            config_store,
            task_list_cache: DashMap::new(),
            current_workspace: tokio::sync::RwLock::new(None),
            current_user: tokio::sync::RwLock::new(None),
        }
    }

    pub async fn initialize(&self) -> AppResult<()> {
        // Load configuration
        let mut config = self.config_store.load_config().await?;

        // Load current user first to verify authentication
        match self.workspace_repo.get_current_user().await {
            Ok(user) => {
                *self.current_user.write().await = Some(user);
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        // Set current workspace from config, or auto-select if there's only one
        let mut workspace_set = false;
        if let Some(workspace_id) = config.default_workspace.clone() {
            *self.current_workspace.write().await = Some(workspace_id);
            workspace_set = true;
        }

        // If no workspace configured, try to auto-select
        if !workspace_set {
            match self.workspace_repo.list_workspaces().await {
                Ok(workspaces) => {
                    if workspaces.len() == 1 {
                        // Only one workspace, auto-select it
                        let workspace_id = workspaces[0].id.clone();
                        tracing::info!("Auto-selecting workspace: {}", workspace_id);

                        *self.current_workspace.write().await = Some(workspace_id.clone());

                        // Update config to remember this choice
                        config.default_workspace = Some(workspace_id);
                        if let Err(e) = self.config_store.save_config(&config).await {
                            tracing::warn!("Failed to save workspace config: {}", e);
                        }
                    } else if workspaces.is_empty() {
                        return Err(crate::application::AppError::Application(
                            "No workspaces found for this account".to_string(),
                        ));
                    } else {
                        // Multiple workspaces - user needs to choose
                        tracing::info!(
                            "Multiple workspaces found ({}), user needs to select one",
                            workspaces.len()
                        );
                        for workspace in &workspaces {
                            tracing::info!("  - {} ({})", workspace.name, workspace.id);
                        }
                        return Err(crate::application::AppError::Application(
                            format!("Multiple workspaces found ({}). Please specify one with --workspace <ID>", workspaces.len())
                        ));
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to list workspaces: {}", e);
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }

    pub async fn get_current_workspace(&self) -> Option<WorkspaceId> {
        self.current_workspace.read().await.clone()
    }

    pub async fn get_current_user(&self) -> Option<User> {
        self.current_user.read().await.clone()
    }

    pub async fn get_tasks_for_current_workspace(&self, use_cache: bool) -> AppResult<Vec<Task>> {
        let workspace = self
            .get_current_workspace()
            .await
            .ok_or(AppError::WorkspaceNotConfigured)?;

        let current_user = self
            .get_current_user()
            .await
            .ok_or(AppError::Application("Current user not loaded".to_string()))?;

        let filter = TaskFilter {
            workspace: Some(workspace),
            assignee: Some(current_user.id), // Use current user to satisfy API requirement
            completed: Some(false),          // Only incomplete tasks for main view
            limit: Some(50),
            ..Default::default()
        };

        self.get_tasks_with_filter(&filter, use_cache).await
    }

    pub async fn get_tasks_with_filter(
        &self,
        filter: &TaskFilter,
        use_cache: bool,
    ) -> AppResult<Vec<Task>> {
        let cache_key = filter.to_cache_key();

        if use_cache {
            if let Some(cached) = self.task_list_cache.get(&cache_key) {
                let age = Utc::now() - cached.fetched_at;
                if age < chrono::Duration::minutes(5) {
                    return Ok(cached.items.clone());
                }
            }
        }

        let tasks = self.task_service.list_tasks(filter, false).await?;

        // Sort by due date (ascending, with None at the end)
        let mut sorted_tasks = tasks;
        sorted_tasks.sort_by(|a, b| match (a.due_date, b.due_date) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(a_due), Some(b_due)) => a_due.cmp(&b_due),
        });

        // Update cache
        self.task_list_cache.insert(
            cache_key.clone(),
            CachedList {
                items: sorted_tasks.clone(),
                fetched_at: Utc::now(),
            },
        );

        Ok(sorted_tasks)
    }

    pub async fn get_task(&self, id: &TaskId) -> AppResult<Task> {
        self.task_service.get_task(id, true).await
    }

    pub async fn toggle_task_completion(&self, id: &TaskId) -> AppResult<Task> {
        let result = self.task_service.toggle_task_completion(id).await;

        // Invalidate task list caches since completion status changed
        self.task_list_cache.clear();

        result
    }

    pub async fn get_task_comments(&self, task_id: &TaskId) -> AppResult<Vec<Comment>> {
        self.task_service.get_task_comments(task_id, true).await
    }
}
