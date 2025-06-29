use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;
use color_eyre::Result;

#[derive(Debug, Clone)]
pub enum AppEvent {
    // Navigation
    Quit,
    CloseModal,
    
    // Search
    FocusSearch,
    
    // Task list navigation
    NextTask,
    PreviousTask,
    
    // Task detail
    ScrollDetailPageUp,
    ScrollDetailPageDown,
    
    // Input handling
    Character(char),
    Backspace,
    Enter,
    Tab,
    BackTab,
}

pub struct EventHandler {
    should_quit: bool,
}

impl EventHandler {
    pub fn new() -> Self {
        Self { should_quit: false }
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub async fn next_event(&mut self) -> Result<Option<AppEvent>> {
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key_event) => Ok(self.handle_key_event(key_event)),
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) -> Option<AppEvent> {
        match key_event {
            // Global quit with Ctrl+C
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.should_quit = true;
                Some(AppEvent::Quit)
            }
            
            // 'q' for context-sensitive quit/close
            KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::Character('q')),
            
            // 'r' is handled in Character processing to allow search input
            
            // '?' is handled in Character processing to allow search input
            
            KeyEvent {
                code: KeyCode::Char('/'),
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::FocusSearch),
            
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::CloseModal),
            
            // Navigation
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::Tab),
            
            KeyEvent {
                code: KeyCode::BackTab,
                modifiers: KeyModifiers::SHIFT,
                ..
            } => Some(AppEvent::BackTab),
            
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::Enter),
            
            // Arrow key navigation (always works)
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::NextTask),
            
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::PreviousTask),
            
            // Vim-style navigation as characters (context-sensitive)
            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::Character('j')),
            
            KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::Character('k')),
            
            KeyEvent {
                code: KeyCode::Char('g'),
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::Character('g')),
            
            KeyEvent {
                code: KeyCode::Char('G'),
                modifiers: KeyModifiers::SHIFT,
                ..
            } => Some(AppEvent::Character('G')),
            
            // ' ' (space) is handled in Character processing to allow search input
            
            // Detail view navigation
            KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } | KeyEvent {
                code: KeyCode::PageUp,
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::ScrollDetailPageUp),
            
            KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } | KeyEvent {
                code: KeyCode::PageDown,
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::ScrollDetailPageDown),
            
            // 'c' is handled in Character processing to allow search input
            
            // Input characters
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::Character(c)),
            
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::SHIFT,
                ..
            } => Some(AppEvent::Character(c.to_uppercase().next().unwrap_or(c))),
            
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => Some(AppEvent::Backspace),
            
            _ => None,
        }
    }
}