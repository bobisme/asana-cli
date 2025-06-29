use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use std::time::Duration;
use crate::ports::{RepositoryError, RepositoryResult};
use super::dto::{AsanaResponse, AsanaListResponse};

const ASANA_API_BASE: &str = "https://app.asana.com/api/1.0";

pub struct AsanaClient {
    client: Client,
    api_token: String,
}

impl AsanaClient {
    pub fn new(api_token: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("asana-cli/0.1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self { client, api_token }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> RepositoryResult<T> {
        let url = format!("{}{}", ASANA_API_BASE, path);
        
        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await
            .map_err(|e| RepositoryError::Network(e.to_string()))?;

        self.handle_response(response).await
    }

    pub async fn get_list<T: DeserializeOwned>(&self, path: &str) -> RepositoryResult<Vec<T>> {
        let url = format!("{}{}", ASANA_API_BASE, path);
        
        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await
            .map_err(|e| RepositoryError::Network(e.to_string()))?;

        let response_text = response
            .text()
            .await
            .map_err(|e| RepositoryError::Network(e.to_string()))?;
        
        tracing::debug!("API List Response: {}", response_text);
        
        let list_response: AsanaListResponse<T> = serde_json::from_str(&response_text)
            .map_err(|e| RepositoryError::Serialization(format!("Failed to parse list response: {}. Response was: {}", e, response_text)))?;
        Ok(list_response.data)
    }

    pub async fn put<T: DeserializeOwned, R: serde::Serialize>(
        &self,
        path: &str,
        body: &R,
    ) -> RepositoryResult<T> {
        let url = format!("{}{}", ASANA_API_BASE, path);
        
        let response = self
            .client
            .put(&url)
            .bearer_auth(&self.api_token)
            .json(&serde_json::json!({ "data": body }))
            .send()
            .await
            .map_err(|e| RepositoryError::Network(e.to_string()))?;

        self.handle_response(response).await
    }

    pub async fn post<T: DeserializeOwned, R: serde::Serialize>(
        &self,
        path: &str,
        body: &R,
    ) -> RepositoryResult<T> {
        let url = format!("{}{}", ASANA_API_BASE, path);
        
        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_token)
            .json(&serde_json::json!({ "data": body }))
            .send()
            .await
            .map_err(|e| RepositoryError::Network(e.to_string()))?;

        self.handle_response(response).await
    }

    async fn handle_response<T: DeserializeOwned>(&self, response: Response) -> RepositoryResult<T> {
        let status = response.status();
        
        match status.as_u16() {
            200..=299 => {
                let response_text = response
                    .text()
                    .await
                    .map_err(|e| RepositoryError::Network(e.to_string()))?;
                
                tracing::debug!("API Response: {}", response_text);
                
                let asana_response: AsanaResponse<T> = serde_json::from_str(&response_text)
                    .map_err(|e| RepositoryError::Serialization(format!("Failed to parse response: {}. Response was: {}", e, response_text)))?;
                Ok(asana_response.data)
            }
            401 => Err(RepositoryError::Authentication(
                "Invalid API token".to_string()
            )),
            404 => Err(RepositoryError::NotFound(
                "Resource not found".to_string()
            )),
            429 => {
                // Extract retry-after header if available
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(60);
                Err(RepositoryError::RateLimit(retry_after))
            }
            _ => {
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                Err(RepositoryError::Api(format!(
                    "HTTP {}: {}",
                    status, error_text
                )))
            }
        }
    }
}