use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;
use color_eyre::Result;

#[derive(Debug, Clone)]
pub enum AppEvent {
    // Navigation
    Quit,
    Refresh,
    ShowHelp,
    CloseModal,
    
    // Search
    FocusSearch,
    UpdateSearch(String),
    ClearSearch,
    
    // Task list navigation
    FocusTaskList,
    NextTask,
    PreviousTask,
    FirstTask,
    LastTask,
    SelectTask,
    ToggleTaskComplete,
    
    // Task detail
    OpenTaskDetail(crate::domain::TaskId),
    CloseTaskDetail,
    ScrollDetailUp,
    ScrollDetailDown,
    ScrollDetailPageUp,
    ScrollDetailPageDown,
    ScrollDetailToTop,
    ScrollDetailToBottom,
    
    // Comment system
    StartComment,
    SubmitComment(String),
    CancelComment,
    
    // Input handling
    Character(char),
    Backspace,
    Delete,
    Enter,
    Tab,
    BackTab,
    
    // Other
    Tick,
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

    pub async fn next_event(&mut self) -> Result<AppEvent> {
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key_event) => Ok(self.handle_key_event(key_event)),
                _ => Ok(AppEvent::Tick),
            }
        } else {
            Ok(AppEvent::Tick)
        }
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) -> AppEvent {
        match key_event {
            // Global quit with Ctrl+C
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.should_quit = true;
                AppEvent::Quit
            }
            
            // 'q' for context-sensitive quit/close
            KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::Character('q'),
            
            KeyEvent {
                code: KeyCode::Char('r'),
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::Refresh,
            
            KeyEvent {
                code: KeyCode::Char('?'),
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::ShowHelp,
            
            KeyEvent {
                code: KeyCode::Char('/'),
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::FocusSearch,
            
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::CloseModal,
            
            // Navigation
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::Tab,
            
            KeyEvent {
                code: KeyCode::BackTab,
                modifiers: KeyModifiers::SHIFT,
                ..
            } => AppEvent::BackTab,
            
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::Enter,
            
            // Arrow key navigation (always works)
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::NextTask,
            
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::PreviousTask,
            
            // Vim-style navigation as characters (context-sensitive)
            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::Character('j'),
            
            KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::Character('k'),
            
            KeyEvent {
                code: KeyCode::Char('g'),
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::Character('g'),
            
            KeyEvent {
                code: KeyCode::Char('G'),
                modifiers: KeyModifiers::SHIFT,
                ..
            } => AppEvent::Character('G'),
            
            // Task actions
            KeyEvent {
                code: KeyCode::Char(' '),
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::ToggleTaskComplete,
            
            // Detail view navigation
            KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } | KeyEvent {
                code: KeyCode::PageUp,
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::ScrollDetailPageUp,
            
            KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } | KeyEvent {
                code: KeyCode::PageDown,
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::ScrollDetailPageDown,
            
            // Comments
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::StartComment,
            
            // Input characters
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::Character(c),
            
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::SHIFT,
                ..
            } => AppEvent::Character(c.to_uppercase().next().unwrap_or(c)),
            
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::Backspace,
            
            KeyEvent {
                code: KeyCode::Delete,
                modifiers: KeyModifiers::NONE,
                ..
            } => AppEvent::Delete,
            
            _ => AppEvent::Tick,
        }
    }
}