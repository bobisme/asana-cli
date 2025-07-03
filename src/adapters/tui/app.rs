use color_eyre::Result;
use std::sync::Arc;
// Removed tui_markdown due to version compatibility issues
use super::{
    event::{AppEvent, EventHandler},
    md,
    widgets::SearchBar,
};
use crate::application::StateManager;
use crate::domain::{Comment, Task, TaskId};
use ratatui::{
    prelude::*,
    widgets::{
        Block, BorderType, Borders, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, TableState, Wrap,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Main, // Split layout: task list + details
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusedPane {
    Search,
    TaskList,    // Left pane
    Description, // Right top pane
    Comments,    // Right bottom pane
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskDetailPane {
    Description,
    Comments,
}

pub struct App {
    state_manager: Arc<StateManager>,

    // UI State
    mode: AppMode,
    focused_pane: FocusedPane,
    fullscreen_pane: Option<FocusedPane>,

    // Search
    search_bar: SearchBar,
    search_query: String,

    // Task list
    tasks: Vec<Task>,
    task_list_state: TableState,
    filtered_tasks: Vec<Task>,

    // Loading states
    is_loading: bool,
    error_message: Option<String>,

    // Comment input

    // Task detail
    current_task: Option<Task>,
    task_comments: Vec<Comment>,
    detail_scroll_offset: u16, // Legacy - will be replaced
    detail_loading: bool,

    // Task detail panes
    focused_detail_pane: TaskDetailPane,
    description_scroll_offset: u16,
    comments_scroll_offset: u16,
    needs_task_reload: bool,

    // Cached parsed content for performance
    cached_description_lines: Option<Vec<md::MarkdownLine>>,
    cached_comments_lines: Option<Vec<md::MarkdownLine>>,
}

impl App {
    /// Handle character input with search priority
    /// If search is focused, the character goes to search. Otherwise, return true to indicate action should be executed.
    fn handle_char_with_search_priority(&mut self, c: char) -> bool {
        if self.focused_pane == FocusedPane::Search {
            self.search_bar.insert_char(c);
            self.search_query = self.search_bar.query().to_string();
            self.update_filtered_tasks();
            false // Search handled the character
        } else {
            true // Execute the action instead
        }
    }

    /// Format comment lines with proper markdown parsing
    fn format_comment_lines(text: &str, max_width: Option<u16>) -> Vec<md::MarkdownLine> {
        let mut lines = Vec::new();

        // Convert markdown to lines first with intelligent wrapping
        let parsed_lines = md::parse_markdown_to_marked_lines_with_wrapping(text, max_width);

        for parsed_line in parsed_lines {
            // Skip empty lines from markdown parsing
            if parsed_line.line.spans.is_empty() {
                continue;
            }

            // Add the original spans but make them white if no color is set
            let mut new_spans = Vec::new();
            for span in parsed_line.line.spans {
                let mut new_style = span.style;
                if new_style.fg.is_none() {
                    new_style = new_style.fg(Color::White);
                }
                new_spans.push(Span::styled(span.content, new_style));
            }

            lines.push(md::MarkdownLine {
                line: Line::from(new_spans),
                is_code_block: parsed_line.is_code_block,
            });
        }

        lines
    }

    /// Format activity text with colors: text in white, timestamp in gray
    fn format_activity_line(activity_text: &str, timestamp: &str) -> Vec<Span<'static>> {
        vec![
            Span::styled(activity_text.to_string(), Style::default().fg(Color::White)),
            Span::raw(" • "),
            Span::styled(timestamp.to_string(), Style::default().fg(Color::Gray)),
        ]
    }

    pub fn new(state_manager: Arc<StateManager>) -> Self {
        let mut app = Self {
            state_manager,
            mode: AppMode::Main,
            focused_pane: FocusedPane::TaskList,
            fullscreen_pane: None,
            search_bar: SearchBar::new(),
            search_query: String::new(),
            tasks: Vec::new(),
            task_list_state: TableState::default(),
            filtered_tasks: Vec::new(),
            is_loading: false,
            error_message: None,
            current_task: None,
            task_comments: Vec::new(),
            detail_scroll_offset: 0,
            detail_loading: false,

            // Task detail panes
            focused_detail_pane: TaskDetailPane::Description,
            description_scroll_offset: 0,
            comments_scroll_offset: 0,
            needs_task_reload: false,

            // Cached parsed content
            cached_description_lines: None,
            cached_comments_lines: None,
        };

        // Select first task by default
        app.task_list_state.select(Some(0));
        app.search_bar.set_focused(false);
        app.needs_task_reload = true; // Load first task details on startup
        app
    }

    pub async fn initialize(&mut self) -> Result<()> {
        self.state_manager.initialize().await?;
        self.load_tasks().await?;
        Ok(())
    }

    async fn load_tasks(&mut self) -> Result<()> {
        self.is_loading = true;
        self.error_message = None;

        match self
            .state_manager
            .get_tasks_for_current_workspace(true)
            .await
        {
            Ok(tasks) => {
                self.tasks = tasks;
                self.update_filtered_tasks();

                // Reset selection to first item if we have tasks
                if !self.filtered_tasks.is_empty() {
                    self.task_list_state.select(Some(0));

                    // Preload comments for visible tasks (first 10)
                    let visible_tasks: Vec<TaskId> = self
                        .filtered_tasks
                        .iter()
                        .take(10)
                        .map(|t| t.id.clone())
                        .collect();

                    // Spawn background task to preload comments
                    let state_manager = self.state_manager.clone();
                    tokio::spawn(async move {
                        for task_id in visible_tasks {
                            let _ = state_manager.get_task_comments(&task_id).await;
                        }
                    });
                } else {
                    self.task_list_state.select(None);
                }
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to load tasks: {e}"));
            }
        }

        self.is_loading = false;
        Ok(())
    }

    fn update_filtered_tasks(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_tasks = self.tasks.clone();
        } else {
            let query_lower = self.search_query.to_lowercase();
            self.filtered_tasks = self
                .tasks
                .iter()
                .filter(|task| {
                    task.name.to_lowercase().contains(&query_lower)
                        || task
                            .description
                            .as_ref()
                            .is_some_and(|desc| desc.to_lowercase().contains(&query_lower))
                })
                .cloned()
                .collect();
        }

        // Adjust selection if needed
        if let Some(selected) = self.task_list_state.selected() {
            if selected >= self.filtered_tasks.len() {
                let new_selection = if self.filtered_tasks.is_empty() {
                    None
                } else {
                    Some(self.filtered_tasks.len() - 1)
                };
                self.task_list_state.select(new_selection);
            }
        }
    }

    async fn load_task_details(&mut self, task_id: &TaskId) -> Result<()> {
        self.detail_loading = true;
        self.detail_scroll_offset = 0;

        // Reset scroll offsets for both description and comments panes when switching tasks
        self.description_scroll_offset = 0;
        self.comments_scroll_offset = 0;

        // Clear cached content when switching tasks
        self.cached_description_lines = None;
        self.cached_comments_lines = None;

        // Load task details and comments in parallel
        let task_future = self.state_manager.get_task(task_id);
        let comments_future = self.state_manager.get_task_comments(task_id);

        let (task_result, comments_result) = tokio::join!(task_future, comments_future);

        match task_result {
            Ok(task) => self.current_task = Some(task),
            Err(e) => {
                self.error_message = Some(format!("Failed to load task: {e}"));
                self.current_task = None;
            }
        }

        match comments_result {
            Ok(comments) => self.task_comments = comments,
            Err(e) => {
                self.error_message = Some(format!("Failed to load comments: {e}"));
                self.task_comments = Vec::new();
            }
        }

        self.detail_loading = false;
        Ok(())
    }

    fn clamp_scroll_offset(&mut self) {
        if let Some(task) = &self.current_task {
            // Calculate content height for description pane
            let mut description_content_lines = 0u16;

            // Task info lines (status, due date, assignee)
            description_content_lines += 3; // Status, due date, blank line
            if task.assignee.is_some() {
                description_content_lines += 1;
            }

            // Projects line
            if !task.projects.is_empty() {
                description_content_lines += 1;
            }

            // Custom fields
            description_content_lines += task
                .custom_fields
                .iter()
                .filter(|cf| {
                    cf.display_value
                        .as_ref()
                        .map(|v| !v.is_empty())
                        .unwrap_or(false)
                })
                .count() as u16;

            // Dependencies line
            if !task.dependencies.is_empty() {
                description_content_lines += 1;
            }

            // Description content
            if let Some(desc) = &task.description {
                if !desc.trim().is_empty() {
                    let markdown_desc = md::html_to_markdown(desc);
                    let styled_lines =
                        md::parse_markdown_to_marked_lines_with_wrapping(&markdown_desc, Some(80));
                    description_content_lines += styled_lines.len() as u16 + 1; // +1 for header
                }
            } else {
                description_content_lines += 1; // "No description available"
            }

            // Clamp description scroll offset (assuming ~60% of screen height for description pane)
            let description_available_height = 15u16; // Rough estimate for description pane height
            let description_max_scroll =
                description_content_lines.saturating_sub(description_available_height);
            self.description_scroll_offset =
                self.description_scroll_offset.min(description_max_scroll);

            // Calculate content height for comments pane
            let mut comments_content_lines = 0u16;

            if self.task_comments.is_empty() {
                comments_content_lines += 1; // "No comments or activity"
            } else {
                // Separate comments from system activity
                let user_comments: Vec<_> = self
                    .task_comments
                    .iter()
                    .filter(|c| c.story_type.as_deref() == Some("comment"))
                    .collect();
                let system_activity: Vec<_> = self
                    .task_comments
                    .iter()
                    .filter(|c| c.story_type.as_deref() != Some("comment"))
                    .collect();

                if !user_comments.is_empty() {
                    comments_content_lines += 2; // "Comments" header + spacing
                    for comment in &user_comments {
                        comments_content_lines += 1; // Author line
                        if let Some(ref text) = comment.text {
                            comments_content_lines += text.lines().count() as u16;
                            // Text lines
                        }
                        comments_content_lines += 1; // Spacing
                    }
                }

                if !system_activity.is_empty() {
                    comments_content_lines += 2; // "Activity" header + spacing
                    for activity in &system_activity {
                        comments_content_lines += 1; // Author line
                        if let Some(ref text) = activity.text {
                            comments_content_lines += text.lines().count() as u16;
                            // Text lines
                        }
                        comments_content_lines += 1; // Spacing
                    }
                }
            }

            // Clamp comments scroll offset (assuming ~40% of screen height for comments pane)
            let comments_available_height = 10u16; // Rough estimate for comments pane height
            let comments_max_scroll =
                comments_content_lines.saturating_sub(comments_available_height);
            self.comments_scroll_offset = self.comments_scroll_offset.min(comments_max_scroll);
        }
    }

    pub async fn handle_event(&mut self, event: AppEvent) -> Result<bool> {
        match event {
            AppEvent::Quit => return Ok(true),

            AppEvent::FocusSearch => {
                self.focused_pane = FocusedPane::Search;
                self.search_bar.set_focused(true);
            }

            AppEvent::Tab => match self.focused_pane {
                FocusedPane::Search => {
                    self.focused_pane = FocusedPane::TaskList;
                    self.search_bar.set_focused(false);
                }
                FocusedPane::TaskList => {
                    self.focused_pane = FocusedPane::Description;
                }
                FocusedPane::Description => {
                    self.focused_pane = FocusedPane::Comments;
                }
                FocusedPane::Comments => {
                    self.focused_pane = FocusedPane::TaskList;
                }
            },

            AppEvent::BackTab => {
                // Same as Tab but in reverse
                match self.focused_pane {
                    FocusedPane::Search => {
                        self.focused_pane = FocusedPane::Comments;
                    }
                    FocusedPane::TaskList => {
                        self.focused_pane = FocusedPane::Comments;
                    }
                    FocusedPane::Description => {
                        self.focused_pane = FocusedPane::TaskList;
                    }
                    FocusedPane::Comments => {
                        self.focused_pane = FocusedPane::Description;
                    }
                }
            }

            AppEvent::Character(c) => {
                match c {
                    'q' => {
                        // Search takes priority - if search is focused, type the character
                        if self.focused_pane == FocusedPane::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            // Context-sensitive quit when not in search
                            match &self.mode {
                                AppMode::Help => {
                                    self.mode = AppMode::Main;
                                }
                                _ => {
                                    return Ok(true); // Quit app
                                }
                            }
                        }
                    }
                    'j' => {
                        if self.focused_pane == FocusedPane::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            if self.focused_pane == FocusedPane::Description {
                                self.description_scroll_offset =
                                    self.description_scroll_offset.saturating_add(1);
                                self.clamp_scroll_offset();
                            } else if self.focused_pane == FocusedPane::Comments {
                                self.comments_scroll_offset =
                                    self.comments_scroll_offset.saturating_add(1);
                                self.clamp_scroll_offset();
                            } else if self.focused_pane == FocusedPane::TaskList {
                                self.next_task();
                            }
                        }
                    }
                    'k' => {
                        if self.focused_pane == FocusedPane::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            if self.focused_pane == FocusedPane::Description {
                                self.description_scroll_offset =
                                    self.description_scroll_offset.saturating_sub(1);
                                self.clamp_scroll_offset();
                            } else if self.focused_pane == FocusedPane::Comments {
                                self.comments_scroll_offset =
                                    self.comments_scroll_offset.saturating_sub(1);
                                self.clamp_scroll_offset();
                            } else if self.focused_pane == FocusedPane::TaskList {
                                self.previous_task();
                            }
                        }
                    }
                    'g' => {
                        if self.focused_pane == FocusedPane::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            match &self.mode {
                                _ if self.focused_pane == FocusedPane::Description
                                    || self.focused_pane == FocusedPane::Comments =>
                                {
                                    self.detail_scroll_offset = 0;
                                }
                                _ => {
                                    if self.focused_pane == FocusedPane::TaskList
                                        && !self.filtered_tasks.is_empty()
                                    {
                                        self.task_list_state.select(Some(0));
                                    }
                                }
                            }
                        }
                    }
                    'G' => {
                        if self.focused_pane == FocusedPane::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            match &self.mode {
                                _ if self.focused_pane == FocusedPane::Description
                                    || self.focused_pane == FocusedPane::Comments =>
                                {
                                    self.detail_scroll_offset = u16::MAX;
                                    self.clamp_scroll_offset();
                                }
                                _ => {
                                    if self.focused_pane == FocusedPane::TaskList
                                        && !self.filtered_tasks.is_empty()
                                    {
                                        self.task_list_state
                                            .select(Some(self.filtered_tasks.len() - 1));
                                    }
                                }
                            }
                        }
                    }
                    'r' => {
                        if self.handle_char_with_search_priority(c) {
                            // Refresh when not in search
                            self.load_tasks().await?;
                        }
                    }
                    'c' => {
                        if self.handle_char_with_search_priority(c) {
                            // Start comment when not in search
                            // TODO: Implement comment functionality when needed
                        }
                    }
                    '?' => {
                        if self.handle_char_with_search_priority(c) {
                            // Show help when not in search
                            self.mode = AppMode::Help;
                        }
                    }
                    ' ' => {
                        if self.handle_char_with_search_priority(c) {
                            // Toggle task completion when not in search
                            if self.focused_pane == FocusedPane::TaskList {
                                if let Some(selected) = self.task_list_state.selected() {
                                    if let Some(task) = self.filtered_tasks.get(selected) {
                                        match self
                                            .state_manager
                                            .toggle_task_completion(&task.id)
                                            .await
                                        {
                                            Ok(_) => {
                                                self.load_tasks().await?;
                                            }
                                            Err(e) => {
                                                self.error_message =
                                                    Some(format!("Failed to toggle task: {e}"));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    'f' => {
                        if self.focused_pane == FocusedPane::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            // Toggle fullscreen for current focused pane
                            if self.fullscreen_pane == Some(self.focused_pane) {
                                // Exit fullscreen
                                self.fullscreen_pane = None;
                            } else {
                                // Enter fullscreen for current pane
                                self.fullscreen_pane = Some(self.focused_pane);
                            }
                        }
                    }
                    _ => {
                        // Regular character input for search
                        if self.focused_pane == FocusedPane::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        }
                    }
                }
            }

            AppEvent::Backspace => {
                if self.focused_pane == FocusedPane::Search {
                    self.search_bar.delete_char();
                    self.search_query = self.search_bar.query().to_string();
                    self.update_filtered_tasks();
                }
            }

            AppEvent::CloseModal => {
                if self.focused_pane == FocusedPane::Search {
                    // Esc from search: clear search and focus task list
                    self.search_bar.clear();
                    self.search_query.clear();
                    self.update_filtered_tasks();
                    self.focused_pane = FocusedPane::TaskList;
                    self.search_bar.set_focused(false);
                } else {
                    // Esc from other contexts: close modals/details
                    self.mode = AppMode::Main;
                }
            }

            AppEvent::NextTask => {
                // Arrow keys work in both search and task list for navigation
                if self.focused_pane == FocusedPane::Description {
                    self.description_scroll_offset =
                        self.description_scroll_offset.saturating_add(1);
                    self.clamp_scroll_offset();
                } else if self.focused_pane == FocusedPane::Comments {
                    self.comments_scroll_offset = self.comments_scroll_offset.saturating_add(1);
                    self.clamp_scroll_offset();
                } else if self.focused_pane == FocusedPane::TaskList
                    || self.focused_pane == FocusedPane::Search
                {
                    self.next_task();
                }
            }

            AppEvent::PreviousTask => {
                // Arrow keys work in both search and task list for navigation
                if self.focused_pane == FocusedPane::Description {
                    self.description_scroll_offset =
                        self.description_scroll_offset.saturating_sub(1);
                    self.clamp_scroll_offset();
                } else if self.focused_pane == FocusedPane::Comments {
                    self.comments_scroll_offset = self.comments_scroll_offset.saturating_sub(1);
                    self.clamp_scroll_offset();
                } else if self.focused_pane == FocusedPane::TaskList
                    || self.focused_pane == FocusedPane::Search
                {
                    self.previous_task();
                }
            }

            AppEvent::Enter => {
                if self.focused_pane == FocusedPane::Search {
                    // Enter from search: switch to task list and select highlighted task
                    self.focused_pane = FocusedPane::TaskList;
                    self.search_bar.set_focused(false);

                    // Switch focus to task list
                    self.focused_pane = FocusedPane::TaskList;
                } else if self.focused_pane == FocusedPane::TaskList {
                    // Focus the task details pane and load details for selected task
                    self.focused_pane = FocusedPane::Description;
                    if let Some(selected) = self.task_list_state.selected() {
                        if let Some(task) = self.filtered_tasks.get(selected) {
                            let task_id = task.id.clone();
                            self.load_task_details(&task_id).await?;
                        }
                    }
                }
            }

            AppEvent::ScrollDetailPageUp => {
                if self.focused_pane == FocusedPane::Description {
                    self.description_scroll_offset =
                        self.description_scroll_offset.saturating_sub(10);
                    self.clamp_scroll_offset();
                } else if self.focused_pane == FocusedPane::Comments {
                    self.comments_scroll_offset = self.comments_scroll_offset.saturating_sub(10);
                    self.clamp_scroll_offset();
                }
            }

            AppEvent::ScrollDetailPageDown => {
                if self.focused_pane == FocusedPane::Description {
                    self.description_scroll_offset =
                        self.description_scroll_offset.saturating_add(10);
                    self.clamp_scroll_offset();
                } else if self.focused_pane == FocusedPane::Comments {
                    self.comments_scroll_offset = self.comments_scroll_offset.saturating_add(10);
                    self.clamp_scroll_offset();
                }
            }
        }

        Ok(false)
    }

    fn next_task(&mut self) {
        if self.filtered_tasks.is_empty() {
            return;
        }

        let current = self.task_list_state.selected().unwrap_or(0);
        let next = if current >= self.filtered_tasks.len() - 1 {
            0
        } else {
            current + 1
        };
        self.task_list_state.select(Some(next));
        self.needs_task_reload = true;
    }

    fn previous_task(&mut self) {
        if self.filtered_tasks.is_empty() {
            return;
        }

        let current = self.task_list_state.selected().unwrap_or(0);
        let previous = if current == 0 {
            self.filtered_tasks.len() - 1
        } else {
            current - 1
        };
        self.task_list_state.select(Some(previous));
        self.needs_task_reload = true;
    }

    pub fn render(&mut self, frame: &mut Frame) {
        // Check if we're in fullscreen mode
        if let Some(fullscreen_pane) = self.fullscreen_pane {
            // Render fullscreen pane without borders, using entire frame area
            match fullscreen_pane {
                FocusedPane::Search => {
                    // For search fullscreen, still show search bar at top
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(3), // Search bar
                            Constraint::Min(0),    // Rest for search results or empty
                        ])
                        .split(frame.area());

                    self.search_bar.render(frame, chunks[0]);
                    // Could render search results or help text in chunks[1] if needed
                }
                FocusedPane::TaskList => {
                    self.render_task_list_fullscreen(frame, frame.area());
                }
                FocusedPane::Description => {
                    self.render_description_fullscreen(frame, frame.area());
                }
                FocusedPane::Comments => {
                    self.render_comments_fullscreen(frame, frame.area());
                }
            }
        } else {
            // Normal 3-pane layout
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Search bar
                    Constraint::Min(0),    // Main content (split left/right)
                    Constraint::Length(1), // Status bar
                ])
                .split(frame.area());

            // Render search bar
            self.search_bar.render(frame, main_chunks[0]);

            // Split main content area: task list (left) | right side
            let content_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(40), // Task list (left pane)
                    Constraint::Percentage(60), // Right side (description + comments)
                ])
                .split(main_chunks[1]);

            // Render task list (left pane)
            self.render_task_list(frame, content_chunks[0]);

            // Split right side vertically: description (top) | comments (bottom)
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(50), // Description pane
                    Constraint::Percentage(50), // Comments pane
                ])
                .split(content_chunks[1]);

            // Render description pane (right top)
            self.render_description_pane_standalone(frame, right_chunks[0]);

            // Render comments pane (right bottom)
            self.render_comments_pane_standalone(frame, right_chunks[1]);

            // Render status bar
            self.render_status_bar(frame, main_chunks[2]);

            // Render help modal if active
            if matches!(self.mode, AppMode::Help) {
                self.render_help(frame);
            }
        }
    }

    // Fullscreen render methods (without borders)
    fn render_task_list_fullscreen(&mut self, frame: &mut Frame, area: Rect) {
        if self.is_loading {
            let paragraph =
                Paragraph::new("Loading tasks...").style(Style::default().fg(Color::Gray));
            frame.render_widget(paragraph, area);
            return;
        }

        if let Some(error) = &self.error_message {
            let paragraph = Paragraph::new(error.as_str()).style(Style::default().fg(Color::Red));
            frame.render_widget(paragraph, area);
            return;
        }

        if self.filtered_tasks.is_empty() {
            let paragraph =
                Paragraph::new("No tasks found").style(Style::default().fg(Color::Gray));
            frame.render_widget(paragraph, area);
            return;
        }

        let header_cells = ["Name", "Status", "Due Date"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow)));
        let header = Row::new(header_cells).height(1);

        let rows: Vec<Row> = self
            .filtered_tasks
            .iter()
            .map(|task| {
                let (status_text, _) = task.status_display();
                let due_text = task.due_date_display();
                // Get icon and color based on task type and due date
                let icon = task.type_icon();
                let icon_color = task.icon_color();
                let icon_span = Span::styled(icon, Style::default().fg(icon_color));
                let title_with_icon = vec![icon_span, Span::raw(" "), Span::raw(&task.name)];

                // Make due dates dark gray
                let due_style = Style::default().fg(Color::DarkGray);

                Row::new([
                    Cell::from(Line::from(title_with_icon)),
                    Cell::from(status_text),
                    Cell::from(due_text).style(due_style),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Percentage(50),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ],
        )
        .header(header)
        .row_highlight_style(Style::default().bg(Color::Blue))
        .highlight_symbol(">> ");

        frame.render_stateful_widget(table, area, &mut self.task_list_state);
    }

    fn render_description_fullscreen(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(task) = self.current_task.clone() {
            self.render_description_content_only(frame, area, &task);
        } else if let Some(selected_index) = self.task_list_state.selected() {
            if let Some(task) = self.filtered_tasks.get(selected_index).cloned() {
                self.render_description_content_only(frame, area, &task);
            } else {
                let paragraph =
                    Paragraph::new("No task selected").style(Style::default().fg(Color::Gray));
                frame.render_widget(paragraph, area);
            }
        } else {
            let paragraph =
                Paragraph::new("No task selected").style(Style::default().fg(Color::Gray));
            frame.render_widget(paragraph, area);
        }
    }

    fn render_comments_fullscreen(&mut self, frame: &mut Frame, area: Rect) {
        self.render_comments_content_only(frame, area);
    }

    /// Auto-load task details when selection changes
    pub async fn auto_load_selected_task(&mut self) -> Result<()> {
        if !self.needs_task_reload {
            return Ok(());
        }

        self.needs_task_reload = false;

        if let Some(selected) = self.task_list_state.selected() {
            if let Some(task) = self.filtered_tasks.get(selected) {
                let task_id = task.id.clone();

                // Only reload if it's a different task
                let needs_loading = self
                    .current_task
                    .as_ref()
                    .map(|current| current.id != task_id)
                    .unwrap_or(true);

                if needs_loading {
                    self.load_task_details(&task_id).await?;
                }
            }
        }

        Ok(())
    }

    fn render_description_pane_standalone(&mut self, frame: &mut Frame, area: Rect) {
        // Determine border style based on focus
        let border_style = if self.focused_pane == FocusedPane::Description {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Gray)
        };

        let title = "Description";

        // Get currently selected task
        let selected_task = self
            .task_list_state
            .selected()
            .and_then(|i| self.filtered_tasks.get(i));

        if let Some(task) = selected_task {
            if let Some(current_task) = self.current_task.clone() {
                if current_task.id == task.id {
                    // Show task description using existing render logic
                    let block = Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(border_style);

                    let inner_area = block.inner(area);
                    frame.render_widget(block, area);

                    // Render description content
                    self.render_description_content_only(frame, inner_area, &current_task);
                } else {
                    // Loading different task
                    self.render_loading_placeholder(frame, area, title, border_style);
                }
            } else {
                // No task loaded
                self.render_loading_placeholder(frame, area, title, border_style);
            }
        } else {
            // No task selected
            self.render_empty_placeholder(frame, area, title, border_style, "No task selected");
        }
    }

    fn render_comments_pane_standalone(&mut self, frame: &mut Frame, area: Rect) {
        // Determine border style based on focus
        let border_style = if self.focused_pane == FocusedPane::Comments {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Gray)
        };

        let title = "Comments & Activity";

        // Get currently selected task
        let selected_task = self
            .task_list_state
            .selected()
            .and_then(|i| self.filtered_tasks.get(i));

        if let Some(task) = selected_task {
            if let Some(current_task) = &self.current_task {
                if current_task.id == task.id {
                    // Show comments using existing render logic
                    let block = Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(border_style);

                    let inner_area = block.inner(area);
                    frame.render_widget(block, area);

                    // Render comments content
                    self.render_comments_content_only(frame, inner_area);
                } else {
                    // Loading different task
                    self.render_loading_placeholder(frame, area, title, border_style);
                }
            } else {
                // No task loaded
                self.render_loading_placeholder(frame, area, title, border_style);
            }
        } else {
            // No task selected
            self.render_empty_placeholder(frame, area, title, border_style, "No task selected");
        }
    }

    fn render_loading_placeholder(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        border_style: Style,
    ) {
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        let paragraph = Paragraph::new("Loading...")
            .block(block)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }

    fn render_empty_placeholder(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        border_style: Style,
        message: &str,
    ) {
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        let paragraph = Paragraph::new(message)
            .block(block)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
    }

    fn render_description_content_only(&mut self, frame: &mut Frame, area: Rect, task: &Task) {
        // Check if we need to regenerate the cache
        if self.cached_description_lines.is_none() {
            // Generate and cache the lines
            let mut lines: Vec<md::MarkdownLine> = Vec::new();

            // Add task info section
            let (status_text, status_color) = task.status_display();
            let status_style = match status_color {
                "red" => Style::default().fg(Color::Red),
                "yellow" => Style::default().fg(Color::Yellow),
                "green" => Style::default().fg(Color::Green),
                "gray" => Style::default().fg(Color::Gray),
                _ => Style::default(),
            };

            lines.push(md::MarkdownLine {
                line: Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(Color::Cyan)),
                    Span::styled(status_text, status_style),
                ]),
                is_code_block: false,
            });

            let due_text = task.due_date_display();
            let due_style = if task.is_overdue() {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            lines.push(md::MarkdownLine {
                line: Line::from(vec![
                    Span::styled("Due: ", Style::default().fg(Color::Cyan)),
                    Span::styled(due_text, due_style),
                ]),
                is_code_block: false,
            });

            if task.assignee.is_some() {
                let assignee_display = task.assignee_name.as_deref().unwrap_or("Unknown User");
                lines.push(md::MarkdownLine {
                    line: Line::from(vec![
                        Span::styled("Assignee: ", Style::default().fg(Color::Cyan)),
                        Span::raw(assignee_display.to_string()),
                    ]),
                    is_code_block: false,
                });
            }

            // Add projects with colors
            if !task.projects.is_empty() {
                let mut project_spans =
                    vec![Span::styled("Projects: ", Style::default().fg(Color::Cyan))];
                for (i, project) in task.projects.iter().enumerate() {
                    if i > 0 {
                        project_spans.push(Span::raw(", "));
                    }

                    // Use project color if available
                    let project_style = if let Some(color) = &project.color {
                        Style::default().fg(asana_color_to_ratatui(color))
                    } else {
                        Style::default()
                    };

                    project_spans.push(Span::styled(project.name.clone(), project_style));
                }
                lines.push(md::MarkdownLine {
                    line: Line::from(project_spans),
                    is_code_block: false,
                });
            }

            // Add custom fields
            if !task.custom_fields.is_empty() {
                for custom_field in &task.custom_fields {
                    if let Some(display_value) = &custom_field.display_value {
                        if !display_value.is_empty() {
                            let mut field_spans = vec![Span::styled(
                                format!("{}: ", custom_field.name),
                                Style::default().fg(Color::Cyan),
                            )];

                            // Check if it's an enum value with color
                            if let Some(enum_value) = &custom_field.enum_value {
                                if let Some(color) = &enum_value.color {
                                    let value_style =
                                        Style::default().fg(asana_color_to_ratatui(color));
                                    field_spans
                                        .push(Span::styled(display_value.clone(), value_style));
                                } else {
                                    field_spans.push(Span::raw(display_value.clone()));
                                }
                            } else {
                                field_spans.push(Span::raw(display_value.clone()));
                            }

                            lines.push(md::MarkdownLine {
                                line: Line::from(field_spans),
                                is_code_block: false,
                            });
                        }
                    }
                }
            }

            // Add dependencies
            if !task.dependencies.is_empty() {
                let mut dep_spans = vec![Span::styled(
                    "Dependencies: ",
                    Style::default().fg(Color::Cyan),
                )];
                for (i, dependency) in task.dependencies.iter().enumerate() {
                    if i > 0 {
                        dep_spans.push(Span::raw(", "));
                    }
                    dep_spans.push(Span::raw(dependency.name.clone()));
                }
                lines.push(md::MarkdownLine {
                    line: Line::from(dep_spans),
                    is_code_block: false,
                });
            }

            // Add blank line separator
            lines.push(md::MarkdownLine {
                line: Line::from(""),
                is_code_block: false,
            });

            // Add description if present
            if let Some(description) = &task.description {
                if !description.trim().is_empty() {
                    let markdown_desc = md::html_to_markdown(description);
                    let styled_lines = md::parse_markdown_to_marked_lines_with_wrapping(
                        &markdown_desc,
                        Some(area.width),
                    );

                    lines.extend(styled_lines);
                }
            } else {
                lines.push(md::MarkdownLine {
                    line: Line::from(vec![Span::styled(
                        "No description available",
                        Style::default().fg(Color::Gray),
                    )]),
                    is_code_block: false,
                });
            }

            // Cache the generated lines
            self.cached_description_lines = Some(lines);
        }

        // Use cached lines for rendering
        if let Some(cached_lines) = &self.cached_description_lines {
            // Apply scrolling - skip lines based on scroll offset
            let visible_lines: Vec<&md::MarkdownLine> = cached_lines
                .iter()
                .skip(self.description_scroll_offset as usize)
                .collect();

            // Render lines with special handling for code blocks
            let mut y = 0;
            for marked_line in visible_lines {
                if y >= area.height {
                    break;
                }

                // Create a sub-area for this line
                let line_area = Rect {
                    x: area.x,
                    y: area.y + y,
                    width: area.width,
                    height: 1,
                };

                if marked_line.is_code_block {
                    // Render code blocks without wrapping
                    let paragraph = Paragraph::new(marked_line.line.clone())
                        .alignment(ratatui::layout::Alignment::Left);
                    frame.render_widget(paragraph, line_area);
                } else {
                    // Render regular lines with wrapping
                    let paragraph = Paragraph::new(marked_line.line.clone())
                        .wrap(Wrap { trim: false })
                        .alignment(ratatui::layout::Alignment::Left);
                    frame.render_widget(paragraph, line_area);
                }

                y += 1;
            }
        }
    }

    fn render_comments_content_only(&mut self, frame: &mut Frame, area: Rect) {
        // Check if we need to regenerate the cache
        if self.cached_comments_lines.is_none() {
            // Generate and cache the lines
            let mut lines: Vec<md::MarkdownLine> = Vec::new();

            // Comments and activity
            if self.task_comments.is_empty() {
                lines.push(md::MarkdownLine {
                    line: Line::from(vec![Span::styled(
                        "No comments or activity",
                        Style::default().fg(Color::Gray),
                    )]),
                    is_code_block: false,
                });
            } else {
                // Separate comments from system activity
                let mut user_comments = Vec::new();
                let mut system_activity = Vec::new();

                for comment in &self.task_comments {
                    match comment.story_type.as_deref() {
                        Some("comment") => user_comments.push(comment),
                        Some("system") => system_activity.push(comment),
                        _ => system_activity.push(comment), // Default to system if unclear
                    }
                }

                // Render user comments first
                if !user_comments.is_empty() {
                    lines.push(md::MarkdownLine {
                        line: Line::from(vec![Span::styled(
                            "Comments",
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        )]),
                        is_code_block: false,
                    });
                    lines.push(md::MarkdownLine {
                        line: Line::from(""),
                        is_code_block: false,
                    });

                    for comment in &user_comments {
                        let author_name = comment
                            .author
                            .as_ref()
                            .map(|u| u.name.clone())
                            .unwrap_or_else(|| "Unknown".to_string());
                        let time_display = comment.created_at.format("%Y-%m-%d %H:%M").to_string();

                        // Header: Name • timestamp
                        lines.push(md::MarkdownLine {
                            line: Line::from(vec![
                                Span::styled(author_name, Style::default().fg(Color::Blue)),
                                Span::styled(
                                    format!(" • {}", time_display),
                                    Style::default().fg(Color::Gray),
                                ),
                            ]),
                            is_code_block: false,
                        });

                        if let Some(ref text) = comment.text {
                            let cleaned_text = md::html_to_markdown(text);
                            let comment_lines = Self::format_comment_lines(&cleaned_text, None);
                            lines.extend(comment_lines);
                        }
                        lines.push(md::MarkdownLine {
                            line: Line::from(""),
                            is_code_block: false,
                        });
                    }
                }

                // Render system activity
                if !system_activity.is_empty() {
                    lines.push(md::MarkdownLine {
                        line: Line::from(vec![Span::styled(
                            "Activity",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        )]),
                        is_code_block: false,
                    });
                    lines.push(md::MarkdownLine {
                        line: Line::from(""),
                        is_code_block: false,
                    });

                    for activity in &system_activity {
                        let time_display = activity.created_at.format("%Y-%m-%d %H:%M").to_string();
                        let cleaned_text = activity
                            .text
                            .as_ref()
                            .map(|text| md::html_to_markdown(text))
                            .unwrap_or_else(|| "[No text content]".to_string());

                        let mut activity_spans =
                            vec![Span::styled("• ", Style::default().fg(Color::Blue))];
                        activity_spans
                            .extend(Self::format_activity_line(&cleaned_text, &time_display));

                        lines.push(md::MarkdownLine {
                            line: Line::from(activity_spans),
                            is_code_block: false,
                        });
                    }
                }
            }

            // Cache the generated lines
            self.cached_comments_lines = Some(lines);
        }

        // Use cached lines for rendering
        if let Some(cached_lines) = &self.cached_comments_lines {
            // Apply scrolling - skip lines based on scroll offset
            let visible_lines: Vec<Line> = cached_lines
                .iter()
                .skip(self.comments_scroll_offset as usize)
                .map(|ml| ml.line.clone())
                .collect();

            let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
            frame.render_widget(paragraph, area);
        }
    }

    fn render_task_list(&mut self, frame: &mut Frame, area: Rect) {
        let len = self.filtered_tasks.len();
        let title = format!("Tasks ({len})");
        let border_style = if self.focused_pane == FocusedPane::TaskList {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Gray)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        if self.is_loading {
            let paragraph = Paragraph::new("Loading tasks...")
                .block(block)
                .style(Style::default().fg(Color::Gray));
            frame.render_widget(paragraph, area);
            return;
        }

        if let Some(error) = &self.error_message {
            let paragraph = Paragraph::new(error.as_str())
                .block(block)
                .style(Style::default().fg(Color::Red));
            frame.render_widget(paragraph, area);
            return;
        }

        if self.filtered_tasks.is_empty() {
            let message = if self.search_query.is_empty() {
                "No tasks found"
            } else {
                "No tasks match your search"
            };
            let paragraph = Paragraph::new(message)
                .block(block)
                .style(Style::default().fg(Color::Gray));
            frame.render_widget(paragraph, area);
            return;
        }

        let rows: Vec<Row> = self
            .filtered_tasks
            .iter()
            .map(|task| {
                let due_text = task.due_date_display();

                // Get icon and color based on task type and due date
                let icon = task.type_icon();
                let icon_color = task.icon_color();
                let icon_span = Span::styled(icon, Style::default().fg(icon_color));
                let title_with_icon = vec![icon_span, Span::raw(" "), Span::raw(&task.name)];

                // Make due dates dark gray
                let due_style = Style::default().fg(Color::DarkGray);

                Row::new(vec![
                    Cell::from(Line::from(title_with_icon)),
                    Cell::from(due_text).style(due_style),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            &[
                Constraint::Min(20),    // Title column (flexible)
                Constraint::Length(12), // Due date column
            ],
        )
        .block(block)
        .row_highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("");

        frame.render_stateful_widget(table, area, &mut self.task_list_state);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let help_text = match self.focused_pane {
            FocusedPane::Search => "Tab: switch to tasks | Enter: go to tasks | /: focus search | f: fullscreen | q: quit | ?: help",
            FocusedPane::TaskList => "j/k: navigate | Tab: switch panes | Space: toggle complete | /: search | f: fullscreen | q: quit | ?: help",
            FocusedPane::Description => "j/k: scroll | Tab: next pane | f: fullscreen | q: quit | ?: help",
            FocusedPane::Comments => "j/k: scroll | Tab: next pane | f: fullscreen | q: quit | ?: help",
        };

        let paragraph = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(paragraph, area);
    }

    fn render_task_details_pane(&mut self, frame: &mut Frame, area: Rect) {
        // Determine border style based on focus
        let border_style = if self.focused_pane == FocusedPane::Description
            || self.focused_pane == FocusedPane::Comments
        {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Gray)
        };

        // Get currently selected task
        let selected_task = self
            .task_list_state
            .selected()
            .and_then(|i| self.filtered_tasks.get(i));

        if let Some(task) = selected_task {
            // Auto-load task details if not already loaded or if different task
            let needs_loading = self
                .current_task
                .as_ref()
                .map(|current| current.id != task.id)
                .unwrap_or(true);

            if needs_loading && !self.detail_loading {
                // Trigger async loading - we'll handle this in event processing
                // For now, show loading state
            }

            if self.detail_loading {
                // Show loading state
                let block = Block::default()
                    .title("Task Details")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(border_style);

                let paragraph = Paragraph::new("Loading task details...")
                    .block(block)
                    .style(Style::default().fg(Color::Gray));
                frame.render_widget(paragraph, area);
            } else if let Some(current_task) = &self.current_task {
                // Show task details using existing render logic
                let block = Block::default()
                    .title(format!("Task Details - {}", current_task.name))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(border_style);

                let inner_area = block.inner(area);
                frame.render_widget(block, area);

                // Use existing scrollable content renderer
                self.render_scrollable_content(frame, inner_area, current_task);
            } else {
                // Task selected but details not loaded yet
                let block = Block::default()
                    .title("Task Details")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(border_style);

                let paragraph = Paragraph::new("Select a task to view details")
                    .block(block)
                    .style(Style::default().fg(Color::Gray));
                frame.render_widget(paragraph, area);
            }
        } else {
            // No task selected - show placeholder
            let block = Block::default()
                .title("Task Details")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style);

            let paragraph = Paragraph::new("No task selected\n\nUse ↑↓ to select a task")
                .block(block)
                .style(Style::default().fg(Color::Gray))
                .alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
        }
    }

    fn render_help(&self, frame: &mut Frame) {
        let popup_area = Self::centered_rect(60, 70, frame.area());

        frame.render_widget(ratatui::widgets::Clear, popup_area);

        let help_text = vec![
            "Asana TUI Help",
            "",
            "Navigation:",
            "  j/k or ↑/↓     - Move up/down in task list / scroll in task detail",
            "  g/G            - Go to first/last task / top/bottom in task detail",
            "  Tab/Shift+Tab  - Switch between search and task list",
            "  Enter          - Open task detail view",
            "",
            "Task Detail:",
            "  j/k or ↑/↓     - Scroll up/down",
            "  g/G            - Go to top/bottom",
            "  Page Up/Down   - Fast scroll",
            "  q or Esc       - Close task detail",
            "",
            "Task Actions:",
            "  Space          - Toggle task completion",
            "  r              - Refresh task list",
            "",
            "Search:",
            "  /              - Focus search bar",
            "  Esc            - Clear search",
            "  Note: Type normally in search (j/k/g work as regular letters)",
            "",
            "General:",
            "  ?              - Show this help",
            "  q              - Context-sensitive quit/close",
            "  Ctrl+C         - Force quit application",
            "  Esc            - Close modals/cancel actions",
            "",
            "Press any key to close this help",
        ]
        .join("\n");

        let paragraph = Paragraph::new(help_text)
            .block(Block::default().title("Help").borders(Borders::ALL))
            .wrap(ratatui::widgets::Wrap { trim: false });

        frame.render_widget(paragraph, popup_area);
    }

    fn render_task_detail(&self, frame: &mut Frame, _task_id: &TaskId) {
        let popup_area = Self::centered_rect(70, 85, frame.area());

        frame.render_widget(ratatui::widgets::Clear, popup_area);

        if self.detail_loading {
            let paragraph = Paragraph::new("Loading task details...")
                .block(Block::default().title("Task Detail").borders(Borders::ALL))
                .style(Style::default().fg(Color::Gray));
            frame.render_widget(paragraph, popup_area);
            return;
        }

        let Some(task) = &self.current_task else {
            let paragraph = Paragraph::new("Task not found\n\nPress q or Esc to close")
                .block(Block::default().title("Task Detail").borders(Borders::ALL))
                .style(Style::default().fg(Color::Red));
            frame.render_widget(paragraph, popup_area);
            return;
        };

        // Calculate dynamic title height based on text wrapping
        let available_width = popup_area.width.saturating_sub(4) as usize; // Account for margins and borders
        let title_height = if available_width > 0 {
            // Simple word-aware wrapping calculation
            let words: Vec<&str> = task.name.split_whitespace().collect();
            let mut lines = 1u16;
            let mut current_line_len = 0;

            for word in words {
                let word_len = word.chars().count();
                if current_line_len + word_len + 1 > available_width && current_line_len > 0 {
                    lines += 1;
                    current_line_len = word_len;
                } else {
                    current_line_len += if current_line_len > 0 {
                        word_len + 1
                    } else {
                        word_len
                    };
                }
            }
            lines.min(3) // Cap at 3 lines max
        } else {
            1
        };

        // Split the modal into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(title_height), // Task title (dynamic height)
                Constraint::Min(10),              // All scrollable content (gets most space)
                Constraint::Length(1),            // Instructions
            ])
            .margin(1)
            .split(popup_area);

        // Render task title at top in bold with wrapping
        let title_paragraph = Paragraph::new(task.name.clone())
            .style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });
        frame.render_widget(title_paragraph, chunks[0]);

        // Render all content as one scrollable section
        self.render_scrollable_content(frame, chunks[1], task);

        // Render instructions
        let instructions = Paragraph::new(
            "↑↓: scroll | q/Esc: close | g/G: top/bottom | Page Up/Down: fast scroll",
        )
        .style(Style::default().fg(Color::Gray));
        frame.render_widget(instructions, chunks[2]);

        // Render border with title including task ID
        let border = Block::default()
            .title(format!("Task Detail - {}", task.id))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Yellow));
        frame.render_widget(border, popup_area);
    }

    fn render_scrollable_content(&self, frame: &mut Frame, area: Rect, task: &Task) {
        // Split into two panes: Description pane (top) and Comments pane (bottom)
        let panes = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(60), // Description pane (task info + description)
                Constraint::Percentage(40), // Comments pane
            ])
            .split(area);

        // Render description pane (top)
        self.render_description_pane(frame, panes[0], task);

        // Render comments pane (bottom)
        self.render_comments_pane(frame, panes[1]);
    }

    fn render_description_pane(&self, frame: &mut Frame, area: Rect, task: &Task) {
        // Create border with focus indicator
        let border_style = if self.focused_detail_pane == TaskDetailPane::Description {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let title = if self.focused_detail_pane == TaskDetailPane::Description {
            "Description [FOCUSED] (Tab to switch)"
        } else {
            "Description (Tab to switch)"
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Collect all content for this pane
        let mut lines = Vec::new();

        // Add task info section
        let (status_text, status_color) = task.status_display();
        let status_style = match status_color {
            "red" => Style::default().fg(Color::Red),
            "yellow" => Style::default().fg(Color::Yellow),
            "green" => Style::default().fg(Color::Green),
            "gray" => Style::default().fg(Color::Gray),
            _ => Style::default(),
        };

        lines.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Cyan)),
            Span::styled(status_text, status_style),
        ]));

        let due_text = task.due_date_display();
        let due_style = if task.is_overdue() {
            Style::default().fg(Color::Red)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled("Due: ", Style::default().fg(Color::Cyan)),
            Span::styled(due_text, due_style),
        ]));

        if task.assignee.is_some() {
            let assignee_display = task.assignee_name.as_deref().unwrap_or("Unknown User");
            lines.push(Line::from(vec![
                Span::styled("Assignee: ", Style::default().fg(Color::Cyan)),
                Span::raw(assignee_display.to_string()),
            ]));
        }

        // Add blank line separator
        lines.push(Line::from(""));

        // Add description if present
        if let Some(description) = &task.description {
            if !description.trim().is_empty() {
                let markdown_desc = md::html_to_markdown(description);
                let styled_lines = md::parse_markdown_to_lines(&markdown_desc);

                lines.extend(styled_lines);
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                "No description available",
                Style::default().fg(Color::Gray),
            )]));
        }

        // Apply scrolling - skip lines based on scroll offset
        let visible_lines: Vec<Line> = lines
            .into_iter()
            .skip(self.description_scroll_offset as usize)
            .collect();

        let paragraph = Paragraph::new(visible_lines)
            .wrap(Wrap { trim: false })
            .alignment(ratatui::layout::Alignment::Left);
        frame.render_widget(paragraph, inner_area);
    }

    fn render_comments_pane(&self, frame: &mut Frame, area: Rect) {
        // Create border with focus indicator
        let border_style = if self.focused_detail_pane == TaskDetailPane::Comments {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let title = if self.focused_detail_pane == TaskDetailPane::Comments {
            "Comments & Activity [FOCUSED] (Tab to switch)"
        } else {
            "Comments & Activity (Tab to switch)"
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Collect all comments content
        let mut lines = Vec::new();

        // Comments and activity
        if self.task_comments.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "No comments or activity",
                Style::default().fg(Color::Gray),
            )]));
        } else {
            // Separate comments from system activity
            let mut user_comments = Vec::new();
            let mut system_activity = Vec::new();

            for comment in &self.task_comments {
                match comment.story_type.as_deref() {
                    Some("comment") => user_comments.push(comment),
                    Some("system") => system_activity.push(comment),
                    _ => system_activity.push(comment), // Default to system if unclear
                }
            }

            // Render user comments first
            if !user_comments.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    "Comments",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));

                for comment in &user_comments {
                    let author_name = comment
                        .author
                        .as_ref()
                        .map(|u| u.name.as_str())
                        .unwrap_or("Unknown");
                    let time_display = comment.created_at.format("%Y-%m-%d %H:%M").to_string();

                    // Header: Name • timestamp
                    lines.push(Line::from(vec![
                        Span::styled(author_name, Style::default().fg(Color::Blue)),
                        Span::styled(
                            format!(" • {}", time_display),
                            Style::default().fg(Color::Gray),
                        ),
                    ]));

                    if let Some(ref text) = comment.text {
                        let cleaned_text = md::html_to_markdown(text);
                        let comment_lines = Self::format_comment_lines(&cleaned_text, None);
                        lines.extend(comment_lines.into_iter().map(|ml| ml.line));
                    }
                    lines.push(Line::from(""));
                }
            }

            // Render system activity
            if !system_activity.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    "Activity",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));

                for activity in &system_activity {
                    let time_display = activity.created_at.format("%Y-%m-%d %H:%M").to_string();
                    let cleaned_text = activity
                        .text
                        .as_ref()
                        .map(|text| md::html_to_markdown(text))
                        .unwrap_or_else(|| "[No text content]".to_string());

                    let mut activity_spans =
                        vec![Span::styled("• ", Style::default().fg(Color::Blue))];
                    activity_spans.extend(Self::format_activity_line(&cleaned_text, &time_display));

                    lines.push(Line::from(activity_spans));
                }
            }
        }

        // Apply scrolling - skip lines based on scroll offset
        let visible_lines: Vec<Line> = lines
            .into_iter()
            .skip(self.comments_scroll_offset as usize)
            .collect();

        let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner_area);
    }

    fn render_task_info_section(&self, frame: &mut Frame, area: Rect, task: &Task) {
        let mut lines = Vec::new();

        // Task info section
        let (status_text, status_color) = task.status_display();
        let status_style = match status_color {
            "red" => Style::default().fg(Color::Red),
            "yellow" => Style::default().fg(Color::Yellow),
            "green" => Style::default().fg(Color::Green),
            "gray" => Style::default().fg(Color::Gray),
            _ => Style::default(),
        };

        lines.push(Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Cyan)),
            Span::styled(status_text, status_style),
        ]));

        // Due date with red color if overdue
        let due_text = task.due_date_display();
        let due_style = if task.is_overdue() {
            Style::default().fg(Color::Red)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled("Due: ", Style::default().fg(Color::Cyan)),
            Span::styled(due_text, due_style),
        ]));

        // Assignee
        if task.assignee.is_some() {
            let assignee_display = task.assignee_name.as_deref().unwrap_or("Unknown User");
            lines.push(Line::from(vec![
                Span::styled("Assignee: ", Style::default().fg(Color::Cyan)),
                Span::raw(assignee_display.to_string()),
            ]));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    fn render_description_section(&self, frame: &mut Frame, area: Rect, task: &Task) {
        if let Some(description) = &task.description {
            if !description.trim().is_empty() {
                let markdown_desc = md::html_to_markdown(description);

                // Parse and render markdown with custom styling
                let styled_lines = md::parse_markdown_to_lines(&markdown_desc);

                let paragraph = Paragraph::new(styled_lines).wrap(Wrap { trim: false });

                frame.render_widget(paragraph, area);
            }
        }
    }

    fn render_comments_section(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();

        // Comments and activity
        if self.task_comments.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "No comments or activity",
                Style::default().fg(Color::Gray),
            )]));
        } else {
            // Separate comments from system activity
            let mut user_comments = Vec::new();
            let mut system_activity = Vec::new();

            for comment in &self.task_comments {
                match comment.story_type.as_deref() {
                    Some("comment") => user_comments.push(comment),
                    Some("system") => system_activity.push(comment),
                    _ => system_activity.push(comment), // Default to system if unclear
                }
            }

            // Render user comments first
            if !user_comments.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    "Comments",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));

                for comment in &user_comments {
                    // Author and timestamp
                    let author_name = comment
                        .author
                        .as_ref()
                        .map(|u| u.name.as_str())
                        .unwrap_or("Unknown");
                    let time_display = comment.created_at.format("%Y-%m-%d %H:%M").to_string();

                    // Header: Name • timestamp
                    lines.push(Line::from(vec![
                        Span::styled(author_name, Style::default().fg(Color::Blue)),
                        Span::styled(
                            format!(" • {}", time_display),
                            Style::default().fg(Color::Gray),
                        ),
                    ]));

                    // Comment text with proper markdown parsing and ┃ prefix
                    if let Some(ref text) = comment.text {
                        let cleaned_text = md::html_to_markdown(text);
                        let comment_lines = Self::format_comment_lines(&cleaned_text, None);
                        lines.extend(comment_lines.into_iter().map(|ml| ml.line));
                    }
                    lines.push(Line::from(""));
                }
            }

            // Render system activity
            if !system_activity.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    "Activity",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));

                for activity in &system_activity {
                    let time_display = activity.created_at.format("%Y-%m-%d %H:%M").to_string();
                    let icon = match activity.resource_subtype.as_deref() {
                        Some("due_date_changed") => "•",
                        Some("duplicated") => "•",
                        Some("due_today") => "•",
                        _ => "•",
                    };

                    let activity_text = activity
                        .text
                        .as_ref()
                        .map(|s| s.as_str())
                        .unwrap_or("[No text content]");
                    let mut activity_spans = vec![Span::styled(
                        format!("{icon} "),
                        Style::default().fg(Color::Yellow),
                    )];
                    activity_spans.extend(Self::format_activity_line(activity_text, &time_display));

                    lines.push(Line::from(activity_spans));
                }
            }
        }

        // Calculate scrolling with proper clamping
        let content_height = lines.len() as u16;
        let max_scroll = content_height.saturating_sub(area.height);
        let scroll_offset = self.detail_scroll_offset.min(max_scroll);

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll_offset, 0));

        frame.render_widget(paragraph, area);

        // Render scrollbar if content is longer than area
        if content_height > area.height {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state =
                ScrollbarState::new(content_height as usize).position(scroll_offset as usize);

            frame.render_stateful_widget(
                scrollbar,
                area.inner(Margin {
                    vertical: 0,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }
    }

    fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }
}

