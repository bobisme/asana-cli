use std::future::Future;

use crate::{
    app::error::RepositoryResult,
    domain::{user::User, workspace::Workspace},
};

pub trait WorkspaceRepository: Send + Sync + 'static {
    fn list_workspaces(&self) -> impl Future<Output = RepositoryResult<Vec<Workspace>>> + Send;
    fn get_current_user(&self) -> impl Future<Output = RepositoryResult<User>> + Send;
}
