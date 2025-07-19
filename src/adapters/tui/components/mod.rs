pub mod comments_pane;
pub mod description_pane;
pub mod search_bar;
pub mod task_list_pane;

use ratatui::{layout::Rect, Frame};

pub trait Component {
    type State;

    fn render(state: &Self::State, frame: &mut Frame, area: Rect);
}
