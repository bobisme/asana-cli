use kuchiki::traits::*;
use ratatui::text::Line;

/// Fix invalid nested list structure in HTML
/// Asana's API can produce invalid HTML for nested lists (e.g., a <ul>
/// as a direct child of another <ul>, or <ol> as a direct child of <ol>).
/// We pre-process the HTML to correct the structure before converting to Markdown.
fn fix_nested_lists(html: &str) -> String {
    let document = kuchiki::parse_html().one(html);
    
    // We need to process lists from deepest to shallowest to avoid issues
    // First, collect all lists with their depth
    let mut invalid_lists = Vec::new();
    
    // Find all <ul> and <ol> elements
    if let Ok(list_selector) = document.select("ul, ol") {
        for list_ref in list_selector {
            let list_node = list_ref.as_node();
            
            // Check if parent is also a list (ul or ol)
            if let Some(parent) = list_node.parent() {
                if let Some(element) = parent.as_element() {
                    let parent_name = &element.name.local;
                    if parent_name.as_ref() == "ul" || parent_name.as_ref() == "ol" {
                        // Calculate depth for sorting
                        let mut depth = 0;
                        let mut current = list_node.clone();
                        while let Some(p) = current.parent() {
                            depth += 1;
                            current = p;
                        }
                        invalid_lists.push((depth, list_node.clone()));
                    }
                }
            }
        }
    }
    
    // Sort by depth (deepest first) to avoid processing order issues
    invalid_lists.sort_by(|a, b| b.0.cmp(&a.0));
    
    // Apply fixes
    for (_, list_node) in invalid_lists {
        // Find the preceding <li> sibling
        let mut prev_li = None;
        let mut current = list_node.clone();
        
        while let Some(prev_sibling) = current.previous_sibling() {
            if let Some(element) = prev_sibling.as_element() {
                if element.name.local.as_ref() == "li" {
                    prev_li = Some(prev_sibling);
                    break;
                }
            }
            current = prev_sibling;
        }
        
        // If we found a preceding <li>, move the list inside it
        if let Some(li_node) = prev_li {
            // Check if there are any nodes after the list that should stay with the parent
            let mut nodes_after = Vec::new();
            let mut next = list_node.next_sibling();
            
            // Collect any <li> elements that come after this invalid list
            while let Some(sibling) = next {
                let next_next = sibling.next_sibling(); // Store before potential detach
                
                if let Some(element) = sibling.as_element() {
                    if element.name.local.as_ref() == "li" {
                        nodes_after.push(sibling.clone());
                    } else if element.name.local.as_ref() == "ul" || element.name.local.as_ref() == "ol" {
                        // Stop if we hit another list
                        break;
                    }
                }
                
                next = next_next;
            }
            
            // Detach the list from its current position
            list_node.detach();
            
            // Append it as the last child of the preceding <li>
            li_node.append(list_node);
            
            // The nodes after the list stay where they are (siblings of the li_node)
        }
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

    // Configure htmd options to reduce aggressive spacing
    let options = htmd::options::Options {
        // Reduce the aggressive spacing htmd uses by default
        ul_bullet_spacing: 1,  // Default is 3, use 1 for "* item" instead of "*   item"
        ol_number_spacing: 1,  // Default is likely 2-3, use 1 for "1. item" instead of "1.  item"
        ..Default::default()
    };

    // Convert HTML to markdown using htmd with custom options
    let converter = htmd::HtmlToMarkdown::builder()
        .options(options)
        .build();
        
    match converter.convert(&fixed_html) {
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
    // For now, just return the raw markdown without any styling
    markdown
        .lines()
        .map(|line| Line::from(line.to_string()))
        .collect()
}

// These helper functions are commented out for now since we're not using custom styling
/*
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
*/