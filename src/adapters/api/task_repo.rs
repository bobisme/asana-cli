use super::{
    AsanaClient, CommentCreateDto, CommentDto, TaskDto, TaskUpdateDto, UserDto, WorkspaceDto,
};
use crate::domain::*;
use crate::ports::{RepositoryResult, TaskRepository, WorkspaceRepository};
use async_trait::async_trait;

pub struct AsanaTaskRepository {
    client: AsanaClient,
}

impl AsanaTaskRepository {
    pub fn new(client: AsanaClient) -> Self {
        Self { client }
    }

    fn build_task_query_params(&self, filter: &TaskFilter) -> Vec<(String, String)> {
        let mut params = Vec::new();

        if let Some(workspace) = &filter.workspace {
            params.push(("workspace".to_string(), workspace.0.clone()));
        }

        if let Some(project) = &filter.project {
            params.push(("project".to_string(), project.0.clone()));
        }

        if let Some(assignee) = &filter.assignee {
            params.push(("assignee".to_string(), assignee.0.clone()));
        }

        if let Some(completed) = filter.completed {
            if !completed {
                // For incomplete tasks, use completed_since=now to exclude recently completed tasks
                params.push(("completed_since".to_string(), "now".to_string()));
            }
            // For completed tasks, we don't add completed_since parameter
        }

        if let Some(limit) = filter.limit {
            params.push(("limit".to_string(), limit.to_string()));
        }

        if let Some(offset) = filter.offset {
            params.push(("offset".to_string(), offset.to_string()));
        }

        // Add fields we want to retrieve
        params.push((
            "opt_fields".to_string(),
            "gid,name,notes,html_notes,completed,due_on,due_at,assignee.gid,assignee.name,assignee.email,projects.gid,projects.name,tags.gid,tags.name,created_at,modified_at,workspace.gid,workspace.name".to_string(),
        ));

        params
    }

    fn build_query_string(&self, params: &[(String, String)]) -> String {
        if params.is_empty() {
            return String::new();
        }

        format!(
            "?{}",
            params
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
                .collect::<Vec<_>>()
                .join("&")
        )
    }
}

#[async_trait]
impl TaskRepository for AsanaTaskRepository {
    async fn get_task(&self, id: &TaskId) -> RepositoryResult<Task> {
        let path = format!(
            "/tasks/{}?opt_fields=gid,name,notes,html_notes,completed,due_on,due_at,assignee.gid,assignee.name,assignee.email,projects.gid,projects.name,tags.gid,tags.name,created_at,modified_at,workspace.gid,workspace.name",
            id.0
        );

        let task_dto: TaskDto = self.client.get(&path).await?;
        Ok(task_dto.into())
    }

    async fn list_tasks(&self, filter: &TaskFilter) -> RepositoryResult<Vec<Task>> {
        let params = self.build_task_query_params(filter);
        let query_string = self.build_query_string(&params);
        let path = format!("/tasks{query_string}");

        let task_dtos: Vec<TaskDto> = self.client.get_list(&path).await?;
        Ok(task_dtos.into_iter().map(|dto| dto.into()).collect())
    }

    async fn update_task(&self, id: &TaskId, updates: &TaskUpdate) -> RepositoryResult<Task> {
        let path = format!("/tasks/{}", id.0);
        let update_dto: TaskUpdateDto = updates.clone().into();

        let task_dto: TaskDto = self.client.put(&path, &update_dto).await?;
        Ok(task_dto.into())
    }

    async fn get_task_comments(&self, task_id: &TaskId) -> RepositoryResult<Vec<Comment>> {
        let path = format!(
            "/tasks/{}/stories?opt_fields=gid,text,created_by.gid,created_by.name,created_by.email,created_at,type,resource_subtype",
            task_id.0
        );

        let comment_dtos: Vec<CommentDto> = self.client.get_list(&path).await?;
        Ok(comment_dtos
            .into_iter()
            .map(|dto| {
                let mut comment: Comment = dto.into();
                comment.task_id = task_id.clone();
                comment
            })
            .collect())
    }

    async fn create_comment(&self, task_id: &TaskId, content: &str) -> RepositoryResult<Comment> {
        let path = format!("/tasks/{}/stories", task_id.0);
        let create_dto = CommentCreateDto {
            text: content.to_string(),
        };

        let comment_dto: CommentDto = self.client.post(&path, &create_dto).await?;
        let mut comment: Comment = comment_dto.into();
        comment.task_id = task_id.clone();
        Ok(comment)
    }
}

#[async_trait]
impl WorkspaceRepository for AsanaTaskRepository {
    async fn list_workspaces(&self) -> RepositoryResult<Vec<Workspace>> {
        let path = "/workspaces?opt_fields=gid,name,is_organization";

        let workspace_dtos: Vec<WorkspaceDto> = self.client.get(path).await?;
        Ok(workspace_dtos.into_iter().map(|dto| dto.into()).collect())
    }

    async fn get_current_user(&self) -> RepositoryResult<User> {
        let path = "/users/me?opt_fields=gid,name,email,photo.image_60x60";

        let user_dto: UserDto = self.client.get(path).await?;
        Ok(user_dto.into())
    }
}
