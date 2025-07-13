use crate::{
    domain::workspace::WorkspaceId,
    ports::{AppConfig, ConfigError, ConfigResult, ConfigStore},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Serialize, Deserialize)]
struct ConfigFile {
    default_workspace: Option<String>,
    cache_ttl_seconds: Option<u64>,
    max_tasks_per_page: Option<usize>,
}

pub struct FileConfigStore {
    config_path: PathBuf,
    keyring_service: String,
}

impl FileConfigStore {
    pub fn new() -> ConfigResult<Self> {
        let config_dir = dirs::config_dir().ok_or_else(|| {
            ConfigError::ReadError("Cannot determine config directory".to_string())
        })?;

        let app_config_dir = config_dir.join("asana-cli");
        let config_path = app_config_dir.join("config.json");

        Ok(Self {
            config_path,
            keyring_service: "asana-cli".to_string(),
        })
    }

    async fn ensure_config_dir(&self) -> ConfigResult<()> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| ConfigError::WriteError(e.to_string()))?;
        }
        Ok(())
    }

    fn token_file_path(&self) -> PathBuf {
        self.config_path.parent().unwrap().join(".token")
    }

    async fn get_token_from_file(&self) -> ConfigResult<Option<String>> {
        let token_path = self.token_file_path();
        match fs::read_to_string(&token_path).await {
            Ok(token) => Ok(Some(token.trim().to_string())),
            Err(_) => Ok(None), // File doesn't exist or can't be read
        }
    }

    async fn set_token_in_file(&self, token: &str) -> ConfigResult<()> {
        self.ensure_config_dir().await?;
        let token_path = self.token_file_path();
        fs::write(&token_path, token)
            .await
            .map_err(|e| ConfigError::WriteError(e.to_string()))?;

        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&token_path)
                .await
                .map_err(|e| ConfigError::WriteError(e.to_string()))?
                .permissions();
            perms.set_mode(0o600); // Read/write for owner only
            fs::set_permissions(&token_path, perms)
                .await
                .map_err(|e| ConfigError::WriteError(e.to_string()))?;
        }

        Ok(())
    }
}

impl ConfigStore for FileConfigStore {
    async fn load_config(&self) -> ConfigResult<AppConfig> {
        let content = match fs::read_to_string(&self.config_path).await {
            Ok(content) => content,
            Err(_) => {
                // Config file doesn't exist, create default config with just API token
                let api_token = self.get_api_token().await?;
                return Ok(AppConfig {
                    api_token,
                    ..Default::default()
                });
            }
        };

        let config_file: ConfigFile = serde_json::from_str(&content)
            .map_err(|e| ConfigError::InvalidFormat(e.to_string()))?;

        // Always try to get the latest API token (from keyring, file, or env)
        let mut api_token = self.get_api_token().await?;

        // If no token from storage, check environment variable as final fallback
        if api_token.is_none() {
            if let Ok(env_token) = std::env::var("ASANA_TOKEN") {
                api_token = Some(env_token);
            }
        }

        Ok(AppConfig {
            api_token,
            default_workspace: config_file.default_workspace.map(WorkspaceId),
            cache_ttl_seconds: config_file.cache_ttl_seconds.unwrap_or(300),
            max_tasks_per_page: config_file.max_tasks_per_page.unwrap_or(50),
        })
    }

    async fn save_config(&self, config: &AppConfig) -> ConfigResult<()> {
        self.ensure_config_dir().await?;

        let config_file = ConfigFile {
            default_workspace: config.default_workspace.as_ref().map(|w| w.0.clone()),
            cache_ttl_seconds: Some(config.cache_ttl_seconds),
            max_tasks_per_page: Some(config.max_tasks_per_page),
        };

        let content = serde_json::to_string_pretty(&config_file)
            .map_err(|e| ConfigError::WriteError(e.to_string()))?;

        fs::write(&self.config_path, content)
            .await
            .map_err(|e| ConfigError::WriteError(e.to_string()))?;

        // Save API token separately if provided
        if let Some(token) = &config.api_token {
            self.set_api_token(token).await?;
        }

        Ok(())
    }

    async fn get_api_token(&self) -> ConfigResult<Option<String>> {
        // Try keyring first, fall back to environment variable, then file
        match keyring::Entry::new(&self.keyring_service, "api_token") {
            Ok(entry) => match entry.get_password() {
                Ok(token) => return Ok(Some(token)),
                Err(keyring::Error::NoEntry) => {
                    // No token in keyring, try other methods
                }
                Err(_) => {
                    // Keyring not available, try other methods
                    tracing::warn!("Keyring not available, falling back to file storage");
                }
            },
            Err(_) => {
                // Keyring service not available
                tracing::warn!("Keyring service not available, falling back to file storage");
            }
        }

        // Try reading from file as fallback
        self.get_token_from_file().await
    }

    async fn set_api_token(&self, token: &str) -> ConfigResult<()> {
        // Try keyring first, fall back to file storage
        match keyring::Entry::new(&self.keyring_service, "api_token") {
            Ok(entry) => match entry.set_password(token) {
                Ok(()) => return Ok(()),
                Err(_) => {
                    tracing::warn!("Failed to store in keyring, falling back to file storage");
                }
            },
            Err(_) => {
                tracing::warn!("Keyring not available, using file storage");
            }
        }

        // Fall back to file storage
        self.set_token_in_file(token).await
    }
}
