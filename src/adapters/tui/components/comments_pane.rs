use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

use crate::adapters::tui::{components::Component, theme::Theme, State};

const NO_TASK_SELECTED: &str = "No task selected";
const NO_COMMENTS: &str = "No comments or activity";
const LOADING_COMMENTS: &str = "Loading comments...";

pub struct CommentsPane;

impl Component for CommentsPane {
    type State = State;

    fn render(
        state: &Self::State,
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

        let content = Self::get_content(state);
        let paragraph = Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: false });

        frame.render_widget(paragraph, area);
    }
}

impl CommentsPane {
    fn get_content(state: &State) -> Vec<Line<'static>> {
        if state.selected_task().is_none() {
            return vec![Line::from(Span::styled(
                NO_TASK_SELECTED,
                Style::default().fg(Color::Gray),
            ))];
        }

        if state.is_loading_comments() {
            return vec![Line::from(Span::styled(
                LOADING_COMMENTS,
                Style::default().fg(Color::Yellow),
            ))];
        }

        if let Some(error) = state.comment_load_error() {
            return vec![Line::from(Span::styled(
                format!("Error loading comments: {error}"),
                Style::default().fg(Color::Red),
            ))];
        }

        if let Some(comments) = state.selected_task_comments() {
            if comments.is_empty() {
                return vec![Line::from(Span::styled(
                    NO_COMMENTS,
                    Style::default().fg(Color::Gray),
                ))];
            }

            Self::format_comments(comments)
        } else {
            vec![Line::from(Span::styled(
                NO_COMMENTS,
                Style::default().fg(Color::Gray),
            ))]
        }
    }

    fn format_comments(comments: &[crate::domain::comment::Comment]) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Separate comments from system activity
        let mut user_comments = Vec::new();
        let mut system_activity = Vec::new();

        for comment in comments {
            match comment.story_type.as_deref() {
                Some("comment") => user_comments.push(comment),
                Some("system") => system_activity.push(comment),
                _ => system_activity.push(comment), // Default to system if unclear
            }
        }

        // Render user comments first
        if !user_comments.is_empty() {
            lines.push(Line::from(Span::styled(
                "Comments",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            for comment in &user_comments {
                let author_name = comment
                    .author
                    .as_ref()
                    .map(|u| u.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                let time_display = comment.created_at.format("%Y-%m-%d %H:%M").to_string();

                // Header: Name • timestamp
                lines.push(Line::from(vec![
                    Span::styled(author_name, Style::default().fg(Color::Blue)),
                    Span::styled(
                        format!(" • {time_display}"),
                        Style::default().fg(Color::Gray),
                    ),
                ]));

                if let Some(ref text) = comment.text {
                    // Simple text rendering for now - can be enhanced later
                    let cleaned_text = text.replace("<br>", "\n").replace("&nbsp;", " ");
                    for line in cleaned_text.lines() {
                        if !line.trim().is_empty() {
                            lines.push(Line::from(line.to_string()));
                        }
                    }
                }
                lines.push(Line::from(""));
            }
        }

        // Render system activity
        if !system_activity.is_empty() {
            lines.push(Line::from(Span::styled(
                "Activity",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            for activity in &system_activity {
                let time_display = activity.created_at.format("%Y-%m-%d %H:%M").to_string();
                let text = activity
                    .text
                    .as_ref()
                    .map(|t| t.replace("<br>", " ").replace("&nbsp;", " "))
                    .unwrap_or_else(|| "[No text content]".to_string());

                lines.push(Line::from(vec![
                    Span::styled("• ", Style::default().fg(Color::Blue)),
                    Span::styled(text, Style::default()),
                    Span::styled(
                        format!(" ({time_display})"),
                        Style::default().fg(Color::Gray),
                    ),
                ]));
            }
        }

        lines
    }
}
