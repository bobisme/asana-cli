use std::collections::HashMap;

use nucleo_matcher::{
    pattern::{Normalization, Pattern},
    Matcher, Utf32Str,
};

use crate::domain::task::{Task, TaskId};

#[derive(Debug, Clone)]
pub struct TaskSearchItem {
    pub id: TaskId,
    title: String,
}

impl From<&Task> for TaskSearchItem {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id.clone(),
            title: task.name.clone(),
        }
    }
}

impl AsRef<str> for TaskSearchItem {
    fn as_ref(&self) -> &str {
        &self.title
    }
}

/// Performs fuzzy search on tasks using their titles
///
/// Returns a vector of TaskIds sorted by relevance (highest score first)
/// If query is empty, returns all task IDs
pub fn fuzzy_search_tasks(query: &str, tasks: &HashMap<TaskId, Task>) -> Vec<TaskId> {
    if query.is_empty() {
        return tasks.keys().cloned().collect();
    }

    // Create search items from tasks
    let search_items: Vec<TaskSearchItem> = tasks.values().map(TaskSearchItem::from).collect();

    // Setup matcher and pattern
    let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
    let pattern = Pattern::parse(
        query,
        nucleo_matcher::pattern::CaseMatching::Ignore,
        Normalization::Smart,
    );

    // Perform fuzzy matching
    let mut matches: Vec<(TaskSearchItem, u32)> = search_items
        .into_iter()
        .filter_map(|item| {
            let mut buf = Vec::new();
            let haystack = Utf32Str::new(item.as_ref(), &mut buf);
            let score = pattern.score(haystack, &mut matcher)?;
            Some((item, score))
        })
        .collect();

    // Sort by score (highest first)
    matches.sort_by(|a, b| b.1.cmp(&a.1));

    // Extract filtered task IDs
    matches.into_iter().map(|(item, _)| item.id).collect()
}

