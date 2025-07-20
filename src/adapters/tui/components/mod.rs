pub mod comments_pane;
pub mod description_pane;
pub mod search_bar;
pub mod task_list_pane;

use ratatui::{crossterm, layout::Rect, Frame};

use crate::adapters::tui::Event;

pub trait Component {
    type State;

    fn render(state: &Self::State, frame: &mut Frame, area: Rect);

    fn handle_terminal_event(
        _state: &Self::State,
        _event: &crossterm::event::Event,
    ) -> Option<Event> {
        None
    }
}
