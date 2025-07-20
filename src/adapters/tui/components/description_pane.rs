use ratatui::{
    prelude::*,
    widgets::{Block, Paragraph},
};

use crate::adapters::tui::{components::Component, theme::Theme, State};

pub struct DescriptionPane;

impl Component for DescriptionPane {
    type State = State;

    fn render(state: &Self::State, frame: &mut Frame, area: Rect, theme: Theme) {
        let title = "Description";

        let block = Block::default()
            .title(title)
            .borders(theme.borders)
            .border_type(theme.border_type)
            .border_style(theme.border_style);

        let Some(task) = state.selected_task() else {
            let paragraph = Paragraph::new("No task selected")
                .block(block)
                .style(Style::new().dark_gray());
            frame.render_widget(paragraph, area);
            return;
        };

        let Some(description) = &task.description else {
            let paragraph = Paragraph::new("No description")
                .block(block)
                .style(Style::new().red());
            frame.render_widget(paragraph, area);
            return;
        };

        let paragraph = Paragraph::new(description.clone()).block(block);

        frame.render_widget(paragraph, area);
    }
}

// fn render_description_pane_standalone(&mut self, frame: &mut Frame, area: Rect) {
//     if let Some(task) = selected_task {
//         if let Some(current_task) = self.current_task.clone() {
//             if current_task.id == task.id {
//                 // Show task description using existing render logic
//                 let block = Block::default()
//                     .title(title)
//                     .borders(Borders::ALL)
//                     .border_type(BorderType::Rounded)
//                     .border_style(border_style);
//
//                 let inner_area = block.inner(area);
//                 frame.render_widget(block, area);
//
//                 // Render description content
//                 self.render_description_content_only(frame, inner_area, &current_task);
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

