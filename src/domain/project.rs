use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(pub String);

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ProjectId {
    fn from(s: String) -> Self {
        ProjectId(s)
    }
}

impl From<&str> for ProjectId {
    fn from(s: &str) -> Self {
        ProjectId(s.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub archived: bool,
    pub workspace: super::WorkspaceId,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}
