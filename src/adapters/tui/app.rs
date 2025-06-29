use std::sync::Arc;
use color_eyre::Result;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use crate::application::StateManager;
use crate::domain::{Task, TaskId};
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
    task_list_state: ListState,
    filtered_tasks: Vec<Task>,
    
    // Loading states
    is_loading: bool,
    error_message: Option<String>,
    
    // Comment input
    comment_input: String,
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
            task_list_state: ListState::default(),
            filtered_tasks: Vec::new(),
            is_loading: false,
            error_message: None,
            comment_input: String::new(),
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
                if self.focused_widget == FocusedWidget::Search {
                    self.search_bar.insert_char(c);
                    self.search_query = self.search_bar.query().to_string();
                    self.update_filtered_tasks();
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
                if self.focused_widget == FocusedWidget::TaskList {
                    self.next_task();
                }
            }
            
            AppEvent::PreviousTask => {
                if self.focused_widget == FocusedWidget::TaskList {
                    self.previous_task();
                }
            }
            
            AppEvent::FirstTask => {
                if self.focused_widget == FocusedWidget::TaskList && !self.filtered_tasks.is_empty() {
                    self.task_list_state.select(Some(0));
                }
            }
            
            AppEvent::LastTask => {
                if self.focused_widget == FocusedWidget::TaskList && !self.filtered_tasks.is_empty() {
                    self.task_list_state.select(Some(self.filtered_tasks.len() - 1));
                }
            }
            
            AppEvent::Enter | AppEvent::SelectTask => {
                if self.focused_widget == FocusedWidget::TaskList {
                    if let Some(selected) = self.task_list_state.selected() {
                        if let Some(task) = self.filtered_tasks.get(selected) {
                            self.mode = AppMode::TaskDetail(task.id.clone());
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

        let items: Vec<ListItem> = self.filtered_tasks
            .iter()
            .map(|task| {
                let (status_text, status_color) = task.status_display();
                let due_text = task.due_date_display();
                
                let content = format!(
                    "{} {} | Due: {}",
                    status_text,
                    task.name,
                    due_text
                );
                
                let color = match status_color {
                    "red" => Color::Red,
                    "yellow" => Color::Yellow,
                    "green" => Color::Green,
                    "gray" => Color::Gray,
                    _ => Color::White,
                };
                ListItem::new(content).style(Style::default().fg(color))
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().bg(Color::DarkGray))
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, &mut self.task_list_state);
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
            "  j/k or ↑/↓     - Move up/down in task list",
            "  g/G            - Go to first/last task",
            "  Tab/Shift+Tab  - Switch between search and task list",
            "  Enter          - Open task detail view",
            "",
            "Task Actions:",
            "  Space          - Toggle task completion",
            "  r              - Refresh task list",
            "",
            "Search:",
            "  /              - Focus search bar",
            "  Esc            - Clear search",
            "",
            "General:",
            "  ?              - Show this help",
            "  q              - Quit application",
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
        let popup_area = Self::centered_rect(80, 80, frame.area());
        
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        
        let paragraph = Paragraph::new("Task detail view coming soon!\n\nPress q or Esc to close")
            .block(Block::default().title("Task Detail").borders(Borders::ALL));
        
        frame.render_widget(paragraph, popup_area);
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