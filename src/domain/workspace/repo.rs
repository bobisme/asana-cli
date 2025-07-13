use crate::{
    app::error::RepositoryResult,
    domain::{user::User, workspace::Workspace},
};

pub trait WorkspaceRepository: Send + Sync {
    async fn list_workspaces(&self) -> RepositoryResult<Vec<Workspace>>;
    async fn get_current_user(&self) -> RepositoryResult<User>;
}
