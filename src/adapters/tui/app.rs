use std::sync::Arc;
use color_eyre::Result;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap, Row, Table, Cell, TableState},
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
    pub fn new(state_manager: Arc<StateManager>) -> Self {
        let mut app = Self {
            state_manager,
            mode: AppMode::TaskList,
            focused_widget: FocusedWidget::Search,
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
                let (status_text, _) = task.status_display();
                let due_text = task.due_date_display();
                
                // Check if task is overdue for red coloring
                let is_overdue = task.is_overdue();
                let due_style = if is_overdue {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                
                Row::new(vec![
                    Cell::from(status_text),
                    Cell::from(task.name.clone()),
                    Cell::from(due_text).style(due_style),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            &[
                Constraint::Length(4),  // Status column
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
        let popup_area = Self::centered_rect(85, 85, frame.area());
        
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
                Constraint::Length(3),  // Task title
                Constraint::Min(0),     // All scrollable content
                Constraint::Length(1),  // Instructions
            ])
            .margin(1)
            .split(popup_area);
        
        // Render task title at top in bold
        let title_paragraph = Paragraph::new(task.name.clone())
            .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(title_paragraph, chunks[0]);
        
        // Render all content as one scrollable section
        self.render_scrollable_content(frame, chunks[1], task);
        
        // Render instructions
        let instructions = Paragraph::new("↑↓: scroll | q/Esc: close | g/G: top/bottom | Page Up/Down: fast scroll")
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(instructions, chunks[2]);
        
        // Render border without title
        let border = Block::default()
            .title("Task Detail")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        frame.render_widget(border, popup_area);
    }
    
    fn render_scrollable_content(&self, frame: &mut Frame, area: Rect, task: &Task) {
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
        if let Some(assignee_id) = &task.assignee {
            lines.push(Line::from(vec![
                Span::styled("Assignee: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("User {}", assignee_id.0)),
            ]));
        }
        
        // Description
        if let Some(description) = &task.description {
            if !description.trim().is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Description:", Style::default().fg(Color::Cyan)),
                ]));
                lines.push(Line::from(description.as_str()));
            }
        }
        
        // Add separator before comments
        lines.push(Line::from(""));
        lines.push(Line::from("─".repeat(60)));
        lines.push(Line::from(""));
        
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
        if let Some(assignee_id) = &task.assignee {
            lines.push(Line::from(vec![
                Span::styled("Assignee: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("User {}", assignee_id.0)),
            ]));
        }
        
        // Description
        if let Some(description) = &task.description {
            if !description.trim().is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Description:", Style::default().fg(Color::Cyan)),
                ]));
                lines.push(Line::from(description.as_str()));
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