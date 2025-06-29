use std::sync::Arc;
use color_eyre::Result;
use htmd;
// Removed tui_markdown due to version compatibility issues
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap, Row, Table, Cell, TableState},
};
use crate::application::StateManager;
use crate::domain::{Task, TaskId, Comment};
use super::{
    event::{AppEvent, EventHandler},
    widgets::SearchBar,
};

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    TaskList,
    TaskDetail(TaskId),
    Help,
    CommentInput(TaskId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FocusedWidget {
    Search,
    TaskList,
}

pub struct App {
    state_manager: Arc<StateManager>,
    
    // UI State
    mode: AppMode,
    focused_widget: FocusedWidget,
    
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
    comment_input: String,
    
    // Task detail
    current_task: Option<Task>,
    task_comments: Vec<Comment>,
    detail_scroll_offset: u16,
    detail_loading: bool,
}

impl App {
    /// Handle character input with search priority
    /// If search is focused, the character goes to search. Otherwise, return true to indicate action should be executed.
    fn handle_char_with_search_priority(&mut self, c: char) -> bool {
        if self.focused_widget == FocusedWidget::Search {
            self.search_bar.insert_char(c);
            self.search_query = self.search_bar.query().to_string();
            self.update_filtered_tasks();
            false // Search handled the character
        } else {
            true // Execute the action instead
        }
    }

    /// Convert HTML description to markdown for better TUI rendering
    fn html_to_markdown(html: &str) -> String {
        if html.trim().is_empty() {
            return String::new();
        }
        
        // Convert HTML to markdown using htmd with better error handling
        // htmd has better customization options for modifying HTML before conversion
        match htmd::convert(html) {
            Ok(markdown) => {
                // Clean up extra whitespace and newlines
                markdown.trim().to_string()
            }
            Err(_) => {
                // Fallback to original HTML if conversion fails
                html.to_string()
            }
        }
    }
    pub fn new(state_manager: Arc<StateManager>) -> Self {
        let mut app = Self {
            state_manager,
            mode: AppMode::TaskList,
            focused_widget: FocusedWidget::TaskList,
            search_bar: SearchBar::new(),
            search_query: String::new(),
            tasks: Vec::new(),
            task_list_state: TableState::default(),
            filtered_tasks: Vec::new(),
            is_loading: false,
            error_message: None,
            comment_input: String::new(),
            current_task: None,
            task_comments: Vec::new(),
            detail_scroll_offset: 0,
            detail_loading: false,
        };
        
        // Select first task by default
        app.task_list_state.select(Some(0));
        app.search_bar.set_focused(false);
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
        
        match self.state_manager.get_tasks_for_current_workspace(true).await {
            Ok(tasks) => {
                self.tasks = tasks;
                self.update_filtered_tasks();
                
                // Reset selection to first item if we have tasks
                if !self.filtered_tasks.is_empty() {
                    self.task_list_state.select(Some(0));
                } else {
                    self.task_list_state.select(None);
                }
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to load tasks: {}", e));
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
            self.filtered_tasks = self.tasks
                .iter()
                .filter(|task| {
                    task.name.to_lowercase().contains(&query_lower) ||
                    task.description.as_ref().map_or(false, |desc| desc.to_lowercase().contains(&query_lower))
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
        
        // Load task details and comments in parallel
        let task_future = self.state_manager.get_task(task_id);
        let comments_future = self.state_manager.get_task_comments(task_id);
        
        let (task_result, comments_result) = tokio::join!(task_future, comments_future);
        
        match task_result {
            Ok(task) => self.current_task = Some(task),
            Err(e) => {
                self.error_message = Some(format!("Failed to load task: {}", e));
                self.current_task = None;
            }
        }
        
        match comments_result {
            Ok(comments) => self.task_comments = comments,
            Err(e) => {
                self.error_message = Some(format!("Failed to load comments: {}", e));
                self.task_comments = Vec::new();
            }
        }
        
        self.detail_loading = false;
        Ok(())
    }
    
    fn clamp_scroll_offset(&mut self) {
        // Calculate content height for scrolling bounds
        let mut content_lines = 0u16;
        
        if let Some(task) = &self.current_task {
            // Task info lines
            content_lines += 3; // Status, due date, maybe assignee
            if task.assignee.is_some() {
                content_lines += 1;
            }
            if let Some(desc) = &task.description {
                if !desc.trim().is_empty() {
                    content_lines += 3 + desc.lines().count() as u16; // Header + lines
                }
            }
            
            // Separator
            content_lines += 3;
            
            // Comments
            if self.task_comments.is_empty() {
                content_lines += 1;
            } else {
                // Comments section header + spacing
                let user_comments: Vec<_> = self.task_comments.iter()
                    .filter(|c| c.story_type.as_deref() == Some("comment"))
                    .collect();
                let system_activity: Vec<_> = self.task_comments.iter()
                    .filter(|c| c.story_type.as_deref() != Some("comment"))
                    .collect();
                
                if !user_comments.is_empty() {
                    content_lines += 2; // Header + spacing
                    for comment in &user_comments {
                        content_lines += 2 + comment.text.lines().count() as u16; // Author line + text lines + spacing
                    }
                }
                
                if !system_activity.is_empty() {
                    content_lines += 2; // Header + spacing
                    content_lines += (system_activity.len() * 2) as u16; // Each activity + spacing
                }
            }
        }
        
        // Clamp scroll offset to valid range
        let available_height = 20u16; // Rough estimate of scrollable area height
        let max_scroll = content_lines.saturating_sub(available_height);
        self.detail_scroll_offset = self.detail_scroll_offset.min(max_scroll);
    }

    pub async fn handle_event(&mut self, event: AppEvent) -> Result<bool> {
        match event {
            AppEvent::Quit => return Ok(true),
            
            AppEvent::Refresh => {
                self.load_tasks().await?;
            }
            
            AppEvent::FocusSearch => {
                self.focused_widget = FocusedWidget::Search;
                self.search_bar.set_focused(true);
            }
            
            AppEvent::Tab => {
                match self.focused_widget {
                    FocusedWidget::Search => {
                        self.focused_widget = FocusedWidget::TaskList;
                        self.search_bar.set_focused(false);
                    }
                    FocusedWidget::TaskList => {
                        self.focused_widget = FocusedWidget::Search;
                        self.search_bar.set_focused(true);
                    }
                }
            }
            
            AppEvent::BackTab => {
                // Same as Tab but in reverse - just duplicate the logic to avoid recursion
                match self.focused_widget {
                    FocusedWidget::Search => {
                        self.focused_widget = FocusedWidget::TaskList;
                        self.search_bar.set_focused(false);
                    }
                    FocusedWidget::TaskList => {
                        self.focused_widget = FocusedWidget::Search;
                        self.search_bar.set_focused(true);
                    }
                }
            }
            
            AppEvent::Character(c) => {
                match c {
                    'q' => {
                        // Search takes priority - if search is focused, type the character
                        if self.focused_widget == FocusedWidget::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            // Context-sensitive quit when not in search
                            match &self.mode {
                                AppMode::TaskDetail(_) | AppMode::Help => {
                                    self.mode = AppMode::TaskList;
                                }
                                _ => {
                                    return Ok(true); // Quit app
                                }
                            }
                        }
                    }
                    'j' => {
                        if self.focused_widget == FocusedWidget::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            match &self.mode {
                                AppMode::TaskDetail(_) => {
                                    self.detail_scroll_offset = self.detail_scroll_offset.saturating_add(1);
                                    self.clamp_scroll_offset();
                                }
                                _ => {
                                    if self.focused_widget == FocusedWidget::TaskList {
                                        self.next_task();
                                    }
                                }
                            }
                        }
                    }
                    'k' => {
                        if self.focused_widget == FocusedWidget::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            match &self.mode {
                                AppMode::TaskDetail(_) => {
                                    self.detail_scroll_offset = self.detail_scroll_offset.saturating_sub(1);
                                    self.clamp_scroll_offset();
                                }
                                _ => {
                                    if self.focused_widget == FocusedWidget::TaskList {
                                        self.previous_task();
                                    }
                                }
                            }
                        }
                    }
                    'g' => {
                        if self.focused_widget == FocusedWidget::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            match &self.mode {
                                AppMode::TaskDetail(_) => {
                                    self.detail_scroll_offset = 0;
                                }
                                _ => {
                                    if self.focused_widget == FocusedWidget::TaskList && !self.filtered_tasks.is_empty() {
                                        self.task_list_state.select(Some(0));
                                    }
                                }
                            }
                        }
                    }
                    'G' => {
                        if self.focused_widget == FocusedWidget::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        } else {
                            match &self.mode {
                                AppMode::TaskDetail(_) => {
                                    self.detail_scroll_offset = u16::MAX;
                                    self.clamp_scroll_offset();
                                }
                                _ => {
                                    if self.focused_widget == FocusedWidget::TaskList && !self.filtered_tasks.is_empty() {
                                        self.task_list_state.select(Some(self.filtered_tasks.len() - 1));
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
                            if self.focused_widget == FocusedWidget::TaskList {
                                if let Some(selected) = self.task_list_state.selected() {
                                    if let Some(task) = self.filtered_tasks.get(selected) {
                                        match self.state_manager.toggle_task_completion(&task.id).await {
                                            Ok(_) => {
                                                self.load_tasks().await?;
                                            }
                                            Err(e) => {
                                                self.error_message = Some(format!("Failed to toggle task: {}", e));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        // Regular character input for search
                        if self.focused_widget == FocusedWidget::Search {
                            self.search_bar.insert_char(c);
                            self.search_query = self.search_bar.query().to_string();
                            self.update_filtered_tasks();
                        }
                    }
                }
            }
            
            AppEvent::Backspace => {
                if self.focused_widget == FocusedWidget::Search {
                    self.search_bar.delete_char();
                    self.search_query = self.search_bar.query().to_string();
                    self.update_filtered_tasks();
                }
            }
            
            AppEvent::CloseModal | AppEvent::ClearSearch => {
                if self.focused_widget == FocusedWidget::Search && !self.search_query.is_empty() {
                    self.search_bar.clear();
                    self.search_query.clear();
                    self.update_filtered_tasks();
                } else {
                    self.mode = AppMode::TaskList;
                }
            }
            
            AppEvent::NextTask => {
                // Arrow keys only - for simple navigation
                match &self.mode {
                    AppMode::TaskDetail(_) => {
                        self.detail_scroll_offset = self.detail_scroll_offset.saturating_add(1);
                        self.clamp_scroll_offset();
                    }
                    _ => {
                        if self.focused_widget == FocusedWidget::TaskList {
                            self.next_task();
                        }
                    }
                }
            }
            
            AppEvent::PreviousTask => {
                // Arrow keys only - for simple navigation
                match &self.mode {
                    AppMode::TaskDetail(_) => {
                        self.detail_scroll_offset = self.detail_scroll_offset.saturating_sub(1);
                        self.clamp_scroll_offset();
                    }
                    _ => {
                        if self.focused_widget == FocusedWidget::TaskList {
                            self.previous_task();
                        }
                    }
                }
            }
            
            AppEvent::Enter | AppEvent::SelectTask => {
                if self.focused_widget == FocusedWidget::TaskList {
                    if let Some(selected) = self.task_list_state.selected() {
                        if let Some(task) = self.filtered_tasks.get(selected) {
                            let task_id = task.id.clone();
                            self.mode = AppMode::TaskDetail(task_id.clone());
                            self.load_task_details(&task_id).await?;
                        }
                    }
                }
            }
            
            AppEvent::ToggleTaskComplete => {
                if self.focused_widget == FocusedWidget::TaskList {
                    if let Some(selected) = self.task_list_state.selected() {
                        if let Some(task) = self.filtered_tasks.get(selected) {
                            match self.state_manager.toggle_task_completion(&task.id).await {
                                Ok(_) => {
                                    self.load_tasks().await?;
                                }
                                Err(e) => {
                                    self.error_message = Some(format!("Failed to toggle task: {}", e));
                                }
                            }
                        }
                    }
                }
            }
            
            AppEvent::ShowHelp => {
                self.mode = AppMode::Help;
            }
            
            // Task detail scroll events
            AppEvent::ScrollDetailUp => {
                if matches!(self.mode, AppMode::TaskDetail(_)) {
                    self.detail_scroll_offset = self.detail_scroll_offset.saturating_sub(1);
                    self.clamp_scroll_offset();
                }
            }
            
            AppEvent::ScrollDetailDown => {
                if matches!(self.mode, AppMode::TaskDetail(_)) {
                    self.detail_scroll_offset = self.detail_scroll_offset.saturating_add(1);
                    self.clamp_scroll_offset();
                }
            }
            
            AppEvent::ScrollDetailPageUp => {
                if matches!(self.mode, AppMode::TaskDetail(_)) {
                    self.detail_scroll_offset = self.detail_scroll_offset.saturating_sub(10);
                    self.clamp_scroll_offset();
                }
            }
            
            AppEvent::ScrollDetailPageDown => {
                if matches!(self.mode, AppMode::TaskDetail(_)) {
                    self.detail_scroll_offset = self.detail_scroll_offset.saturating_add(10);
                    self.clamp_scroll_offset();
                }
            }
            
            AppEvent::ScrollDetailToTop => {
                if matches!(self.mode, AppMode::TaskDetail(_)) {
                    self.detail_scroll_offset = 0;
                    self.clamp_scroll_offset();
                }
            }
            
            AppEvent::ScrollDetailToBottom => {
                if matches!(self.mode, AppMode::TaskDetail(_)) {
                    self.detail_scroll_offset = u16::MAX;
                    self.clamp_scroll_offset();
                }
            }
            
            _ => {} // Handle other events as needed
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
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search bar
                Constraint::Min(0),    // Task list
                Constraint::Length(1), // Status bar
            ])
            .split(frame.area());

        // Render search bar
        self.search_bar.render(frame, chunks[0]);

        // Render task list
        self.render_task_list(frame, chunks[1]);

        // Render status bar
        self.render_status_bar(frame, chunks[2]);

        // Render modals
        match &self.mode {
            AppMode::Help => self.render_help(frame),
            AppMode::TaskDetail(task_id) => self.render_task_detail(frame, task_id),
            _ => {}
        }
    }

    fn render_task_list(&mut self, frame: &mut Frame, area: Rect) {
        let title = format!("Tasks ({})", self.filtered_tasks.len());
        let border_style = if self.focused_widget == FocusedWidget::TaskList {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Gray)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
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

        let rows: Vec<Row> = self.filtered_tasks
            .iter()
            .map(|task| {
                let due_text = task.due_date_display();
                
                // Check if task is overdue for red coloring
                let is_overdue = task.is_overdue();
                let due_style = if is_overdue {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                
                Row::new(vec![
                    Cell::from(task.name.clone()),
                    Cell::from(due_text).style(due_style),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            &[
                Constraint::Min(20),    // Title column (flexible)
                Constraint::Length(12), // Due date column
            ]
        )
        .block(block)
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");

        frame.render_stateful_widget(table, area, &mut self.task_list_state);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let help_text = match self.focused_widget {
            FocusedWidget::Search => "Tab: switch to tasks | Enter: go to tasks | /: focus search | q: quit | ?: help",
            FocusedWidget::TaskList => "j/k: navigate | Enter: view task | Space: toggle complete | /: search | q: quit | ?: help",
        };

        let paragraph = Paragraph::new(help_text)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(paragraph, area);
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
        ].join("\n");

        let paragraph = Paragraph::new(help_text)
            .block(Block::default().title("Help").borders(Borders::ALL))
            .wrap(ratatui::widgets::Wrap { trim: true });
        
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
        
        // Split the modal into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(2),     // Task title (flexible for wrapping)
                Constraint::Min(5),     // All scrollable content
                Constraint::Length(1),  // Instructions
            ])
            .margin(1)
            .split(popup_area);
        
        // Render task title at top in bold with wrapping
        let title_paragraph = Paragraph::new(task.name.clone())
            .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(title_paragraph, chunks[0]);
        
        // Render all content as one scrollable section
        self.render_scrollable_content(frame, chunks[1], task);
        
        // Render instructions
        let instructions = Paragraph::new("↑↓: scroll | q/Esc: close | g/G: top/bottom | Page Up/Down: fast scroll")
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(instructions, chunks[2]);
        
        // Render border with title including task ID
        let border = Block::default()
            .title(format!("Task Detail - {}", task.id.0))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        frame.render_widget(border, popup_area);
    }
    
    fn render_scrollable_content(&self, frame: &mut Frame, area: Rect, task: &Task) {
        // Calculate layout for different sections
        let has_description = task.description.as_ref().map_or(false, |d| !d.trim().is_empty());
        
        // Calculate how much space the description actually needs
        let description_lines = if has_description {
            let markdown_desc = Self::html_to_markdown(task.description.as_ref().unwrap());
            let styled_lines = Self::parse_markdown_to_lines(&markdown_desc);
            styled_lines.len() as u16 + 2 // +2 for title block and blank line
        } else {
            0
        };
        
        let constraints = if has_description {
            vec![
                Constraint::Length(4),                     // Task info (status, due date, assignee)
                Constraint::Length(description_lines.min(15)), // Description (actual size, max 15 lines)
                Constraint::Length(1),                     // Separator
                Constraint::Min(3),                        // Comments/activity (takes remaining space)
            ]
        } else {
            vec![
                Constraint::Length(4),  // Task info
                Constraint::Length(1),  // Separator
                Constraint::Min(3),     // Comments/activity
            ]
        };
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);
        
        let mut chunk_idx = 0;
        
        // Render task info section
        self.render_task_info_section(frame, chunks[chunk_idx], task);
        chunk_idx += 1;
        
        // Render description with markdown if present
        if has_description {
            self.render_description_section(frame, chunks[chunk_idx], task);
            chunk_idx += 1;
        }
        
        // Render separator - use actual area width
        let separator_width = chunks[chunk_idx].width.saturating_sub(2) as usize; // Account for padding
        let separator = Paragraph::new(Line::from(vec![
            Span::styled("─".repeat(separator_width), Style::default().fg(Color::DarkGray))
        ]));
        frame.render_widget(separator, chunks[chunk_idx]);
        chunk_idx += 1;
        
        // Render comments/activity section
        self.render_comments_section(frame, chunks[chunk_idx]);
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
                let markdown_desc = Self::html_to_markdown(description);
                
                // Parse and render markdown with custom styling
                let mut styled_lines = Self::parse_markdown_to_lines(&markdown_desc);
                
                // Add blank line after Description header
                styled_lines.insert(0, Line::from(""));
                
                // Prepend "Description:" as the first line instead of using a block
                styled_lines.insert(0, Line::from(vec![
                    Span::styled("Description:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                ]));
                
                let paragraph = Paragraph::new(styled_lines)
                    .wrap(Wrap { trim: true });
                
                frame.render_widget(paragraph, area);
            }
        }
    }
    
    /// Parse markdown text and convert to styled Lines for better rendering
    fn parse_markdown_to_lines(markdown: &str) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        
        for line in markdown.lines() {
            let trimmed = line.trim();
            
            if trimmed.is_empty() {
                lines.push(Line::from(""));
                continue;
            }
            
            // Handle headers
            if trimmed.starts_with("# ") {
                let text = &trimmed[2..];
                lines.push(Line::from(vec![
                    Span::styled(text.to_string(), Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD))
                ]));
                lines.push(Line::from(""));
            } else if trimmed.starts_with("## ") {
                let text = &trimmed[3..];
                lines.push(Line::from(vec![
                    Span::styled(text.to_string(), Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD))
                ]));
            } else if trimmed.starts_with("### ") {
                let text = &trimmed[4..];
                lines.push(Line::from(vec![
                    Span::styled(text.to_string(), Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD))
                ]));
            }
            // Handle bullet points
            else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                let text = &trimmed[2..];
                lines.push(Line::from(vec![
                    Span::styled("• ", Style::default().fg(Color::Green)),
                    Span::raw(text.to_string()),
                ]));
            }
            // Handle numbered lists
            else if trimmed.chars().next().map_or(false, |c| c.is_ascii_digit()) && trimmed.contains(". ") {
                if let Some(dot_pos) = trimmed.find(". ") {
                    let number = &trimmed[..dot_pos + 1];
                    let text = &trimmed[dot_pos + 2..];
                    lines.push(Line::from(vec![
                        Span::styled(format!("{} ", number), Style::default().fg(Color::Magenta)),
                        Span::raw(text.to_string()),
                    ]));
                } else {
                    lines.push(Line::from(trimmed.to_string()));
                }
            }
            // Handle bold text (basic **text** parsing)
            else if trimmed.contains("**") {
                let styled_line = Self::parse_bold_text(trimmed);
                lines.push(styled_line);
            }
            // Handle italic text (basic *text* parsing)
            else if trimmed.contains('*') && !trimmed.starts_with("*") {
                let styled_line = Self::parse_italic_text(trimmed);
                lines.push(styled_line);
            }
            // Handle code blocks or inline code
            else if trimmed.starts_with("```") {
                lines.push(Line::from(vec![
                    Span::styled(trimmed.to_string(), Style::default()
                        .fg(Color::Gray)
                        .bg(Color::DarkGray))
                ]));
            }
            else if trimmed.contains('`') {
                let styled_line = Self::parse_inline_code(trimmed);
                lines.push(styled_line);
            }
            // Regular text
            else {
                lines.push(Line::from(trimmed.to_string()));
            }
        }
        
        // Remove trailing empty lines to reduce blank space
        while let Some(last_line) = lines.last() {
            if last_line.spans.is_empty() || 
               (last_line.spans.len() == 1 && last_line.spans[0].content.is_empty()) {
                lines.pop();
            } else {
                break;
            }
        }
        
        lines
    }
    
    /// Parse bold text (**text**)
    fn parse_bold_text(text: &str) -> Line<'static> {
        let mut spans = Vec::new();
        let mut current = String::new();
        let mut in_bold = false;
        let mut chars = text.chars().peekable();
        
        while let Some(ch) = chars.next() {
            if ch == '*' && chars.peek() == Some(&'*') {
                chars.next(); // consume second *
                if !current.is_empty() {
                    spans.push(if in_bold {
                        Span::styled(current.clone(), Style::default().add_modifier(Modifier::BOLD))
                    } else {
                        Span::raw(current.clone())
                    });
                    current.clear();
                }
                in_bold = !in_bold;
            } else {
                current.push(ch);
            }
        }
        
        if !current.is_empty() {
            spans.push(if in_bold {
                Span::styled(current, Style::default().add_modifier(Modifier::BOLD))
            } else {
                Span::raw(current)
            });
        }
        
        Line::from(spans)
    }
    
    /// Parse italic text (*text*)
    fn parse_italic_text(text: &str) -> Line<'static> {
        let mut spans = Vec::new();
        let mut current = String::new();
        let mut in_italic = false;
        
        for ch in text.chars() {
            if ch == '*' && !in_italic {
                if !current.is_empty() {
                    spans.push(Span::raw(current.clone()));
                    current.clear();
                }
                in_italic = true;
            } else if ch == '*' && in_italic {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), Style::default().add_modifier(Modifier::ITALIC)));
                    current.clear();
                }
                in_italic = false;
            } else {
                current.push(ch);
            }
        }
        
        if !current.is_empty() {
            spans.push(if in_italic {
                Span::styled(current, Style::default().add_modifier(Modifier::ITALIC))
            } else {
                Span::raw(current)
            });
        }
        
        Line::from(spans)
    }
    
    /// Parse inline code (`code`)
    fn parse_inline_code(text: &str) -> Line<'static> {
        let mut spans = Vec::new();
        let mut current = String::new();
        let mut in_code = false;
        
        for ch in text.chars() {
            if ch == '`' {
                if !current.is_empty() {
                    spans.push(if in_code {
                        Span::styled(current.clone(), Style::default()
                            .fg(Color::Green)
                            .bg(Color::DarkGray))
                    } else {
                        Span::raw(current.clone())
                    });
                    current.clear();
                }
                in_code = !in_code;
            } else {
                current.push(ch);
            }
        }
        
        if !current.is_empty() {
            spans.push(if in_code {
                Span::styled(current, Style::default()
                    .fg(Color::Green)
                    .bg(Color::DarkGray))
            } else {
                Span::raw(current)
            });
        }
        
        Line::from(spans)
    }
    
    fn render_comments_section(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();
        
        // Comments and activity
        if self.task_comments.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("No comments or activity", Style::default().fg(Color::Gray)),
            ]));
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
                lines.push(Line::from(vec![
                    Span::styled("Comments", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                ]));
                lines.push(Line::from(""));
                
                for comment in &user_comments {
                    // Author and timestamp
                    let author_name = comment.author.as_ref()
                        .map(|u| u.name.as_str())
                        .unwrap_or("Unknown");
                    let time_display = comment.created_at.format("%Y-%m-%d %H:%M").to_string();
                    
                    lines.push(Line::from(vec![
                        Span::styled(format!("{} • ", author_name), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                        Span::styled(time_display, Style::default().fg(Color::Gray)),
                    ]));
                    
                    // Comment text with word wrapping
                    for line in comment.text.lines() {
                        lines.push(Line::from(line));
                    }
                    lines.push(Line::from(""));
                }
            }
            
            // Render system activity
            if !system_activity.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Activity", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ]));
                lines.push(Line::from(""));
                
                for activity in &system_activity {
                    let time_display = activity.created_at.format("%Y-%m-%d %H:%M").to_string();
                    let icon = match activity.resource_subtype.as_deref() {
                        Some("due_date_changed") => "•",
                        Some("duplicated") => "•",
                        Some("due_today") => "•",
                        _ => "•",
                    };
                    
                    lines.push(Line::from(vec![
                        Span::styled(format!("{} ", icon), Style::default().fg(Color::Yellow)),
                        Span::raw(activity.text.clone()),
                        Span::styled(format!(" ({})", time_display), Style::default().fg(Color::Gray)),
                    ]));
                }
            }
        }
        
        // Calculate scrolling with proper clamping
        let content_height = lines.len() as u16;
        let max_scroll = content_height.saturating_sub(area.height);
        let scroll_offset = self.detail_scroll_offset.min(max_scroll);
        
        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((scroll_offset, 0));
        
        frame.render_widget(paragraph, area);
        
        // Render scrollbar if content is longer than area
        if content_height > area.height {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));
            
            let mut scrollbar_state = ScrollbarState::new(content_height as usize)
                .position(scroll_offset as usize);
            
            frame.render_stateful_widget(
                scrollbar,
                area.inner(Margin { vertical: 0, horizontal: 0 }),
                &mut scrollbar_state,
            );
        }
    }

    fn render_task_info(&self, frame: &mut Frame, area: Rect, task: &Task) {
        let mut lines = Vec::new();
        
        // Task name and status
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
        
        // Due date
        lines.push(Line::from(vec![
            Span::styled("Due: ", Style::default().fg(Color::Cyan)),
            Span::raw(task.due_date_display()),
        ]));
        
        // Assignee
        if task.assignee.is_some() {
            let assignee_display = task.assignee_name.as_deref().unwrap_or("Unknown User");
            lines.push(Line::from(vec![
                Span::styled("Assignee: ", Style::default().fg(Color::Cyan)),
                Span::raw(assignee_display.to_string()),
            ]));
        }
        
        // Description
        if let Some(description) = &task.description {
            if !description.trim().is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Description:", Style::default().fg(Color::Cyan)),
                ]));
                let markdown_desc = Self::html_to_markdown(description);
                for line in markdown_desc.lines() {
                    lines.push(Line::from(line.to_string()));
                }
            }
        }
        
        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }
    
    fn render_task_comments(&self, frame: &mut Frame, area: Rect) {
        if self.task_comments.is_empty() {
            let paragraph = Paragraph::new("No comments or activity")
                .style(Style::default().fg(Color::Gray));
            frame.render_widget(paragraph, area);
            return;
        }
        
        let mut lines = Vec::new();
        
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
            lines.push(Line::from(vec![
                Span::styled("💬 Comments", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(""));
            
            for comment in &user_comments {
                // Author and timestamp
                let author_name = comment.author.as_ref()
                    .map(|u| u.name.as_str())
                    .unwrap_or("Unknown");
                let time_display = comment.created_at.format("%Y-%m-%d %H:%M").to_string();
                
                lines.push(Line::from(vec![
                    Span::styled(format!("{} • ", author_name), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                    Span::styled(time_display, Style::default().fg(Color::Gray)),
                ]));
                
                // Comment text with word wrapping
                for line in comment.text.lines() {
                    lines.push(Line::from(line));
                }
                lines.push(Line::from(""));
            }
        }
        
        // Render system activity
        if !system_activity.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("📋 Activity", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(""));
            
            for activity in &system_activity {
                let time_display = activity.created_at.format("%Y-%m-%d %H:%M").to_string();
                let icon = match activity.resource_subtype.as_deref() {
                    Some("due_date_changed") => "📅",
                    Some("duplicated") => "📄",
                    Some("due_today") => "⏰",
                    _ => "•",
                };
                
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(Color::Yellow)),
                    Span::raw(activity.text.clone()),
                    Span::styled(format!(" ({})", time_display), Style::default().fg(Color::Gray)),
                ]));
                lines.push(Line::from(""));
            }
        }
        
        // Calculate scrolling
        let content_height = lines.len() as u16;
        let max_scroll = content_height.saturating_sub(area.height);
        let scroll_offset = self.detail_scroll_offset.min(max_scroll);
        
        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((scroll_offset, 0));
        
        frame.render_widget(paragraph, area);
        
        // Render scrollbar if content is longer than area
        if content_height > area.height {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));
            
            let mut scrollbar_state = ScrollbarState::new(content_height as usize)
                .position(scroll_offset as usize);
            
            frame.render_stateful_widget(
                scrollbar,
                area.inner(Margin { vertical: 0, horizontal: 0 }),
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

        let event = event_handler.next_event().await?;
        let should_quit = app.handle_event(event).await?;

        if should_quit || event_handler.should_quit() {
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