use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::adapters::tui::{components::Component, Pane, State};

fn get_border_style(is_focused: bool) -> Style {
    if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn get_text_style(query: &str) -> Style {
    if query.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    }
}

pub struct SearchBar;

impl Component for SearchBar {
    type State = State;

    fn render(state: &Self::State, frame: &mut Frame, area: Rect) {
        let is_focused = state.focus == Pane::SearchBar;
        let title = "Search";
        let border_style = get_border_style(is_focused);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        let query = &state.search.query;

        let search_text = if query.is_empty() {
            if is_focused {
                "Type to search tasks..."
            } else {
                "Press / to search"
            }
        } else {
            query
        };

        let text_style = get_text_style(query);

        let paragraph = Paragraph::new(search_text).block(block).style(text_style);

        frame.render_widget(paragraph, area);

        // Render cursor if focused
        if is_focused && !query.is_empty() {
            let cursor_x = area.x + 1 + state.search.cursor_pos as u16;
            let cursor_y = area.y + 1;

            if cursor_x < area.x + area.width - 1 {
                frame.set_cursor_position(ratatui::layout::Position {
                    x: cursor_x,
                    y: cursor_y,
                });
            }
        }
    }

    // pub fn insert_char(&mut self, c: char) {
    //     self.query.insert(self.cursor_position, c);
    //     self.cursor_position += c.len_utf8();
    // }
    //
    // pub fn delete_char(&mut self) {
    //     if self.cursor_position > 0 {
    //         let mut chars: Vec<char> = self.query.chars().collect();
    //         if self.cursor_position <= chars.len() {
    //             chars.remove(self.cursor_position - 1);
    //             self.query = chars.into_iter().collect();
    //             self.cursor_position -= 1;
    //         }
    //     }
    // }
    //
    // pub fn clear(&mut self) {
    //     self.query.clear();
    //     self.cursor_position = 0;
    // }
}