/// Calculate luminance of a color for contrast calculation
fn calculate_luminance(r: u8, g: u8, b: u8) -> f64 {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;

    let r = if r <= 0.03928 {
        r / 12.92
    } else {
        ((r + 0.055) / 1.055).powf(2.4)
    };
    let g = if g <= 0.03928 {
        g / 12.92
    } else {
        ((g + 0.055) / 1.055).powf(2.4)
    };
    let b = if b <= 0.03928 {
        b / 12.92
    } else {
        ((b + 0.055) / 1.055).powf(2.4)
    };

    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Parse hex color string to RGB values
fn hex_to_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some((r, g, b))
}

/// Determine if text should be light or dark based on background color
fn should_use_light_text(bg_color: &str) -> bool {
    if let Some((r, g, b)) = hex_to_rgb(bg_color) {
        let luminance = calculate_luminance(r, g, b);
        // Use light text if background is dark (luminance < 0.5)
        luminance < 0.5
    } else {
        // Default to dark text if we can't parse the color
        false
    }
}

/// Convert Asana color to ratatui Color
fn asana_color_to_ratatui(color: &str) -> Color {
    match color {
        "light-pink" => Color::LightRed,
        "light-purple" => Color::LightMagenta,
        "light-blue" => Color::LightBlue,
        "light-green" => Color::LightGreen,
        "light-yellow" => Color::LightYellow,
        "light-orange" => Color::Rgb(255, 165, 0),
        "light-gray" => Color::Gray,
        "light-red" => Color::LightRed,
        "dark-pink" => Color::Red,
        "dark-purple" => Color::Magenta,
        "dark-blue" => Color::Blue,
        "dark-green" => Color::Green,
        "dark-brown" => Color::Rgb(139, 69, 19),
        "dark-red" => Color::Red,
        "dark-gray" => Color::DarkGray,
        "dark-orange" => Color::Rgb(255, 140, 0),
        _ => {
            // Try to parse as hex color
            if let Some((r, g, b)) = hex_to_rgb(color) {
                Color::Rgb(r, g, b)
            } else {
                Color::Gray // Default color
            }
        }
    }
}

pub async fn run_tui(mut app: App) -> Result<()> {
    // color-eyre is already initialized in main.rs

    // Set up terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize app
    app.initialize().await?;

    // Event handling
    let mut event_handler = EventHandler::new();

    // Main loop
    loop {
        terminal.draw(|frame| app.render(frame))?;

        if let Some(event) = event_handler.next_event().await? {
            let should_quit = app.handle_event(event).await?;
            if should_quit {
                break;
            }
        }

        // Auto-load task details when selection changes
        app.auto_load_selected_task().await?;

        if event_handler.should_quit() {
            break;
        }
    }

    // Cleanup
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;

    Ok(())
}
