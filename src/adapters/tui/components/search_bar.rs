use ratatui::{
    crossterm::{
        self,
        event::{KeyCode, KeyEvent, KeyModifiers},
    },
    prelude::*,
    widgets::{Block, Paragraph},
};

use crate::adapters::tui::{components::Component, theme::Theme, Event, Pane, State};

fn get_text_style(query: &str) -> Style {
    if query.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    }
}

fn handle_key_event(state: &State, key: &KeyEvent) -> Option<Event> {
    let mut buf = [0u8; 4];
    match (key.code, key.modifiers) {
        (KeyCode::Backspace, _) => {
            if state.search.query.is_empty() || state.search.cursor_pos == 0 {
                return None;
            }
            let mut query = state.search.query.clone();
            query.remove(state.search.cursor_pos - 1);
            Some(Event::Searched {
                query,
                cursor_position: state.search.cursor_pos - 1,
            })
        }
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
            if state.search.query.is_empty() || state.search.cursor_pos == 0 {
                return None;
            }
            let mut query = state.search.query.clone();
            let cursor_pos = state.search.cursor_pos;

            if cursor_pos == 0 {
                return None;
            }

            // Find the start of the previous word
            let chars: Vec<char> = query.chars().collect();
            let mut pos = cursor_pos.saturating_sub(1);

            // Skip trailing whitespace
            while pos > 0 && chars.get(pos).is_some_and(|c| c.is_whitespace()) {
                pos -= 1;
            }

            // Delete the word characters
            while pos > 0 && chars.get(pos).is_some_and(|c| !c.is_whitespace()) {
                pos -= 1;
            }

            // If we stopped at whitespace and we're not at the beginning, move forward one
            if pos > 0 && chars.get(pos).is_some_and(|c| c.is_whitespace()) {
                pos += 1;
            }

            // Remove the characters from pos to cursor_pos
            query.drain(pos..cursor_pos);

            Some(Event::Searched {
                query,
                cursor_position: pos,
            })
        }
        (KeyCode::Left, _) => Some(Event::ChangedCursorPosition(state.search.cursor_pos - 1)),
        (KeyCode::Right, _) => Some(Event::ChangedCursorPosition(state.search.cursor_pos + 1)),
        (KeyCode::Esc, _) => Some(Event::ClearedSearch),
        (KeyCode::Enter, _) => Some(Event::FocusedPane(Pane::TaskList)),
        (KeyCode::Char(c), _) => Some(Event::Searched {
            query: state.search.query.clone() + c.encode_utf8(&mut buf),
            cursor_position: state.search.cursor_pos + 1,
        }),
        (KeyCode::Up, _) => Some(Event::SelectedTask(
            state
                .task_list_state
                .selected_row_index
                .map(|x| x.saturating_sub(1)),
        )),
        (KeyCode::Down, _) => Some(Event::SelectedTask(
            state.task_list_state.selected_row_index.map(|x| x + 1),
        )),
        _ => None,
    }
}

pub struct SearchBar;

impl Component for SearchBar {
    type State = State;

    fn handle_terminal_event(state: &State, event: &crossterm::event::Event) -> Option<Event> {
        match event {
            crossterm::event::Event::Key(key) => handle_key_event(state, key),
            _ => None,
        }
    }

    fn render(state: &Self::State, frame: &mut Frame, area: Rect, theme: Theme) {
        let is_focused = state.focus == Pane::SearchBar;
        let title = "Search";
        let block = Block::default()
            .title(title)
            .borders(theme.borders)
            .border_type(theme.border_type)
            .border_style(theme.border_style);

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

        if is_focused {
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
