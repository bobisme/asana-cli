use crate::ports::{ConfigError, RepositoryError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Application error: {0}")]
    Application(String),

    #[error("Authentication required")]
    AuthenticationRequired,

    #[error("Workspace not configured")]
    WorkspaceNotConfigured,
}

pub type AppResult<T> = Result<T, AppError>;
