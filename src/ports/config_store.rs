use async_trait::async_trait;
use thiserror::Error;
use crate::domain::WorkspaceId;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration file not found")]
    NotFound,
    
    #[error("Failed to read configuration: {0}")]
    ReadError(String),
    
    #[error("Failed to write configuration: {0}")]
    WriteError(String),
    
    #[error("Invalid configuration format: {0}")]
    InvalidFormat(String),
    
    #[error("Credential storage error: {0}")]
    CredentialError(String),
}

pub type ConfigResult<T> = Result<T, ConfigError>;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub api_token: Option<String>,
    pub default_workspace: Option<WorkspaceId>,
    pub cache_ttl_seconds: u64,
    pub max_tasks_per_page: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_token: None,
            default_workspace: None,
            cache_ttl_seconds: 300, // 5 minutes
            max_tasks_per_page: 50,
        }
    }
}

#[async_trait]
pub trait ConfigStore {
    async fn load_config(&self) -> ConfigResult<AppConfig>;
    async fn save_config(&self, config: &AppConfig) -> ConfigResult<()>;
    async fn get_api_token(&self) -> ConfigResult<Option<String>>;
    async fn set_api_token(&self, token: &str) -> ConfigResult<()>;
    async fn clear_api_token(&self) -> ConfigResult<()>;
}