// fn render_description_content_only(&mut self, frame: &mut Frame, area: Rect, task: &Task) {
//         // Check if we need to regenerate the cache
//         if self.cached_description_lines.is_none() {
//             // Generate and cache the lines
//             let mut lines: Vec<md::MarkdownLine> = Vec::new();
//
//             // Add task info section
//             let (status_text, status_color) = task.status_display();
//             let status_style = match status_color {
//                 "red" => Style::default().fg(Color::Red),
//                 "yellow" => Style::default().fg(Color::Yellow),
//                 "green" => Style::default().fg(Color::Green),
//                 "gray" => Style::default().fg(Color::Gray),
//                 _ => Style::default(),
//             };
//
//             lines.push(md::MarkdownLine {
//                 line: Line::from(vec![
//                     Span::styled("Status: ", Style::default().fg(Color::Cyan)),
//                     Span::styled(status_text, status_style),
//                 ]),
//                 is_code_block: false,
//             });
//
//             let due_text = task.due_date_display();
//             let due_style = if task.is_overdue() {
//                 Style::default().fg(Color::Red)
//             } else {
//                 Style::default()
//             };
//             lines.push(md::MarkdownLine {
//                 line: Line::from(vec![
//                     Span::styled("Due: ", Style::default().fg(Color::Cyan)),
//                     Span::styled(due_text, due_style),
//                 ]),
//                 is_code_block: false,
//             });
//
//             if task.assignee.is_some() {
//                 let assignee_display = task.assignee_name.as_deref().unwrap_or("Unknown User");
//                 lines.push(md::MarkdownLine {
//                     line: Line::from(vec![
//                         Span::styled("Assignee: ", Style::default().fg(Color::Cyan)),
//                         Span::raw(assignee_display.to_string()),
//                     ]),
//                     is_code_block: false,
//                 });
//             }
//
//             // Add projects with colored labels
//             if !task.projects.is_empty() {
//                 let mut project_spans =
//                     vec![Span::styled("Projects: ", Style::default().fg(Color::Cyan))];
//                 for (i, project) in task.projects.iter().enumerate() {
//                     if i > 0 {
//                         project_spans.push(Span::raw(" "));
//                     }
//
//                     // Create colored label with proper contrast
//                     let bg_color = if let Some(color) = &project.color {
//                         asana_color_to_ratatui(color)
//                     } else {
//                         Color::Gray
//                     };
//
//                     project_spans.extend(create_colored_label(&project.name, bg_color));
//                 }
//                 lines.push(md::MarkdownLine {
//                     line: Line::from(project_spans),
//                     is_code_block: false,
//                 });
//             }
//
//             // Add custom fields
//             if !task.custom_fields.is_empty() {
//                 for custom_field in &task.custom_fields {
//                     if let Some(display_value) = &custom_field.display_value {
//                         if !display_value.is_empty() {
//                             let mut field_spans = vec![Span::styled(
//                                 format!("{}: ", custom_field.name),
//                                 Style::default().fg(Color::Cyan),
//                             )];
//
//                             // Check if it's an enum value with color
//                             if let Some(enum_value) = &custom_field.enum_value {
//                                 if let Some(color) = &enum_value.color {
//                                     let value_style =
//                                         Style::default().fg(asana_color_to_ratatui(color));
//                                     field_spans
//                                         .push(Span::styled(display_value.clone(), value_style));
//                                 } else {
//                                     field_spans.push(Span::raw(display_value.clone()));
//                                 }
//                             } else {
//                                 field_spans.push(Span::raw(display_value.clone()));
//                             }
//
//                             lines.push(md::MarkdownLine {
//                                 line: Line::from(field_spans),
//                                 is_code_block: false,
//                             });
//                         }
//                     }
//                 }
//             }
//
//             // Add dependencies
//             if !task.dependencies.is_empty() {
//                 let mut dep_spans = vec![Span::styled(
//                     "Dependencies: ",
//                     Style::default().fg(Color::Cyan),
//                 )];
//                 for (i, dependency) in task.dependencies.iter().enumerate() {
//                     if i > 0 {
//                         dep_spans.push(Span::raw(", "));
//                     }
//                     dep_spans.push(Span::raw(dependency.name.clone()));
//                 }
//                 lines.push(md::MarkdownLine {
//                     line: Line::from(dep_spans),
//                     is_code_block: false,
//                 });
//             }
//
//             // Add blank line separator
//             lines.push(md::MarkdownLine {
//                 line: Line::from(""),
//                 is_code_block: false,
//             });
//
//             // Add description if present
//             if let Some(description) = &task.description {
//                 if !description.trim().is_empty() {
//                     let markdown_desc = md::html_to_markdown(description);
//                     let styled_lines = md::parse_markdown_to_marked_lines_with_wrapping(
//                         &markdown_desc,
//                         Some(area.width),
//                     );
//
//                     lines.extend(styled_lines);
//                 }
//             } else {
//                 lines.push(md::MarkdownLine {
//                     line: Line::from(vec![Span::styled(
//                         "No description available",
//                         Style::default().fg(Color::Gray),
//                     )]),
//                     is_code_block: false,
//                 });
//             }
//
//             // Cache the generated lines
//             self.cached_description_lines = Some(lines);
//         }
//
//         // Use cached lines for rendering
//         if let Some(cached_lines) = &self.cached_description_lines {
//             // Apply scrolling - skip lines based on scroll offset
//             let visible_lines: Vec<&md::MarkdownLine> = cached_lines
//                 .iter()
//                 .skip(self.description_scroll_offset as usize)
//                 .collect();
//
//             // Render lines with special handling for code blocks
//             let mut y = 0;
//             for marked_line in visible_lines {
//                 if y >= area.height {
//                     break;
//                 }
//
//                 // Create a sub-area for this line
//                 let line_area = Rect {
//                     x: area.x,
//                     y: area.y + y,
//                     width: area.width,
//                     height: 1,
//                 };
//
//                 if marked_line.is_code_block {
//                     // Render code blocks without wrapping
//                     let paragraph = Paragraph::new(marked_line.line.clone())
//                         .alignment(ratatui::layout::Alignment::Left);
//                     frame.render_widget(paragraph, line_area);
//                 } else {
//                     // Render regular lines with wrapping
//                     let paragraph = Paragraph::new(marked_line.line.clone())
//                         .wrap(Wrap { trim: false })
//                         .alignment(ratatui::layout::Alignment::Left);
//                     frame.render_widget(paragraph, line_area);
//                 }
//
//                 y += 1;
//             }
//         }
//     }
