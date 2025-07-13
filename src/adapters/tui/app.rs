use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use color_eyre::Result;
use ratatui::crossterm;
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::widgets::Paragraph;
use ratatui::{
    crossterm::{
        event::KeyEvent,
        terminal::{disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    prelude::*,
};
use tokio::sync::mpsc;

use crate::adapters::api::{AsanaClient, AsanaTaskRepository};
use crate::app::error::{AppError, RepositoryError};
use crate::domain::comment::Comment;
use crate::domain::task::repo::TaskRepository;
use crate::domain::task::{Task, TaskFilter, TaskId};

#[derive(Debug, Clone, Copy)]
enum View {
    TaskList,
}

#[derive(Debug, Clone, Copy)]
enum Pane {
    TaskList,
    Task,
    Comments,
}

#[derive(Debug, Clone)]
enum Event {
    Init,
    Key(KeyEvent),
    ReceivedTasks(Vec<Task>),
    RequestError(RepositoryError),
    // Increment,
    // Decrement,
    // FetchData,
    // DataReceived(Result<String, String>), // Success(data) or Error(message)
    Quit,
}

#[derive(Debug, Clone)]
struct State {
    is_running: bool,
    view: View,
    focus: Pane,
    tasks: HashMap<TaskId, Task>,
    comments: HashMap<String, Comment>,
    last_error: Option<AppError>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            is_running: true,
            view: View::TaskList,
            focus: Pane::TaskList,
            tasks: Default::default(),
            comments: Default::default(),
            last_error: None,
        }
    }
}

fn view(state: &State, frame: &mut Frame) {
    let len = state.tasks.len();
    if state.last_error.is_some() {
        let err = state.last_error.clone().unwrap();
        frame.render_widget(Paragraph::new(format!("ERROR: {err}")), frame.area());
        return;
    }
    if len > 0 {
        frame.render_widget(Paragraph::new(format!("loaded {len}")), frame.area());
        return;
    }
    frame.render_widget(Paragraph::new("loading..."), frame.area());
}

fn handle_task_list_key(_state: &State, key: KeyEvent) -> (Option<State>, Option<Event>) {
    match key {
        KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            ..
        } => (None, Some(Event::Quit)),
        _ => (None, None),
    }
}

struct App<TaskRepo: TaskRepository> {
    task_repo: Arc<TaskRepo>,
}

impl<TaskRepo: TaskRepository> App<TaskRepo> {
    fn task_repository(&self) -> Arc<TaskRepo> {
        Arc::clone(&self.task_repo)
    }

    async fn update(
        &self,
        state: &State,
        event: Event,
        tx: &mpsc::UnboundedSender<Event>,
    ) -> (Option<State>, Option<Event>) {
        match event {
            Event::Init => {
                let repo = self.task_repository();
                let tx = tx.clone();
                tokio::spawn(async move {
                    let filter = TaskFilter::default();
                    let future = repo.list_tasks(&filter);
                    let event = match future.await {
                        Ok(tasks) => Event::ReceivedTasks(tasks),
                        Err(e) => Event::RequestError(e),
                    };
                    let _ = tx.send(event);
                });
                (None, None)
            }
            Event::ReceivedTasks(tasks) => {
                let mut state = state.clone();
                let mut map = HashMap::new();
                for task in tasks.iter() {
                    map.insert(task.id.clone(), task.clone());
                }
                state.tasks = map;
                (Some(state), None)
            }
            Event::RequestError(err) => {
                let mut state = state.clone();
                state.last_error = Some(AppError::Repository(err));
                (Some(state), None)
            }
            Event::Key(key) => match (state.view, state.focus) {
                (View::TaskList, Pane::TaskList) => handle_task_list_key(state, key),
                _ => (None, None),
            },
            Event::Quit => {
                let mut state = state.clone();
                state.is_running = false;
                (Some(state), None)
            }
        }
    }
}

fn handle_event() -> Result<Option<Event>> {
    if crossterm::event::poll(Duration::from_millis(16))? {
        if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
            if key.kind == crossterm::event::KeyEventKind::Press {
                return Ok(Some(Event::Key(key)));
            }
        }
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
    ratatui::crossterm::execute!(stdout, EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok(terminal)
}

fn restore_terminal() -> Result<()> {
    let mut stdout = std::io::stdout();
    ratatui::crossterm::execute!(stdout, LeaveAlternateScreen)?;
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
        let mut current_event = handle_event()?;
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
