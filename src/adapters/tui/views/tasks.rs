use ratatui::{
    crossterm::{
        self,
        event::{KeyCode, KeyEvent, KeyModifiers},
    },
    prelude::*,
    Frame,
};

use crate::adapters::tui::{
    components::{
        comments_pane::CommentsPane, description_pane::DescriptionPane, search_bar::SearchBar,
        task_list_pane::TaskListPane, Component,
    },
    Event, Pane, State,
};

use super::View;

fn render_fullscreen_pane(state: &State, frame: &mut Frame, pane: Pane) {
    match pane {
        Pane::TaskList => {
            TaskListPane::render(&state, frame, frame.area());
        }
        // Pane::Description => {
        //     self.render_description_fullscreen(frame, frame.area());
        // }
        // Pane::Comments => {
        //     self.render_comments_fullscreen(frame, frame.area());
        // }
        _ => {}
    }
}

fn handle_key_event(state: &State, key: KeyEvent) -> Option<Event> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) => Some(Event::Quit),
        (KeyCode::Tab, KeyModifiers::NONE) => Some(Event::FocusedPane(match state.focus {
            Pane::Comments => Pane::TaskList,
            Pane::Description => Pane::Comments,
            Pane::SearchBar => Pane::TaskList,
            Pane::TaskList => Pane::Description,
        })),
        (KeyCode::Tab, KeyModifiers::SHIFT) => Some(Event::FocusedPane(match state.focus {
            Pane::Comments => Pane::Description,
            Pane::Description => Pane::TaskList,
            Pane::SearchBar => Pane::TaskList,
            Pane::TaskList => Pane::Comments,
        })),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Event::Quit),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskView;

impl View for TaskView {
    fn handle_terminal_event(state: &State, event: crossterm::event::Event) -> Option<Event> {
        match event {
            crossterm::event::Event::Key(key) => handle_key_event(state, key),
            _ => None,
        }
    }

    fn render(state: &State, frame: &mut Frame) {
        // Check if we're in fullscreen mode
        if state.fullscreen_pane {
            return render_fullscreen_pane(state, frame, state.focus);
        }

        // Normal 3-pane layout
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search bar
                Constraint::Min(0),    // Main content (split left/right)
                Constraint::Length(1), // Status bar
            ])
            .split(frame.area());

        SearchBar::render(state, frame, main_chunks[0]);

        // Split main content area: task list (left) | right side
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40), // Task list (left pane)
                Constraint::Percentage(60), // Right side (description + comments)
            ])
            .split(main_chunks[1]);

        TaskListPane::render(state, frame, content_chunks[0]);

        // Split right side vertically: description (top) | comments (bottom)
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(60), // Description pane
                Constraint::Percentage(40), // Comments pane
            ])
            .split(content_chunks[1]);

        DescriptionPane::render(state, frame, right_chunks[0]);
        CommentsPane::render(state, frame, right_chunks[1]);
        // self.render_description_pane_standalone(frame, right_chunks[0]);

        // Render comments pane (right bottom)
        // self.render_comments_pane_standalone(frame, right_chunks[1]);

        // Render status bar
        // self.render_status_bar(frame, main_chunks[2]);

        // Render help modal if active
        // if matches!(self.mode, AppMode::Help) {
        //     self.render_help(frame);
        // }
    }
}
