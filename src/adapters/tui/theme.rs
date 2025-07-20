use ratatui::{
    style::{Style, Stylize},
    widgets::{BorderType, Borders},
};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub borders: Borders,
    pub border_type: BorderType,
    pub border_style: Style,
}

impl Theme {
    pub fn focused() -> Self {
        Self {
            borders: Borders::ALL,
            border_type: BorderType::Rounded,
            border_style: Style::new().green(),
        }
    }

    pub fn full_screen() -> Self {
        Self {
            borders: Borders::NONE,
            border_type: BorderType::Rounded,
            border_style: Style::new(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            borders: Borders::ALL,
            border_type: BorderType::Rounded,
            border_style: Style::new().dark_gray(),
        }
    }
}
