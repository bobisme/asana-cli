use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TaskId {
    fn from(s: String) -> Self {
        TaskId(s)
    }
}

impl From<&str> for TaskId {
    fn from(s: &str) -> Self {
        TaskId(s.to_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

impl Priority {
    pub fn color(&self) -> &'static str {
        match self {
            Priority::Low => "gray",
            Priority::Medium => "blue",
            Priority::High => "yellow",
            Priority::Critical => "red",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub description: Option<String>,
    pub completed: bool,
    pub due_date: Option<DateTime<Utc>>,
    pub assignee: Option<super::UserId>,
    pub assignee_name: Option<String>,
    pub projects: Vec<super::ProjectId>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub workspace: super::WorkspaceId,
}

impl Task {
    /// Business rule: determine if task is overdue
    pub fn is_overdue(&self) -> bool {
        self.due_date
            .map(|due| due < Utc::now() && !self.completed)
            .unwrap_or(false)
    }
    
    /// Business rule: calculate task priority based on due date and status
    pub fn priority(&self) -> Priority {
        if self.completed {
            return Priority::Low;
        }
        
        match self.due_date {
            None => Priority::Low,
            Some(_due) if self.is_overdue() => Priority::Critical,
            Some(due) if due.date_naive() == Utc::now().date_naive() => Priority::High,
            Some(due) if (due - Utc::now()).num_days() <= 3 => Priority::Medium,
            _ => Priority::Low,
        }
    }
    
    /// Business rule: get display status with color
    pub fn status_display(&self) -> (&'static str, &'static str) {
        if self.completed {
            ("Complete", "green")
        } else {
            ("Incomplete", "gray")
        }
    }
    
    /// Format due date for display
    pub fn due_date_display(&self) -> String {
        match self.due_date {
            None => "No due date".to_string(),
            Some(due) => {
                let now = Utc::now();
                let days_diff = (due.date_naive() - now.date_naive()).num_days();
                
                match days_diff {
                    0 => "Today".to_string(),
                    1 => "Tomorrow".to_string(),
                    -1 => "Yesterday".to_string(),
                    d if d < 0 => format!("{} days ago", -d),
                    d if d <= 7 => format!("In {} days", d),
                    _ => due.format("%Y-%m-%d").to_string(),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TaskUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub completed: Option<bool>,
    pub due_date: Option<Option<DateTime<Utc>>>,
    pub assignee: Option<Option<super::UserId>>,
}

#[derive(Debug, Clone)]
pub struct TaskFilter {
    pub workspace: Option<super::WorkspaceId>,
    pub project: Option<super::ProjectId>,
    pub assignee: Option<super::UserId>,
    pub completed: Option<bool>,
    pub search_query: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl Default for TaskFilter {
    fn default() -> Self {
        Self {
            workspace: None,
            project: None,
            assignee: None,
            completed: Some(false), // Default to incomplete tasks
            search_query: None,
            limit: Some(50),
            offset: None,
        }
    }
}

impl TaskFilter {
    pub fn to_cache_key(&self) -> String {
        format!(
            "tasks:{}:{}:{}:{}:{}",
            self.workspace.as_ref().map(|w| w.0.as_str()).unwrap_or("all"),
            self.project.as_ref().map(|p| p.0.as_str()).unwrap_or("all"),
            self.assignee.as_ref().map(|a| a.0.as_str()).unwrap_or("all"),
            self.completed.map(|c| c.to_string()).unwrap_or_else(|| "all".to_string()),
            self.search_query.as_deref().unwrap_or(""),
        )
    }
}