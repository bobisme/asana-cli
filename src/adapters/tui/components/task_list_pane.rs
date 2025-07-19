use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState},
};

use crate::{
    adapters::tui::{components::Component, Pane},
    domain::task::Task,
};

fn task_icon(task: &Task) -> char {
    if task.is_milestone() {
        return '◇'; // Milestone
    } else {
        return '○'; // Task (open circle)
    }
}

fn task_icon_color(task: &Task) -> Color {
    if let Some(due_date) = task.due_date {
        let now = chrono::Utc::now();
        // Red if due before today (not including today)
        if due_date.date_naive() < now.date_naive() && !task.completed {
            Color::Red
        } else {
            Color::Green
        }
    } else {
        Color::Green
    }
}

pub struct TaskListPane;

impl Component for TaskListPane {
    type State = crate::adapters::tui::State;

    fn render(state: &Self::State, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let len = state.task_list_state.filtered_task_ids.len();
        let title = format!("Tasks ({len})");
        let border_style = if state.focused_pane == Pane::TaskList {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Gray)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border_style);

        if state.task_list_state.is_loading {
            let paragraph = Paragraph::new("Loading tasks...")
                .block(block)
                .style(Style::default().fg(Color::Gray));
            frame.render_widget(paragraph, area);
            return;
        }

        if let Some(error) = &state.task_list_state.error {
            let paragraph = Paragraph::new(error.to_string())
                .block(block)
                .style(Style::default().fg(Color::Red));
            frame.render_widget(paragraph, area);
            return;
        }

        if state.task_list_state.filtered_task_ids.is_empty() {
            let message = if state.search.query.is_empty() {
                "No tasks found"
            } else {
                "No tasks match your search"
            };
            let paragraph = Paragraph::new(message)
                .block(block)
                .style(Style::default().fg(Color::Gray));
            frame.render_widget(paragraph, area);
            return;
        }

        let rows: Vec<Row> = state
            .task_list_state
            .filtered_task_ids
            .iter()
            .flat_map(|id| state.tasks.get(id).clone())
            .map(|task| {
                let due_text = task.due_date_display();

                // Get icon and color based on task type and due date
                let icon = task_icon(task);
                let icon_color = task_icon_color(task);

                let icon_span = Span::styled(icon.to_string(), Style::default().fg(icon_color));
                let title_with_icon = vec![icon_span, Span::raw(" "), Span::raw(&task.name)];

                // Make due dates dark gray
                let due_style = Style::default().fg(Color::DarkGray);

                Row::new(vec![
                    Cell::from(Line::from(title_with_icon)),
                    Cell::from(due_text).style(due_style),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            &[
                Constraint::Min(20),    // Title column (flexible)
                Constraint::Length(12), // Due date column
            ],
        )
        .block(block)
        .row_highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("");

        let mut table_state = TableState::new();
        table_state.select(state.task_list_state.selected_row_index);

        frame.render_stateful_widget(table, area, &mut table_state);
    }
}
