use ratatui::widgets::{Block, Paragraph};

use crate::adapters::tui::{components::Component, theme::Theme, State};

// const NO_COMMENTS: &str = "No comments available";

pub struct CommentsPane;

impl Component for CommentsPane {
    type State = State;

    fn render(
        _state: &Self::State,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
        theme: Theme,
    ) {
        let title = "Activity";
        let block = Block::default()
            .title(title)
            .borders(theme.borders)
            .border_type(theme.border_type)
            .border_style(theme.border_style);
        let paragraph = Paragraph::new("Comments and stuff...").block(block); //.style(text_style)

        frame.render_widget(paragraph, area);
        // if let Some(task) = state.selected_task() {
        //     // let description = task
        //     //     .description
        //     //     .clone()
        //     //     .or_else(|| Some(NO_DESCRIPTION.to_owned()))
        //     //     .unwrap();
        //     frame.render_widget(Paragraph::new("Comments..."), area);
        // }
    }
}

// fn render_comments_pane_standalone(&mut self, frame: &mut Frame, area: Rect) {
//     // Determine border style based on focus
//     let border_style = if self.focused_pane == FocusedPane::Comments {
//         Style::default().fg(Color::Green)
//     } else {
//         Style::default().fg(Color::Gray)
//     };
//
//     let title = "Comments & Activity";
//
//     // Get currently selected task
//     let selected_task = self
//         .task_list_state
//         .selected()
//         .and_then(|i| self.filtered_tasks.get(i));
//
//     if let Some(task) = selected_task {
//         if let Some(current_task) = &self.current_task {
//             if current_task.id == task.id {
//                 // Show comments using existing render logic
//                 let block = Block::default()
//                     .title(title)
//                     .borders(Borders::ALL)
//                     .border_type(BorderType::Rounded)
//                     .border_style(border_style);
//
//                 let inner_area = block.inner(area);
//                 frame.render_widget(block, area);
//
//                 // Render comments content
//                 self.render_comments_content_only(frame, inner_area);
//             } else {
//                 // Loading different task
//                 self.render_loading_placeholder(frame, area, title, border_style);
//             }
//         } else {
//             // No task loaded
//             self.render_loading_placeholder(frame, area, title, border_style);
//         }
//     } else {
//         // No task selected
//         self.render_empty_placeholder(frame, area, title, border_style, "No task selected");
//     }
// }
