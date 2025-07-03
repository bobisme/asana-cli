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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub description: Option<String>,
    pub completed: bool,
    pub due_date: Option<DateTime<Utc>>,
    pub assignee: Option<super::UserId>,
    pub assignee_name: Option<String>,
    pub projects: Vec<TaskProject>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub workspace: super::WorkspaceId,
    pub resource_type: Option<String>,
    pub resource_subtype: Option<String>,
    pub custom_fields: Vec<CustomField>,
    pub dependencies: Vec<TaskDependency>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomField {
    pub gid: String,
    pub name: String,
    pub display_value: Option<String>,
    pub text_value: Option<String>,
    pub number_value: Option<f64>,
    pub enum_value: Option<EnumValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnumValue {
    pub gid: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskDependency {
    pub gid: String,
    pub resource_type: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskProject {
    pub gid: String,
    pub name: String,
    pub color: Option<String>,
}

impl Task {
    /// Business rule: determine if task is overdue
    pub fn is_overdue(&self) -> bool {
        self.due_date
            .map(|due| due < Utc::now() && !self.completed)
            .unwrap_or(false)
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
                    d if d < 0 => {
                        let days = -d;
                        format!("{days} days ago")
                    }
                    d if d <= 7 => format!("In {d} days"),
                    _ => due.format("%Y-%m-%d").to_string(),
                }
            }
        }
    }

    /// Check if this task is a milestone
    pub fn is_milestone(&self) -> bool {
        self.resource_subtype.as_deref() == Some("milestone")
    }

    /// Get the appropriate icon for this task type
    pub fn type_icon(&self) -> &'static str {
        if self.is_milestone() {
            "◇" // Milestone
        } else {
            "○" // Task (open circle)
        }
    }

    /// Get the appropriate icon color based on due date
    pub fn icon_color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;

        if let Some(due_date) = self.due_date {
            let now = chrono::Utc::now();
            // Red if due before today (not including today)
            if due_date.date_naive() < now.date_naive() && !self.completed {
                Color::Red
            } else {
                Color::Green
            }
        } else {
            Color::Green
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
            self.workspace
                .as_ref()
                .map(|w| w.0.as_str())
                .unwrap_or("all"),
            self.project.as_ref().map(|p| p.0.as_str()).unwrap_or("all"),
            self.assignee
                .as_ref()
                .map(|a| a.0.as_str())
                .unwrap_or("all"),
            self.completed
                .map(|c| c.to_string())
                .unwrap_or_else(|| "all".to_string()),
            self.search_query.as_deref().unwrap_or(""),
        )
    }
}
