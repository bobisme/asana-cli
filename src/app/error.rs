use thiserror::Error;

use crate::ports::ConfigError;

#[derive(Error, Debug, Clone)]
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

#[derive(Error, Debug, Clone)]
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
