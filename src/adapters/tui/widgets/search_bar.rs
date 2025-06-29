use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

pub struct SearchBar {
    query: String,
    cursor_position: usize,
    is_focused: bool,
}

impl SearchBar {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            cursor_position: 0,
            is_focused: false, // Start unfocused by default
        }
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.is_focused = focused;
    }


    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn insert_char(&mut self, c: char) {
        self.query.insert(self.cursor_position, c);
        self.cursor_position += c.len_utf8();
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            let mut chars: Vec<char> = self.query.chars().collect();
            if self.cursor_position <= chars.len() {
                chars.remove(self.cursor_position - 1);
                self.query = chars.into_iter().collect();
                self.cursor_position -= 1;
            }
        }
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.cursor_position = 0;
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let title = if self.is_focused {
            "Search (focused)"
        } else {
            "Search (press / to focus)"
        };

        let border_style = if self.is_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Gray)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let search_text = if self.query.is_empty() {
            if self.is_focused {
                "Type to search tasks..."
            } else {
                "Press / to search"
            }
        } else {
            &self.query
        };

        let text_style = if self.query.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };

        let paragraph = Paragraph::new(search_text)
            .block(block)
            .style(text_style);

        frame.render_widget(paragraph, area);

        // Render cursor if focused
        if self.is_focused && !self.query.is_empty() {
            let cursor_x = area.x + 1 + self.cursor_position as u16;
            let cursor_y = area.y + 1;
            
            if cursor_x < area.x + area.width - 1 {
                frame.set_cursor_position(ratatui::layout::Position { x: cursor_x, y: cursor_y });
            }
        }
    }
}