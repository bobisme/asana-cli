use kuchiki::traits::*;
use ratatui::text::Line;

/// Represents a parsed markdown line with metadata
#[derive(Clone)]
pub struct MarkdownLine {
    pub line: Line<'static>,
    pub is_code_block: bool,
}

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

/// Replace markdown image syntax ![alt](url) with [Image: alt]
fn replace_markdown_images(markdown: &str) -> String {
    let mut result = String::new();
    let mut chars = markdown.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '!' && chars.peek() == Some(&'[') {
            // Found potential image syntax
            chars.next(); // consume '['

            // Extract alt text
            let mut alt_text = String::new();
            let mut found_closing = false;

            while let Some(ch) = chars.next() {
                if ch == ']' {
                    found_closing = true;
                    break;
                }
                alt_text.push(ch);
            }

            if found_closing && chars.peek() == Some(&'(') {
                // This is an image, consume the URL part
                chars.next(); // consume '('

                let mut depth = 1;
                while let Some(ch) = chars.next() {
                    if ch == '(' {
                        depth += 1;
                    } else if ch == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }

                // Add placeholder
                if alt_text.is_empty() {
                    result.push_str("[Image]");
                } else {
                    result.push_str("[Image: ");
                    result.push_str(&alt_text);
                    result.push(']');
                }
            } else {
                // Not an image, restore what we consumed
                result.push('!');
                result.push('[');
                result.push_str(&alt_text);
                if found_closing {
                    result.push(']');
                }
            }
        } else {
            result.push(ch);
        }
    }

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
            // Replace markdown image syntax with placeholders
            let result = replace_markdown_images(&markdown);
            result.trim().to_string()
        }
        Err(_) => {
            // Fallback to original HTML if conversion fails
            html.to_string()
        }
    }
}

/// Preprocess markdown to handle cases where htmd outputs 4-space indented lines
/// that aren't meant to be code blocks
fn preprocess_markdown(markdown: &str) -> String {
    let mut result = Vec::new();
    let mut in_code_block = false;
    let mut prev_was_blank = true;

    for line in markdown.lines() {
        // Check if we're entering or leaving a code block
        if line.trim().starts_with("```") || line.trim().starts_with("~~~") {
            in_code_block = !in_code_block;
            result.push(line.to_string());
            prev_was_blank = false;
            continue;
        }

        // If we're in a code block, don't process the line
        if in_code_block {
            result.push(line.to_string());
            prev_was_blank = false;
            continue;
        }

        // Check if this line starts with exactly 4 spaces
        if line.starts_with("    ") && !line.starts_with("     ") {
            // This might be a false code block from htmd
            // Only treat as code if previous line was blank and next content looks like code
            let trimmed = line.trim();

            // Heuristic: if it looks like prose (has multiple words, punctuation),
            // it's probably not meant to be code
            let looks_like_prose = trimmed.split_whitespace().count() > 5
                || trimmed.contains(". ")
                || trimmed.contains(", ");

            if looks_like_prose || !prev_was_blank {
                // Convert to non-indented line
                result.push(line.trim_start().to_string());
            } else {
                // Keep as-is, it might be actual code
                result.push(line.to_string());
            }
        } else {
            result.push(line.to_string());
        }

        prev_was_blank = line.trim().is_empty();
    }

    result.join("\n")
}

/// Parse markdown text and convert to styled Lines for better rendering
pub fn parse_markdown_to_lines(markdown: &str) -> Vec<Line<'static>> {
    parse_markdown_to_lines_with_width(markdown, None)
}

/// Parse markdown text and convert to styled Lines with optional width for code blocks
pub fn parse_markdown_to_lines_with_width(
    markdown: &str,
    width: Option<u16>,
) -> Vec<Line<'static>> {
    parse_markdown_to_marked_lines(markdown, width)
        .into_iter()
        .map(|ml| ml.line)
        .collect()
}

