pub mod tasks;

use ratatui::{crossterm, Frame};

use crate::adapters::tui::{Event, State};

pub trait View {
    fn render(state: &State, frame: &mut Frame);

    fn handle_terminal_event(_state: &State, _event: crossterm::event::Event) -> Option<Event> {
        None
    }
}
