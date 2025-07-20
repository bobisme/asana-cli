use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use color_eyre::Result;
use ratatui::{
    crossterm::{
        self,
        terminal::{disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::Rect,
    prelude::*,
};
use tokio::sync::mpsc;
use tracing::info;

use super::{search::fuzzy_search_tasks, views::View as _};
use crate::domain::{
    comment::Comment,
    task::{repo::TaskRepository, Task, TaskFilter, TaskId},
};
use crate::{
    adapters::tui::views::tasks::TaskView,
    app::error::{AppError, RepositoryError},
};
use crate::{
    adapters::{
        api::{AsanaClient, AsanaTaskRepository},
        tui::components::{
            comments_pane::CommentsPane, description_pane::DescriptionPane, search_bar::SearchBar,
            task_list_pane::TaskListPane, Component,
        },
    },
    domain::workspace::repo::WorkspaceRepository,
};

#[derive(Debug, Clone, Copy)]
pub enum View {
    TaskList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Comments,
    Description,
    SearchBar,
    TaskList,
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone)]
pub enum Event {
    FocusedPane(Pane),
    Init,
    Quit,
    ReceivedTasks(Vec<Task>),
    RequestError(RepositoryError),
    SelectedTask(Option<usize>),
    RequestComments(TaskId),
    ReceivedComments(TaskId, Vec<Comment>),
    CommentLoadError(TaskId, RepositoryError),
    ScrollComments {
        dir: Direction,
        count: usize,
    },
    UpdatePaneArea {
        pane: Pane,
        area: Rect,
    },
    TerminalResized,
    FullScreenOff,
    FullScreenOn,
    Searched {
        query: String,
        cursor_position: usize,
    },
    ChangedCursorPosition(usize),
    ClearedSearch,
}

#[derive(Debug, Default, Clone)]
pub struct TaskListState {
    pub is_loading: bool,
    pub selected_row_index: Option<usize>,
    pub filtered_task_ids: Vec<TaskId>,
    pub error: Option<AppError>,
}

#[derive(Debug, Default, Clone)]
pub struct SearchState {
    pub query: String,
    pub cursor_pos: usize,
}

#[derive(Debug, Clone, Default)]
pub struct CommentState {
    pub comments: HashMap<TaskId, Vec<Comment>>,
    pub loading_comments: HashSet<TaskId>,
    pub comment_errors: HashMap<TaskId, AppError>,
    pub scroll_offset: usize,
    pub content_lines: usize,
    pub visible_height: usize,
}

#[derive(Debug, Clone)]
pub struct State {
    pub is_running: bool,
    pub view: View,
    pub focus: Pane,
    pub tasks: HashMap<TaskId, Task>,
    pub comment_state: CommentState,
    pub last_error: Option<AppError>,
    pub fullscreen_pane: bool,
    pub task_list_state: TaskListState,
    pub search: SearchState,
}

impl Default for State {
    fn default() -> Self {
        Self {
            is_running: true,
            view: View::TaskList,
            focus: Pane::TaskList,
            tasks: Default::default(),
            comment_state: Default::default(),
            last_error: None,
            task_list_state: TaskListState {
                is_loading: true,
                ..Default::default()
            },
            search: Default::default(),
            fullscreen_pane: false,
        }
    }
}

impl State {
    pub fn selected_task_id(&self) -> Option<&TaskId> {
        let row_index = self.task_list_state.selected_row_index?;
        self.task_list_state.filtered_task_ids.get(row_index)
    }

    pub fn selected_task(&self) -> Option<&Task> {
        let task_id = self.selected_task_id()?;
        self.tasks.get(task_id)
    }

    pub fn selected_task_comments(&self) -> Option<&Vec<Comment>> {
        let task_id = self.selected_task_id()?;
        self.comment_state.comments.get(task_id)
    }

    pub fn is_loading_comments(&self) -> bool {
        self.selected_task_id()
            .map(|task_id| self.comment_state.loading_comments.contains(task_id))
            .unwrap_or(false)
    }

    pub fn comment_load_error(&self) -> Option<&AppError> {
        let task_id = self.selected_task_id()?;
        self.comment_state.comment_errors.get(task_id)
    }

    pub fn comments_scroll_offset(&self) -> usize {
        self.comment_state.scroll_offset
    }

    pub fn comments_max_scroll(&self) -> usize {
        self.comment_state
            .content_lines
            .saturating_sub(self.comment_state.visible_height)
    }
}

fn view(state: &State, frame: &mut Frame) {
    match state.view {
        View::TaskList => TaskView::render(state, frame),
    }
}

struct App<T: TaskRepository + WorkspaceRepository> {
    task_repo: Arc<T>,
}

impl<T: TaskRepository + WorkspaceRepository> App<T> {
    fn task_repository(&self) -> Arc<T> {
        Arc::clone(&self.task_repo)
    }

    fn initialize(&self, tx: &mpsc::UnboundedSender<Event>) {
        let task_repo = self.task_repository();
        let tx = tx.clone();
        tokio::spawn(async move {
            let closure = async move || -> std::result::Result<Event, RepositoryError> {
                let workspaces = task_repo.list_workspaces().await?;
                info!(?workspaces, "got workspaces");
                let Some(workspace) = workspaces.first() else {
                    return Err(RepositoryError::NotFound("No default workspace".to_owned()));
                };
                let user = task_repo.get_current_user().await?;
                info!(?user, "got user");
                let filter = TaskFilter {
                    assignee: Some(user.id.clone()),
                    workspace: Some(workspace.id.clone()),
                    ..Default::default()
                };
                let tasks = task_repo.list_tasks(&filter).await?;
                info!("got {} tasks", tasks.len());
                Ok(Event::ReceivedTasks(tasks))
            };
            let event = match closure().await {
                Ok(event) => event,
                Err(e) => Event::RequestError(e),
            };
            let _ = tx.send(event);
        });
    }

    fn load_comments(&self, task_id: TaskId, tx: &mpsc::UnboundedSender<Event>) {
        let task_repo = self.task_repository();
        let tx = tx.clone();
        let task_id_clone = task_id.clone();
        tokio::spawn(async move {
            let closure = async move || -> std::result::Result<Event, RepositoryError> {
                let comments = task_repo.get_task_comments(&task_id).await?;
                info!("got {} comments for task {}", comments.len(), task_id);
                Ok(Event::ReceivedComments(task_id, comments))
            };
            let event = match closure().await {
                Ok(event) => event,
                Err(e) => Event::CommentLoadError(task_id_clone, e),
            };
            let _ = tx.send(event);
        });
    }

    async fn update(
        &self,
        state: &State,
        event: Event,
        tx: &mpsc::UnboundedSender<Event>,
    ) -> (Option<State>, Option<Event>) {
        match event {
            Event::Init => {
                self.initialize(tx);
                (None, None)
            }
            Event::ReceivedTasks(tasks) => {
                let mut new_state = state.clone();
                let mut map = HashMap::new();
                let mut list = Vec::new();
                for task in tasks.iter() {
                    map.insert(task.id.clone(), task.clone());
                    list.push(task.id.clone());
                }
                new_state.tasks = map;
                new_state.task_list_state.is_loading = false;
                new_state.task_list_state.filtered_task_ids = list;
                (Some(new_state), Some(Event::SelectedTask(Some(0))))
            }
            Event::SelectedTask(idx) => {
                let len = state.task_list_state.filtered_task_ids.len();
                let idx = idx.map(|x| x.clamp(0, len.saturating_sub(1)));
                if idx == state.task_list_state.selected_row_index {
                    return (None, None);
                }
                let mut state = state.clone();
                state.task_list_state.selected_row_index = idx;

                // Reset scroll when switching tasks and trigger comment loading if needed
                state.comment_state.scroll_offset = 0;

                let next_event = if let Some(task_id) = state.selected_task_id() {
                    if !state.comment_state.comments.contains_key(task_id)
                        && !state.comment_state.loading_comments.contains(task_id)
                        && !state.comment_state.comment_errors.contains_key(task_id)
                    {
                        Some(Event::RequestComments(task_id.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                };

                (Some(state), next_event)
            }
            Event::RequestComments(task_id) => {
                let mut state = state.clone();
                state.comment_state.loading_comments.insert(task_id.clone());
                state.comment_state.comment_errors.remove(&task_id);
                self.load_comments(task_id, tx);
                (Some(state), None)
            }
            Event::ReceivedComments(task_id, comments) => {
                let mut state = state.clone();
                state.comment_state.loading_comments.remove(&task_id);

                // Calculate content lines for the new comments
                let content_lines = crate::adapters::tui::components::comments_pane::CommentsPane::calculate_content_lines(&comments, 80); // Default width, will be updated by area events
                state.comment_state.content_lines = content_lines;

                state.comment_state.comments.insert(task_id, comments);
                state.comment_state.scroll_offset = 0; // Reset scroll for new content
                (Some(state), None)
            }
            Event::CommentLoadError(task_id, err) => {
                let mut state = state.clone();
                state.comment_state.loading_comments.remove(&task_id);
                state
                    .comment_state
                    .comment_errors
                    .insert(task_id, AppError::Repository(err));
                (Some(state), None)
            }
            Event::ScrollComments { dir, count } => {
                let mut state = state.clone();
                let max_scroll = state.comments_max_scroll();

                match dir {
                    Direction::Up => {
                        state.comment_state.scroll_offset =
                            state.comment_state.scroll_offset.saturating_sub(count);
                    }
                    Direction::Down => {
                        state.comment_state.scroll_offset =
                            (state.comment_state.scroll_offset + count).min(max_scroll);
                    }
                }
                (Some(state), None)
            }
            Event::UpdatePaneArea {
                pane: Pane::Comments,
                area,
            } => {
                let mut state = state.clone();
                let new_height = area.height.saturating_sub(2) as usize; // Account for borders
                let new_width = area.width.saturating_sub(2); // Account for borders

                let should_recalculate = state.comment_state.visible_height != new_height;
                state.comment_state.visible_height = new_height;

                // Recalculate content lines if we have comments and width might have changed
                if should_recalculate {
                    if let Some(comments) = state.selected_task_comments() {
                        let content_lines = crate::adapters::tui::components::comments_pane::CommentsPane::calculate_content_lines(comments, new_width);
                        state.comment_state.content_lines = content_lines;
                    }
                }
                (Some(state), None)
            }
            Event::UpdatePaneArea { .. } => {
                // Ignore other pane area updates for now
                (None, None)
            }
            Event::TerminalResized => {
                let mut state = state.clone();
                // Recalculate content lines for current comments if we have any
                if let Some(comments) = state.selected_task_comments() {
                    // Estimate comment pane width: 60% of terminal width for right side, minus borders
                    // This is approximate but should be close enough for line counting
                    let estimated_width = 80; // Default reasonable width
                    let content_lines = crate::adapters::tui::components::comments_pane::CommentsPane::calculate_content_lines(comments, estimated_width);
                    state.comment_state.content_lines = content_lines;

                    // Ensure scroll doesn't exceed new content bounds
                    let max_scroll = state.comments_max_scroll();
                    if state.comment_state.scroll_offset > max_scroll {
                        state.comment_state.scroll_offset = max_scroll;
                    }
                }
                (Some(state), None)
            }
            Event::RequestError(err) => {
                let mut state = state.clone();
                state.last_error = Some(AppError::Repository(err.clone()));
                state.task_list_state.error = Some(AppError::Repository(err));
                (Some(state), None)
            }
            Event::Quit => {
                let mut state = state.clone();
                state.is_running = false;
                (Some(state), None)
            }
            Event::FocusedPane(pane) => {
                let mut state = state.clone();
                state.focus = pane;
                (Some(state), None)
            }
            Event::FullScreenOff => {
                if !state.fullscreen_pane {
                    return (None, None);
                }
                let mut state = state.clone();
                state.fullscreen_pane = false;
                (Some(state), None)
            }
            Event::FullScreenOn => {
                if state.fullscreen_pane {
                    return (None, None);
                }
                let mut state = state.clone();
                state.fullscreen_pane = true;
                (Some(state), None)
            }
            Event::Searched {
                query,
                cursor_position,
            } => {
                if query == state.search.query {
                    return (None, None);
                }
                let mut state = state.clone();
                let query_len = query.len();
                state.search.query = query.clone();
                state.search.cursor_pos = cursor_position.clamp(0, query_len);

                // Perform fuzzy search using search module
                state.task_list_state.filtered_task_ids = fuzzy_search_tasks(&query, &state.tasks);

                (Some(state), None)
            }
            Event::ChangedCursorPosition(pos) => {
                let pos = pos.clamp(0, state.search.query.len());
                if pos == state.search.cursor_pos {
                    return (None, None);
                }
                let mut state = state.clone();
                state.search.cursor_pos = pos;
                (Some(state), None)
            }
            Event::ClearedSearch => {
                if state.search.query.is_empty() {
                    return (None, None);
                }
                let mut state = state.clone();
                state.search.query = "".to_owned();
                state.search.cursor_pos = 0;
                state.focus = Pane::TaskList;
                (Some(state), None)
            }
        }
    }
}

fn handle_event(state: &State) -> Result<Option<Event>> {
    if crossterm::event::poll(Duration::from_millis(16))? {
        let terminal_event = crossterm::event::read()?;

        // Handle terminal resize events first
        if let crossterm::event::Event::Resize(_, _) = terminal_event {
            return Ok(Some(Event::TerminalResized));
        }

        let event = match state.focus {
            Pane::Comments => CommentsPane::handle_terminal_event(state, &terminal_event),
            Pane::Description => DescriptionPane::handle_terminal_event(state, &terminal_event),
            Pane::SearchBar => SearchBar::handle_terminal_event(state, &terminal_event),
            Pane::TaskList => TaskListPane::handle_terminal_event(state, &terminal_event),
        };
        if event.is_some() {
            return Ok(event);
        }
        let event = match state.view {
            View::TaskList => TaskView::handle_terminal_event(state, terminal_event),
        };
        return Ok(event);
    }
    Ok(None)
}

fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let mut stdout = std::io::stdout();
        crossterm::execute!(stdout, crossterm::terminal::LeaveAlternateScreen).unwrap();
        disable_raw_mode().unwrap();
        original_hook(panic_info);
    }));
}

fn init_terminal() -> Result<Terminal<impl Backend>> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok(terminal)
}

fn restore_terminal() -> Result<()> {
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

pub async fn run_tui(asana_client: AsanaClient) -> Result<()> {
    install_panic_hook();
    let mut terminal = init_terminal()?;
    let mut state = State::default();
    let (tx, mut rx) = mpsc::unbounded_channel::<Event>();
    let app = App {
        task_repo: Arc::new(AsanaTaskRepository::new(asana_client)),
    };

    tx.send(Event::Init)?;

    while state.is_running {
        terminal.draw(|frame| view(&state, frame))?;
        let mut current_event = handle_event(&state)?;
        if current_event.is_none() {
            current_event = rx.try_recv().ok();
        }

        while let Some(event) = current_event {
            let (update_state, update_event) = app.update(&state, event, &tx).await;
            current_event = update_event;
            if let Some(s) = update_state {
                state = s;
            }
        }
    }

    restore_terminal()?;
    Ok(())
}
