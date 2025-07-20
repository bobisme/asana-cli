use ratatui::{
    crossterm::{
        self,
        event::{KeyCode, KeyEvent, KeyModifiers},
    },
    prelude::*,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

use crate::{
    adapters::tui::{components::Component, md, theme::Theme, Direction, Event, State},
    domain::task::Task,
};

fn asana_color_to_ratatui(asana_color: &str) -> Color {
    match asana_color {
        "red" => Color::Red,
        "orange" => Color::LightRed,
        "yellow" => Color::Yellow,
        "yellow_green" => Color::LightGreen,
        "green" => Color::Green,
        "turquoise" => Color::Cyan,
        "light_blue" => Color::LightBlue,
        "blue" => Color::Blue,
        "purple" => Color::Magenta,
        "pink" => Color::LightMagenta,
        "brown" => Color::LightRed,
        "dark_red" => Color::Red,
        "dark_orange" => Color::Red,
        "dark_yellow" => Color::Yellow,
        "dark_green" => Color::Green,
        "dark_blue" => Color::Blue,
        "dark_purple" => Color::Magenta,
        "dark_pink" => Color::Magenta,
        "dark_brown" => Color::Red,
        _ => Color::Gray,
    }
}

fn create_colored_label(text: &str, bg_color: Color) -> Vec<Span<'static>> {
    let fg_color = match bg_color {
        Color::Yellow
        | Color::LightGreen
        | Color::Cyan
        | Color::LightBlue
        | Color::LightMagenta => Color::Black,
        _ => Color::White,
    };

    vec![
        Span::raw("["),
        Span::styled(text.to_string(), Style::default().fg(fg_color).bg(bg_color)),
        Span::raw("]"),
    ]
}

fn build_task_info_lines(task: &Task) -> Vec<md::MarkdownLine> {
    let mut lines: Vec<md::MarkdownLine> = Vec::new();

    // Add task status with color
    let (status_text, status_color) = task.status_display();
    let status_style = match status_color {
        "red" => Style::default().fg(Color::Red),
        "yellow" => Style::default().fg(Color::Yellow),
        "green" => Style::default().fg(Color::Green),
        "gray" => Style::default().fg(Color::Gray),
        _ => Style::default(),
    };

    lines.push(md::MarkdownLine {
        line: Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Cyan)),
            Span::styled(status_text.to_string(), status_style),
        ]),
        is_code_block: false,
    });

    // Add due date with overdue highlighting
    let due_text = task.due_date_display();
    let due_style = if task.is_overdue() {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    lines.push(md::MarkdownLine {
        line: Line::from(vec![
            Span::styled("Due: ", Style::default().fg(Color::Cyan)),
            Span::styled(due_text, due_style),
        ]),
        is_code_block: false,
    });

    // Add assignee if present
    if task.assignee.is_some() {
        let assignee_display = task.assignee_name.as_deref().unwrap_or("Unknown User");
        lines.push(md::MarkdownLine {
            line: Line::from(vec![
                Span::styled("Assignee: ", Style::default().fg(Color::Cyan)),
                Span::raw(assignee_display.to_string()),
            ]),
            is_code_block: false,
        });
    }

    // Add projects with colored labels
    if !task.projects.is_empty() {
        let mut project_spans = vec![Span::styled("Projects: ", Style::default().fg(Color::Cyan))];
        for (i, project) in task.projects.iter().enumerate() {
            if i > 0 {
                project_spans.push(Span::raw(" "));
            }

            // Create colored label with proper contrast
            let bg_color = if let Some(color) = &project.color {
                asana_color_to_ratatui(color)
            } else {
                Color::Gray
            };

            project_spans.extend(create_colored_label(&project.name, bg_color));
        }
        lines.push(md::MarkdownLine {
            line: Line::from(project_spans),
            is_code_block: false,
        });
    }

    // Add custom fields
    if !task.custom_fields.is_empty() {
        for custom_field in &task.custom_fields {
            if let Some(display_value) = &custom_field.display_value {
                if !display_value.is_empty() {
                    let mut field_spans = vec![Span::styled(
                        format!("{}: ", custom_field.name),
                        Style::default().fg(Color::Cyan),
                    )];

                    // Check if it's an enum value with color
                    if let Some(enum_value) = &custom_field.enum_value {
                        if let Some(color) = &enum_value.color {
                            let value_style = Style::default().fg(asana_color_to_ratatui(color));
                            field_spans.push(Span::styled(display_value.clone(), value_style));
                        } else {
                            field_spans.push(Span::raw(display_value.clone()));
                        }
                    } else {
                        field_spans.push(Span::raw(display_value.clone()));
                    }

                    lines.push(md::MarkdownLine {
                        line: Line::from(field_spans),
                        is_code_block: false,
                    });
                }
            }
        }
    }

    // Add dependencies
    if !task.dependencies.is_empty() {
        let mut dep_spans = vec![Span::styled(
            "Dependencies: ",
            Style::default().fg(Color::Cyan),
        )];
        for (i, dependency) in task.dependencies.iter().enumerate() {
            if i > 0 {
                dep_spans.push(Span::raw(", "));
            }
            dep_spans.push(Span::raw(dependency.name.clone()));
        }
        lines.push(md::MarkdownLine {
            line: Line::from(dep_spans),
            is_code_block: false,
        });
    }

    // Add blank line separator
    lines.push(md::MarkdownLine {
        line: Line::from(""),
        is_code_block: false,
    });

    lines
}

