use kuchiki::traits::*;
use ratatui::text::Line;

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

/// Convert <pre> tags to <pre><code> for proper code block conversion
fn wrap_pre_with_code(html: &str) -> String {
    // Simple approach: replace <pre> with <pre><code> and </pre> with </code></pre>
    let mut result = html.to_string();
    result = result.replace("<pre>", "<pre><code>");
    result = result.replace("</pre>", "</code></pre>");
    result
}

/// Convert HTML description to markdown for better TUI rendering
pub fn html_to_markdown(html: &str) -> String {
    if html.trim().is_empty() {
        return String::new();
    }

    // First wrap <pre> tags with <code> for proper code block conversion
    let pre_wrapped = wrap_pre_with_code(html);

    // Then fix any invalid nested list structures
    let fixed_html = fix_nested_lists(&pre_wrapped);

    // Configure htmd options to reduce aggressive spacing and handle code blocks
    let options = htmd::options::Options {
        // Reduce the aggressive spacing htmd uses by default
        ul_bullet_spacing: 1, // Default is 3, use 1 for "* item" instead of "*   item"
        ol_number_spacing: 1, // Default is likely 2-3, use 1 for "1. item" instead of "1.  item"
        // Configure code blocks to use fence style with backticks
        code_block_style: htmd::options::CodeBlockStyle::Fenced,
        code_block_fence: htmd::options::CodeBlockFence::Backticks,
        ..Default::default()
    };

    // Convert HTML to markdown using htmd with custom options
    let converter = htmd::HtmlToMarkdown::builder().options(options).build();

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
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Span;
    use termimad::minimad;

    let mut lines = Vec::new();

    // Parse markdown with minimad
    let options = minimad::Options::default();
    let md_lines = minimad::parse_text(markdown, options);

    // Convert each parsed line
    for md_line in md_lines.lines {
        let mut spans = Vec::new();

        // Process the line based on its type
        match &md_line {
            minimad::Line::Normal(composite) => {
                // Check the composite style to determine line type
                use termimad::minimad::CompositeStyle;

                match &composite.style {
                    CompositeStyle::Header(level) => {
                        // Style headers based on level
                        let header_style = match level {
                            1 => Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                            2 => Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::BOLD),
                            3 => Style::default()
                                .fg(Color::Magenta)
                                .add_modifier(Modifier::BOLD),
                            _ => Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        };

                        for compound in &composite.compounds {
                            spans.push(Span::styled(compound.src.to_string(), header_style));
                        }
                    }
                    CompositeStyle::ListItem(depth) => {
                        // Add bullet point or number
                        let indent = "  ".repeat(*depth as usize);
                        spans.push(Span::styled(
                            format!("{}• ", indent),
                            Style::default().fg(Color::Yellow),
                        ));

                        // Add the list item content
                        for compound in &composite.compounds {
                            let mut style = Style::default();

                            if compound.bold {
                                style = style.add_modifier(Modifier::BOLD);
                            }
                            if compound.italic {
                                style = style.add_modifier(Modifier::ITALIC);
                            }
                            if compound.strikeout {
                                style = style.add_modifier(Modifier::CROSSED_OUT);
                            }
                            if compound.code {
                                style = style.fg(Color::Green).bg(Color::Black);
                            }

                            spans.push(Span::styled(compound.src.to_string(), style));
                        }
                    }
                    CompositeStyle::Code => {
                        // Code block style
                        spans.push(Span::styled("    ", Style::default()));
                        for compound in &composite.compounds {
                            spans.push(Span::styled(
                                compound.src.to_string(),
                                Style::default().fg(Color::Green),
                            ));
                        }
                    }
                    CompositeStyle::Quote => {
                        // Quote style
                        spans.push(Span::styled("│ ", Style::default().fg(Color::Gray)));
                        for compound in &composite.compounds {
                            spans.push(Span::styled(
                                compound.src.to_string(),
                                Style::default()
                                    .fg(Color::Gray)
                                    .add_modifier(Modifier::ITALIC),
                            ));
                        }
                    }
                    _ => {
                        // Regular paragraph text
                        for compound in &composite.compounds {
                            let mut style = Style::default();

                            if compound.bold {
                                style = style.add_modifier(Modifier::BOLD);
                            }
                            if compound.italic {
                                style = style.add_modifier(Modifier::ITALIC);
                            }
                            if compound.strikeout {
                                style = style.add_modifier(Modifier::CROSSED_OUT);
                            }
                            if compound.code {
                                style = style.fg(Color::Green).bg(Color::Black);
                            }

                            spans.push(Span::styled(compound.src.to_string(), style));
                        }
                    }
                }
            }
            minimad::Line::TableRow(row) => {
                // Simple table rendering
                for (i, cell) in row.cells.iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::raw(" | "));
                    }
                    for compound in &cell.compounds {
                        spans.push(Span::raw(compound.src.to_string()));
                    }
                }
            }
            minimad::Line::CodeFence(ref composite) => {
                // Code fence line (could be opening/closing fence or code content)
                // Check if this is a fence marker (``` or ~~~) or actual code
                let line_content = composite
                    .compounds
                    .iter()
                    .map(|c| c.src)
                    .collect::<String>();

                if line_content.trim().starts_with("```") || line_content.trim().starts_with("~~~")
                {
                    // Skip fence markers
                    continue;
                } else {
                    // Code content - indent and color
                    spans.push(Span::styled(
                        format!("    {}", line_content),
                        Style::default().fg(Color::Green),
                    ));
                }
            }
            minimad::Line::HorizontalRule => {
                spans.push(Span::styled(
                    "─".repeat(40),
                    Style::default().fg(Color::Gray),
                ));
            }
            minimad::Line::TableRule(_) => {
                // Table separator line
                spans.push(Span::styled(
                    "─".repeat(40),
                    Style::default().fg(Color::Gray),
                ));
            }
            _ => {
                // For other line types, try to extract text
                // This shouldn't happen with standard markdown
                continue;
            }
        }

        lines.push(Line::from(spans));
    }

    lines
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pre_tag_conversion() {
        let html = r#"<h2>Test</h2>
<p>Here is some code:</p>
<pre>select os.id, os.posted_at, os.external_status
from backend.carrier_statement cs
where 1 = 1</pre>
<p>After the code</p>"#;

        let result = html_to_markdown(html);
        println!("Final markdown:");
        println!("{}", result);
        println!("\n--- Each line ---");
        for (i, line) in result.lines().enumerate() {
            println!("{}: {:?}", i, line);
        }

        // Check that the SQL code is preserved
        assert!(result.contains("select os.id"));
    }

    #[test]
    fn test_markdown_rendering() {
        let markdown = r#"# Header 1
## Header 2
### Header 3

Regular text with **bold** and *italic* and `inline code`.

* List item 1
* List item 2 with **bold**
* List item 3 with `code`

```rust
fn main() {
    println!("Hello!");
}
```

> Quote text

---"#;

        let lines = parse_markdown_to_lines(markdown);

        println!("\n=== Markdown Rendering Test ===");
        println!("Input markdown has {} chars", markdown.len());
        println!("Parsed into {} lines", lines.len());

        for (i, line) in lines.iter().enumerate() {
            println!("\nLine {}: {} spans", i, line.spans.len());
            for (j, span) in line.spans.iter().enumerate() {
                println!("  Span {}: {:?} -> {:?}", j, span.content, span.style);
            }
        }

        // Basic assertions
        assert!(lines.len() > 0);
        // Should have headers, list items, code blocks, etc
        assert!(lines.len() >= 10);
    }
}