/// Parse markdown text and convert to MarkdownLine structs with metadata
pub fn parse_markdown_to_marked_lines(markdown: &str, width: Option<u16>) -> Vec<MarkdownLine> {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Span;
    use termimad::minimad;

    let mut lines = Vec::new();

    // Preprocess markdown to handle htmd's 4-space indentation
    // htmd sometimes indents content with 4 spaces which minimad interprets as code blocks
    // We'll detect these cases and convert them to regular paragraphs
    let preprocessed = preprocess_markdown(markdown);

    // Parse markdown with minimad
    let options = minimad::Options::default();
    let md_lines = minimad::parse_text(&preprocessed, options);

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
                        // Add bullet point or number with proper indentation
                        // depth is the number of spaces, so divide by 2 for indent level
                        let indent_level = (*depth as usize) / 2;
                        let indent = "  ".repeat(indent_level);
                        spans.push(Span::styled(format!("{}• ", indent), Style::default()));

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
                        // Code block style - no indent, black background
                        let line_content: String =
                            composite.compounds.iter().map(|c| c.src).collect();

                        // Pad to full width if width is provided
                        let padded_content = if let Some(w) = width {
                            let content_len = line_content.chars().count();
                            if content_len < w as usize {
                                format!("{}{}", line_content, " ".repeat(w as usize - content_len))
                            } else {
                                line_content
                            }
                        } else {
                            line_content
                        };

                        spans.push(Span::styled(
                            padded_content,
                            Style::default().bg(Color::Black),
                        ));
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
                    // Code content - no indent, black background
                    // Pad to full width if width is provided
                    let padded_content = if let Some(w) = width {
                        let content_len = line_content.chars().count();
                        if content_len < w as usize {
                            format!("{}{}", line_content, " ".repeat(w as usize - content_len))
                        } else {
                            line_content
                        }
                    } else {
                        line_content
                    };

                    spans.push(Span::styled(
                        padded_content,
                        Style::default().bg(Color::Black),
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

        // Check if this line is part of a code block
        let is_current_line_code = match md_line {
            minimad::Line::CodeFence(_) => true,
            minimad::Line::Normal(composite) => {
                matches!(composite.style, minimad::CompositeStyle::Code)
            }
            _ => false,
        };

        lines.push(MarkdownLine {
            line: Line::from(spans),
            is_code_block: is_current_line_code,
        });
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
    fn test_nested_lists() {
        let markdown = r#"# Nested Lists Test

* Top level item 1
  * Nested item 1.1
  * Nested item 1.2
    * Double nested item 1.2.1
    * Double nested item 1.2.2
  * Nested item 1.3
* Top level item 2
  * Nested item 2.1
* Top level item 3"#;

        // First, let's see what minimad parses directly
        use termimad::minimad;
        let options = minimad::Options::default();
        let md_lines = minimad::parse_text(markdown, options);

        println!("\n=== Direct minimad parsing ===");
        for (i, line) in md_lines.lines.iter().enumerate() {
            if let minimad::Line::Normal(composite) = line {
                if let minimad::CompositeStyle::ListItem(depth) = &composite.style {
                    let content: String = composite.compounds.iter().map(|c| c.src).collect();
                    println!(
                        "Line {}: ListItem depth={}, content={:?}",
                        i, depth, content
                    );
                }
            }
        }

        let lines = parse_markdown_to_lines(markdown);

        println!("\n=== Nested Lists Test ===");
        println!("Input markdown has {} chars", markdown.len());
        println!("Parsed into {} lines", lines.len());

        // Track depths we've seen
        let mut depths_seen = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            println!("\nLine {}: {} spans", i, line.spans.len());

            // Check if this is a list item by looking for bullet point
            if line.spans.len() > 0 {
                let first_span_content = &line.spans[0].content;

                // Count leading spaces to determine depth
                let leading_spaces = first_span_content.chars().take_while(|&c| c == ' ').count();
                let depth = leading_spaces / 2;

                if first_span_content.contains("• ") {
                    println!(
                        "  -> List item at depth {} (leading spaces: {})",
                        depth, leading_spaces
                    );
                    depths_seen.push(depth);

                    // Print the actual content
                    let content: String = line
                        .spans
                        .iter()
                        .map(|span| span.content.to_string())
                        .collect();
                    println!("  -> Full content: {:?}", content);
                }
            }

            for (j, span) in line.spans.iter().enumerate() {
                println!("  Span {}: {:?} -> {:?}", j, span.content, span.style);
            }
        }

        // Verify we have list items at different depths
        println!("\nDepths seen: {:?}", depths_seen);
        assert!(depths_seen.contains(&0), "Should have depth 0 items");
        assert!(depths_seen.contains(&1), "Should have depth 1 items");
        assert!(depths_seen.contains(&2), "Should have depth 2 items");
    }

    #[test]
    fn test_markdown_rendering() {
        let markdown = r#"# Header 1
## Header 2
### Header 3

Regular text with **bold** and *italic* and `inline code`.

* List item 1
* List item 2 with **bold**
  * Nested item 1
  * Nested item 2
    * Double nested
* List item 3 with `code`

    This is indented with 4 spaces but should not be code.

```rust
fn main() {
    println!("Hello!");
}
```

    x = 1
    y = 2

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

        // Debug list item rendering
        println!("\n=== List Item Debug ===");
        let debug_lines =
            termimad::minimad::parse_text(markdown, termimad::minimad::Options::default());
        for (i, line) in debug_lines.lines.iter().enumerate() {
            match line {
                termimad::minimad::Line::Normal(composite) => match &composite.style {
                    termimad::minimad::CompositeStyle::ListItem(depth) => {
                        let content: String = composite.compounds.iter().map(|c| c.src).collect();
                        println!(
                            "Line {}: ListItem depth={} content=\"{}\"",
                            i, depth, content
                        );
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    #[test]
    fn test_replace_markdown_images() {
        // Test basic image replacement
        assert_eq!(
            replace_markdown_images("![alt text](image.png)"),
            "[Image: alt text]"
        );

        // Test empty alt text
        assert_eq!(replace_markdown_images("![](image.png)"), "[Image]");

        // Test image in text
        assert_eq!(
            replace_markdown_images("Here is an image: ![screenshot](shot.png) in the text"),
            "Here is an image: [Image: screenshot] in the text"
        );

        // Test multiple images
        assert_eq!(
            replace_markdown_images("![first](1.png) and ![second](2.png)"),
            "[Image: first] and [Image: second]"
        );

        // Test non-image brackets
        assert_eq!(
            replace_markdown_images("This is [a link](url) not an image"),
            "This is [a link](url) not an image"
        );

        // Test escaped brackets
        assert_eq!(
            replace_markdown_images("This is not ![ an image"),
            "This is not ![ an image"
        );
    }

    #[test]
    fn test_html_to_markdown_with_images() {
        let html = r#"<p>Text with <img src="test.png" alt="test image"> inline</p>"#;
        let result = html_to_markdown(html);
        assert!(result.contains("[Image: test image]"));
        assert!(!result.contains("!["));
    }
}
