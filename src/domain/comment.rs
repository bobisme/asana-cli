use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommentId(pub String);

impl fmt::Display for CommentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for CommentId {
    fn from(s: String) -> Self {
        CommentId(s)
    }
}

impl From<&str> for CommentId {
    fn from(s: &str) -> Self {
        CommentId(s.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub id: CommentId,
    pub text: Option<String>,
    pub author: Option<super::User>,
    pub created_at: DateTime<Utc>,
    pub task_id: super::TaskId,
    pub story_type: Option<String>,
    pub resource_subtype: Option<String>,
}
