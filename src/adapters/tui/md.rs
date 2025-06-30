use kuchiki::traits::*;
use ratatui::{
    prelude::*,
    text::{Line, Span},
};

/// Fix invalid nested list structure in HTML
/// Asana's API can produce invalid HTML for nested lists (e.g., a <ul>
/// as a direct child of another <ul>, or <ol> as a direct child of <ol>).
/// We pre-process the HTML to correct the structure before converting to Markdown.
fn fix_nested_lists(html: &str) -> String {
    let document = kuchiki::parse_html().one(html);
    
    // Find all <ul> and <ol> elements
    let list_selector = match document.select("ul, ol") {
        Ok(selector) => selector,
        Err(_) => return html.to_string(), // Return original if selector fails
    };
    
    // Collect nodes to fix (we can't modify while iterating)
    let mut fixes_needed = Vec::new();
    
    for list_ref in list_selector {
        let list_node = list_ref.as_node();
        
        // Check if parent is also a list (ul or ol)
        if let Some(parent) = list_node.parent() {
            if let Some(element) = parent.as_element() {
                let parent_name = &element.name.local;
                if parent_name.as_ref() == "ul" || parent_name.as_ref() == "ol" {
                    // This list is a direct child of another list - needs fixing
                    // Find the preceding <li> sibling
                    let mut current = list_node.clone();
                    while let Some(prev_sibling) = current.previous_sibling() {
                        if let Some(element) = prev_sibling.as_element() {
                            if element.name.local.as_ref() == "li" {
                                // Found the preceding <li> - store the fix needed
                                fixes_needed.push((list_node.clone(), prev_sibling.clone()));
                                break;
                            }
                        }
                        current = prev_sibling;
                    }
                }
            }
        }
    }
    
    // Apply the fixes
    for (list_node, li_node) in fixes_needed {
        // Detach the list from its current position
        list_node.detach();
        // Append it to the preceding <li>
        li_node.append(list_node);
    }
    
    // Return the fixed HTML
    document.to_string()
}

/// Convert HTML description to markdown for better TUI rendering
pub fn html_to_markdown(html: &str) -> String {
    if html.trim().is_empty() {
        return String::new();
    }

    // First fix any invalid nested list structures
    let fixed_html = fix_nested_lists(html);

    // Convert HTML to markdown using htmd with better error handling
    // htmd has better customization options for modifying HTML before conversion
    match htmd::convert(&fixed_html) {
        Ok(markdown) => {
            // Clean up extra whitespace and newlines
            markdown.trim().to_string()
        }
        Err(_) => {
            // Fallback to original HTML if conversion fails
            html.to_string()
        }
    }
}

/// Parse markdown text and convert to styled Lines for better rendering
pub fn parse_markdown_to_lines(markdown: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for line in markdown.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        // Handle headers
        if let Some(text) = trimmed.strip_prefix("# ") {
            lines.push(Line::from(vec![Span::styled(
                text.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(""));
        } else if let Some(text) = trimmed.strip_prefix("## ") {
            lines.push(Line::from(vec![Span::styled(
                text.to_string(),
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            )]));
        } else if let Some(text) = trimmed.strip_prefix("### ") {
            lines.push(Line::from(vec![Span::styled(
                text.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]));
        }
        // Handle bullet points
        else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let text = &trimmed[2..];
            lines.push(Line::from(vec![
                Span::styled("• ", Style::default().fg(Color::Green)),
                Span::raw(text.to_string()),
            ]));
        }
        // Handle numbered lists
        else if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
            && trimmed.contains(". ")
        {
            if let Some(dot_pos) = trimmed.find(". ") {
                let number = &trimmed[..dot_pos + 1];
                let text = &trimmed[dot_pos + 2..];
                lines.push(Line::from(vec![
                    Span::styled(format!("{number} "), Style::default().fg(Color::Magenta)),
                    Span::raw(text.to_string()),
                ]));
            } else {
                lines.push(Line::from(trimmed.to_string()));
            }
        }
        // Handle bold text (basic **text** parsing)
        else if trimmed.contains("**") {
            let styled_line = parse_bold_text(trimmed);
            lines.push(styled_line);
        }
        // Handle italic text (basic *text* parsing)
        else if trimmed.contains('*') && !trimmed.starts_with("*") {
            let styled_line = parse_italic_text(trimmed);
            lines.push(styled_line);
        }
        // Handle code blocks or inline code
        else if trimmed.starts_with("```") {
            lines.push(Line::from(vec![Span::styled(
                trimmed.to_string(),
                Style::default().fg(Color::Gray).bg(Color::DarkGray),
            )]));
        } else if trimmed.contains('`') {
            let styled_line = parse_inline_code(trimmed);
            lines.push(styled_line);
        }
        // Regular text
        else {
            lines.push(Line::from(trimmed.to_string()));
        }
    }

    // Remove trailing empty lines to reduce blank space
    while let Some(last_line) = lines.last() {
        if last_line.spans.is_empty()
            || (last_line.spans.len() == 1 && last_line.spans[0].content.is_empty())
        {
            lines.pop();
        } else {
            break;
        }
    }

    lines
}

/// Parse bold text (**text**)
fn parse_bold_text(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut in_bold = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '*' && chars.peek() == Some(&'*') {
            chars.next(); // consume second *
            if !current.is_empty() {
                spans.push(if in_bold {
                    Span::styled(
                        current.clone(),
                        Style::default().add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw(current.clone())
                });
                current.clear();
            }
            in_bold = !in_bold;
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        spans.push(if in_bold {
            Span::styled(current, Style::default().add_modifier(Modifier::BOLD))
        } else {
            Span::raw(current)
        });
    }

    Line::from(spans)
}

/// Parse italic text (*text*)
fn parse_italic_text(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut in_italic = false;

    for ch in text.chars() {
        if ch == '*' && !in_italic {
            if !current.is_empty() {
                spans.push(Span::raw(current.clone()));
                current.clear();
            }
            in_italic = true;
        } else if ch == '*' && in_italic {
            if !current.is_empty() {
                spans.push(Span::styled(
                    current.clone(),
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
                current.clear();
            }
            in_italic = false;
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        spans.push(if in_italic {
            Span::styled(current, Style::default().add_modifier(Modifier::ITALIC))
        } else {
            Span::raw(current)
        });
    }

    Line::from(spans)
}

/// Parse inline code (`code`)
fn parse_inline_code(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut in_code = false;

    for ch in text.chars() {
        if ch == '`' {
            if !current.is_empty() {
                spans.push(if in_code {
                    Span::styled(
                        current.clone(),
                        Style::default().fg(Color::Green).bg(Color::DarkGray),
                    )
                } else {
                    Span::raw(current.clone())
                });
                current.clear();
            }
            in_code = !in_code;
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        spans.push(if in_code {
            Span::styled(
                current,
                Style::default().fg(Color::Green).bg(Color::DarkGray),
            )
        } else {
            Span::raw(current)
        });
    }

    Line::from(spans)
}