pub fn generate_description_cache(task: &Task, area_width: u16) -> Vec<md::MarkdownLine> {
    let mut lines = Vec::new();

    // Add task info section
    lines.extend(build_task_info_lines(task));

    // Add description if present
    if let Some(description) = &task.description {
        if !description.trim().is_empty() {
            let markdown_desc = md::html_to_markdown(description);
            let styled_lines =
                md::parse_markdown_to_marked_lines_with_wrapping(&markdown_desc, Some(area_width));

            lines.extend(styled_lines);
        }
    } else {
        lines.push(md::MarkdownLine {
            line: Line::from(vec![Span::styled(
                "No description available",
                Style::default().fg(Color::Gray),
            )]),
            is_code_block: false,
        });
    }

    lines
}

fn render_description_content(
    frame: &mut Frame,
    area: Rect,
    cached_lines: &[md::MarkdownLine],
    scroll_offset: usize,
) {
    // Apply scrolling - skip lines based on scroll offset
    let visible_lines: Vec<&md::MarkdownLine> = cached_lines.iter().skip(scroll_offset).collect();

    // Render lines with special handling for code blocks
    for (y, marked_line) in visible_lines.into_iter().enumerate() {
        let y = y as u16;
        if y >= area.height {
            break;
        }

        // Create a sub-area for this line
        let line_area = Rect {
            x: area.x,
            y: area.y + y,
            width: area.width,
            height: 1,
        };

        if marked_line.is_code_block {
            // Render code blocks without wrapping
            let paragraph = Paragraph::new(marked_line.line.clone())
                .alignment(ratatui::layout::Alignment::Left);
            frame.render_widget(paragraph, line_area);
        } else {
            // Render regular lines with wrapping
            let paragraph = Paragraph::new(marked_line.line.clone())
                .wrap(Wrap { trim: false })
                .alignment(ratatui::layout::Alignment::Left);
            frame.render_widget(paragraph, line_area);
        }
    }
}

pub struct DescriptionPane;

fn handle_description_key_event(state: &State, key: &KeyEvent) -> Option<Event> {
    match (key.code, key.modifiers) {
        (KeyCode::Down, KeyModifiers::NONE) => Some(Event::ScrollDescription {
            dir: Direction::Down,
            count: 1,
        }),
        (KeyCode::Down, KeyModifiers::SHIFT) => Some(Event::ScrollDescription {
            dir: Direction::Down,
            count: 10,
        }),
        (KeyCode::Up, KeyModifiers::NONE) => Some(Event::ScrollDescription {
            dir: Direction::Up,
            count: 1,
        }),
        (KeyCode::Up, KeyModifiers::SHIFT) => Some(Event::ScrollDescription {
            dir: Direction::Up,
            count: 10,
        }),
        (KeyCode::PageDown, KeyModifiers::NONE) => {
            let visible_height = state.description_state.visible_height;
            Some(Event::ScrollDescription {
                dir: Direction::Down,
                count: visible_height.saturating_sub(1).max(1),
            })
        }
        (KeyCode::PageUp, KeyModifiers::NONE) => {
            let visible_height = state.description_state.visible_height;
            Some(Event::ScrollDescription {
                dir: Direction::Up,
                count: visible_height.saturating_sub(1).max(1),
            })
        }
        (KeyCode::Char('j'), KeyModifiers::NONE) => Some(Event::ScrollDescription {
            dir: Direction::Down,
            count: 1,
        }),
        (KeyCode::Char('j'), KeyModifiers::CONTROL) => Some(Event::ScrollDescription {
            dir: Direction::Down,
            count: 10,
        }),
        (KeyCode::Char('k'), KeyModifiers::NONE) => Some(Event::ScrollDescription {
            dir: Direction::Up,
            count: 1,
        }),
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => Some(Event::ScrollDescription {
            dir: Direction::Up,
            count: 10,
        }),
        _ => None,
    }
}

impl Component for DescriptionPane {
    type State = State;

    fn handle_terminal_event(
        state: &Self::State,
        event: &crossterm::event::Event,
    ) -> Option<Event> {
        match event {
            crossterm::event::Event::Key(key) => handle_description_key_event(state, key),
            _ => None,
        }
    }

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

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Check if we have cached description lines for the current task
        if let Some(cached_lines) = &state.description_state.cached_description_lines {
            // Use cached lines
            render_description_content(
                frame,
                inner_area,
                cached_lines,
                state.description_scroll_offset(),
            );
        } else {
            // No cache available, generate lines on the fly for this render
            let area_width = inner_area.width.saturating_sub(2);
            let lines = generate_description_cache(task, area_width);

            render_description_content(
                frame,
                inner_area,
                &lines,
                state.description_scroll_offset(),
            );
        }
    }
}
