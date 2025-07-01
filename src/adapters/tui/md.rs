use kuchiki::traits::*;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

/// Represents a parsed markdown line with metadata
#[derive(Clone)]
pub struct MarkdownLine {
    pub line: Line<'static>,
    pub is_code_block: bool,
}

/// Represents a word or whitespace with its associated style
#[derive(Clone, Debug)]
struct WordSpan {
    content: String,
    style: Style,
    is_whitespace: bool,
}

/// Split a line into individual words while preserving styles
fn split_line_into_words(line: &Line) -> Vec<WordSpan> {
    let mut words = Vec::new();

    for span in &line.spans {
        let mut current_pos = 0;
        let content = &span.content;
        let mut chars = content.char_indices().peekable();

        while let Some((i, ch)) = chars.next() {
            if ch.is_whitespace() {
                // Add accumulated non-whitespace as a word
                if i > current_pos {
                    words.push(WordSpan {
                        content: content[current_pos..i].to_string(),
                        style: span.style,
                        is_whitespace: false,
                    });
                }

                // Collect consecutive whitespace
                let whitespace_start = i;
                let mut whitespace_end = i + ch.len_utf8();

                while let Some((next_i, next_ch)) = chars.peek() {
                    if next_ch.is_whitespace() {
                        whitespace_end = *next_i + next_ch.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }

                words.push(WordSpan {
                    content: content[whitespace_start..whitespace_end].to_string(),
                    style: span.style,
                    is_whitespace: true,
                });

                current_pos = whitespace_end;
            } else if chars.peek().is_none() {
                // Last character, add remaining content (including this character)
                words.push(WordSpan {
                    content: content[current_pos..].to_string(),
                    style: span.style,
                    is_whitespace: false,
                });
                current_pos = content.len(); // Mark as processed
            }
        }

        // Handle case where span ends with non-whitespace that wasn't processed yet
        if current_pos < content.len() {
            words.push(WordSpan {
                content: content[current_pos..].to_string(),
                style: span.style,
                is_whitespace: false,
            });
        }
    }

    words
}

/// Calculate indentation for continuation lines based on list structure
fn calculate_continuation_indent(line: &Line) -> usize {
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    let mut chars = text.char_indices();
    let mut indent = 0;

    // Count leading whitespace
    while let Some((_, ch)) = chars.next() {
        if ch == ' ' {
            indent += 1;
        } else if ch == '\t' {
            indent += 4; // Treat tab as 4 spaces
        } else {
            break;
        }
    }

    // Look for list markers after the whitespace
    let trimmed = text.trim_start();

    // Bullet points: •, -, *
    if trimmed.starts_with("• ") || trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        return indent + 2; // Position after "• "
    }

    // Numbered lists: 1., 12., etc.
    if let Some(dot_pos) = trimmed.find(". ") {
        if trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            return indent + dot_pos + 2; // Position after "1. "
        }
    }

    // No list marker found, use original indentation
    indent
}

/// Convert words back to spans, adding continuation indent if needed
fn words_to_spans(
    words: &[WordSpan],
    is_first_line: bool,
    continuation_indent: usize,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    // Add indentation for continuation lines
    if !is_first_line && continuation_indent > 0 {
        spans.push(Span::raw(" ".repeat(continuation_indent)));
    }

    // Convert words back to spans, merging consecutive words with same style
    let mut current_content = String::new();
    let mut current_style = None;

    for word in words {
        // Skip leading whitespace on continuation lines
        if !is_first_line && current_content.is_empty() && word.is_whitespace {
            continue;
        }

        if current_style.as_ref() == Some(&word.style) {
            current_content.push_str(&word.content);
        } else {
            // Style changed, finish previous span
            if !current_content.is_empty() {
                if let Some(style) = current_style {
                    spans.push(Span::styled(current_content.clone(), style));
                } else {
                    spans.push(Span::raw(current_content.clone()));
                }
            }
            current_content = word.content.clone();
            current_style = Some(word.style);
        }
    }

    // Add final span
    if !current_content.is_empty() {
        if let Some(style) = current_style {
            spans.push(Span::styled(current_content, style));
        } else {
            spans.push(Span::raw(current_content));
        }
    }

    spans
}

/// Wrap a single MarkdownLine to fit within the specified width
fn wrap_single_line(
    line: MarkdownLine,
    max_width: u16,
    continuation_indent: usize,
) -> Vec<MarkdownLine> {
    if line.is_code_block || max_width < 10 {
        return vec![line]; // Never wrap code blocks or very narrow widths
    }

    let words = split_line_into_words(&line.line);
    if words.is_empty() {
        return vec![line];
    }

    let mut result_lines = Vec::new();
    let mut current_line_words = Vec::new();
    let mut current_width = 0usize;
    let mut is_first_line = true;

    for word in words {
        let word_width = word.content.width();
        let available_width = if is_first_line {
            max_width as usize
        } else {
            (max_width as usize).saturating_sub(continuation_indent)
        };

        // Check if we need to wrap
        if current_width + word_width > available_width && !current_line_words.is_empty() {
            // Current word doesn't fit, finish current line
            let line_spans =
                words_to_spans(&current_line_words, is_first_line, continuation_indent);
            result_lines.push(MarkdownLine {
                line: Line::from(line_spans),
                is_code_block: false,
            });

            current_line_words.clear();
            current_width = 0;
            is_first_line = false;
        }

        current_line_words.push(word);
        current_width += word_width;
    }

    // Add final line if any words remain
    if !current_line_words.is_empty() {
        let line_spans = words_to_spans(&current_line_words, is_first_line, continuation_indent);
        result_lines.push(MarkdownLine {
            line: Line::from(line_spans),
            is_code_block: false,
        });
    }

    // Return original line if no wrapping occurred
    if result_lines.is_empty() {
        vec![line]
    } else {
        result_lines
    }
}

/// Wrap all markdown lines to fit within the specified width
fn wrap_markdown_lines(lines: Vec<MarkdownLine>, width: u16) -> Vec<MarkdownLine> {
    let mut wrapped = Vec::new();

    for line in lines {
        let continuation_indent = calculate_continuation_indent(&line.line);
        let wrapped_lines = wrap_single_line(line, width, continuation_indent);
        wrapped.extend(wrapped_lines);
    }

    wrapped
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

/// Parse markdown text with intelligent text wrapping that preserves formatting
pub fn parse_markdown_to_marked_lines_with_wrapping(
    markdown: &str,
    width: Option<u16>,
) -> Vec<MarkdownLine> {
    // First parse the markdown with width for code block padding
    let lines = parse_markdown_to_marked_lines(markdown, width);

    // Apply intelligent wrapping if width is specified
    if let Some(w) = width {
        wrap_markdown_lines(lines, w)
    } else {
        lines
    }
}

/// Parse markdown text and convert to MarkdownLine structs with metadata
pub fn parse_markdown_to_marked_lines(markdown: &str, width: Option<u16>) -> Vec<MarkdownLine> {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::Span;

    let mut lines = Vec::new();

    // Create parser with default options
    let parser = Parser::new(markdown);

    // Track state
    let mut current_line_spans: Vec<Span<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut list_stack: Vec<ListInfo> = Vec::new();
    let mut emphasis_stack = Vec::new();
    let mut link_destination = None;

    #[derive(Clone)]
    struct ListInfo {
        is_ordered: bool,
        indent_level: usize,
        next_number: u64,
    }

    // Helper to finish current line
    let finish_line =
        |spans: &mut Vec<Span<'static>>, lines: &mut Vec<MarkdownLine>, is_code: bool| {
            if !spans.is_empty() {
                let line_spans: Vec<Span<'static>> = spans.drain(..).collect();
                lines.push(MarkdownLine {
                    line: Line::from(line_spans),
                    is_code_block: is_code,
                });
            }
        };

    // Helper to add a blank line
    let add_blank_line = |lines: &mut Vec<MarkdownLine>| {
        lines.push(MarkdownLine {
            line: Line::from(vec![Span::raw("")]),
            is_code_block: false,
        });
    };

    for event in parser {
        match event {
            Event::Start(tag) => {
                match tag {
                    Tag::Paragraph => {
                        // Start new paragraph
                    }
                    Tag::Heading { level, .. } => {
                        // Apply header styling based on level
                        let header_style = match level {
                            pulldown_cmark::HeadingLevel::H1 => Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                            pulldown_cmark::HeadingLevel::H2 => Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::BOLD),
                            pulldown_cmark::HeadingLevel::H3 => Style::default()
                                .fg(Color::Magenta)
                                .add_modifier(Modifier::BOLD),
                            _ => Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        };
                        emphasis_stack.push(header_style);
                    }
                    Tag::List(start_num) => {
                        // Track list nesting
                        let indent_level = list_stack.len() * 2; // 2 spaces per level
                        list_stack.push(ListInfo {
                            is_ordered: start_num.is_some(),
                            indent_level,
                            next_number: start_num.unwrap_or(1),
                        });
                    }
                    Tag::Item => {
                        // Start a new list item - finish previous line if any
                        finish_line(&mut current_line_spans, &mut lines, false);

                        if let Some(list_info) = list_stack.last_mut() {
                            let indent = " ".repeat(list_info.indent_level);
                            if list_info.is_ordered {
                                current_line_spans.push(Span::raw(format!(
                                    "{}{}. ",
                                    indent, list_info.next_number
                                )));
                                list_info.next_number += 1;
                            } else {
                                current_line_spans.push(Span::raw(format!("{}• ", indent)));
                            }
                        }
                    }
                    Tag::CodeBlock(_) => {
                        in_code_block = true;
                        // Finish current line if any
                        finish_line(&mut current_line_spans, &mut lines, false);
                    }
                    Tag::Emphasis => {
                        emphasis_stack.push(Style::default().add_modifier(Modifier::ITALIC));
                    }
                    Tag::Strong => {
                        emphasis_stack.push(Style::default().add_modifier(Modifier::BOLD));
                    }
                    Tag::Strikethrough => {
                        emphasis_stack.push(Style::default().add_modifier(Modifier::CROSSED_OUT));
                    }
                    Tag::Link { dest_url, .. } => {
                        link_destination = Some(dest_url.to_string());
                        emphasis_stack.push(
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::UNDERLINED),
                        );
                    }
                    Tag::Image { dest_url, .. } => {
                        // We'll convert images to text placeholders
                        link_destination = Some(dest_url.to_string());
                    }
                    Tag::BlockQuote(_) => {
                        current_line_spans
                            .push(Span::styled("│ ", Style::default().fg(Color::Gray)));
                        emphasis_stack.push(
                            Style::default()
                                .fg(Color::Gray)
                                .add_modifier(Modifier::ITALIC),
                        );
                    }
                    Tag::Table(_) => {
                        // Handle table start
                    }
                    Tag::TableHead => {}
                    Tag::TableRow => {}
                    Tag::TableCell => {}
                    _ => {}
                }
            }
            Event::End(tag) => {
                match tag {
                    TagEnd::Paragraph => {
                        finish_line(&mut current_line_spans, &mut lines, false);
                        add_blank_line(&mut lines);
                    }
                    TagEnd::Heading(_) => {
                        emphasis_stack.pop();
                        finish_line(&mut current_line_spans, &mut lines, false);
                        add_blank_line(&mut lines);
                    }
                    TagEnd::List(_) => {
                        // Finish current line if any
                        finish_line(&mut current_line_spans, &mut lines, false);
                        list_stack.pop();
                        // Only add blank line after outermost list ends
                        if list_stack.is_empty() {
                            add_blank_line(&mut lines);
                        }
                    }
                    TagEnd::Item => {
                        // Don't finish line here - let other events handle line breaks
                    }
                    TagEnd::CodeBlock => {
                        in_code_block = false;
                        add_blank_line(&mut lines);
                    }
                    TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                        emphasis_stack.pop();
                    }
                    TagEnd::Link => {
                        // Clear the link destination but don't display the URL
                        link_destination.take();
                        emphasis_stack.pop();
                    }
                    TagEnd::Image => {
                        // Add image placeholder if we captured alt text
                        if link_destination.is_some() {
                            // The alt text was already added in the Text event
                            // Just add the [Image] suffix if no alt text was provided
                            if current_line_spans.is_empty()
                                || !current_line_spans
                                    .last()
                                    .map_or(false, |s| s.content.contains("["))
                            {
                                current_line_spans.push(Span::raw("[Image]"));
                            }
                        }
                        link_destination = None;
                    }
                    TagEnd::BlockQuote => {
                        emphasis_stack.pop();
                        finish_line(&mut current_line_spans, &mut lines, false);
                        add_blank_line(&mut lines);
                    }
                    TagEnd::Table => {}
                    TagEnd::TableHead => {}
                    TagEnd::TableRow => {
                        finish_line(&mut current_line_spans, &mut lines, false);
                    }
                    TagEnd::TableCell => {
                        current_line_spans.push(Span::raw(" | "));
                    }
                    _ => {}
                }
            }
            Event::Text(text) => {
                if link_destination.is_some() && matches!(emphasis_stack.last(), Some(_)) {
                    // This might be image alt text - check if we're in an image context
                    // For now, we'll handle it as regular link text
                    let mut style = Style::default();
                    for s in &emphasis_stack {
                        style = style.patch(*s);
                    }
                    current_line_spans.push(Span::styled(text.to_string(), style));
                } else {
                    // Apply accumulated styles
                    let mut style = Style::default();
                    for s in &emphasis_stack {
                        style = style.patch(*s);
                    }

                    if in_code_block {
                        // Split code text by newlines and create separate lines
                        let text_str = text.to_string();
                        let code_lines: Vec<&str> = text_str.split('\n').collect();

                        for line_text in code_lines.iter() {
                            // Skip empty lines, especially the trailing empty line from split
                            if !line_text.is_empty() || (code_lines.len() == 1) {
                                // Pad to width if specified
                                let content = if let Some(w) = width {
                                    let content_len = line_text.chars().count();
                                    if content_len < w as usize {
                                        format!(
                                            "{}{}",
                                            line_text,
                                            " ".repeat(w as usize - content_len)
                                        )
                                    } else {
                                        line_text.to_string()
                                    }
                                } else {
                                    line_text.to_string()
                                };

                                // Code blocks get special background
                                lines.push(MarkdownLine {
                                    line: Line::from(vec![Span::styled(
                                        content,
                                        style.bg(Color::Black),
                                    )]),
                                    is_code_block: true,
                                });
                            }
                        }
                    } else {
                        current_line_spans.push(Span::styled(text.to_string(), style));
                    }
                }
            }
            Event::Code(code) => {
                // Inline code
                let style = Style::default().fg(Color::White).bg(Color::Black);
                current_line_spans.push(Span::styled(code.to_string(), style));
            }
            Event::SoftBreak => {
                finish_line(&mut current_line_spans, &mut lines, false);
            }
            Event::HardBreak => {
                finish_line(&mut current_line_spans, &mut lines, false);
            }
            Event::Rule => {
                let rule_width = width.unwrap_or(80) as usize;
                lines.push(MarkdownLine {
                    line: Line::from(vec![Span::styled(
                        "─".repeat(rule_width),
                        Style::default().fg(Color::Gray),
                    )]),
                    is_code_block: false,
                });
            }
            Event::FootnoteReference(name) => {
                current_line_spans.push(Span::styled(
                    format!("[^{}]", name),
                    Style::default().fg(Color::Blue),
                ));
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                current_line_spans.push(Span::raw(marker));
            }
            _ => {}
        }
    }

    // Finish any remaining content
    finish_line(&mut current_line_spans, &mut lines, false);

    lines
}

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
                let depth = leading_spaces / 2; // we use 2 spaces per level

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

- List item 1
- List item 2 with **bold**
  - Nested item 1
  - Nested item 2
    - Double nested
- List item 3 with `code`

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
        // With blank lines added, we'll have more lines
        assert!(lines.len() >= 20);
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

    #[test]
    fn test_numbered_nested_lists() {
        let html = r#"<ol>
<li>First item
<ol>
<li>Nested item 1</li>
<li>Nested item 2</li>
</ol>
</li>
<li>Second item</li>
</ol>"#;

        let markdown = html_to_markdown(html);
        println!("Converted markdown:\n{}", markdown);

        let lines = parse_markdown_to_lines(&markdown);
        for (i, line) in lines.iter().enumerate() {
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.to_string())
                .collect();
            println!("Line {}: '{}'", i, text);
        }

        // Check that nested items are properly indented
        assert!(lines.len() >= 4);
        let line1_text: String = lines[1]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        let line2_text: String = lines[2]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();

        // First level items should start with number
        assert!(lines[0].spans.iter().any(|s| s.content.contains("1.")));
        // Nested items should have indentation (2 spaces)
        assert!(
            line1_text.starts_with("  "),
            "Line 1 should start with 2 spaces, got: '{}'",
            line1_text
        );
    }

    #[test]
    fn test_double_nested_numbered_lists() {
        // Test with 4-space indentation (what htmd actually produces)
        let markdown = r#"1. item 1
    1. nested item 1
        1. double nested item 1"#;

        println!("Original markdown:");
        for (i, line) in markdown.lines().enumerate() {
            println!(
                "Line {}: '{}' ({} leading spaces)",
                i,
                line,
                line.len() - line.trim_start().len()
            );
        }

        let lines = parse_markdown_to_lines(markdown);

        println!("\nParsed lines:");
        for (i, line) in lines.iter().enumerate() {
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.to_string())
                .collect();
            println!("Line {}: '{}'", i, text);

            // Also print character count for indentation debugging
            let leading_spaces = text.len() - text.trim_start().len();
            println!("  Leading spaces: {}", leading_spaces);
        }

        // Get the text of each line
        let line0_text: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        let line1_text: String = lines[1]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        let line2_text: String = lines[2]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();

        // Check indentation levels
        assert_eq!(line0_text, "1. item 1");
        assert!(
            line1_text.starts_with("  "),
            "Line 1 should have 2 spaces, got: '{}'",
            line1_text
        );
        assert!(
            line2_text.starts_with("    "),
            "Line 2 should have 4 spaces, got: '{}'",
            line2_text
        );

        // Make sure we don't have excessive indentation
        let line2_spaces = line2_text.len() - line2_text.trim_start().len();
        assert!(
            line2_spaces == 4,
            "Line 2 should have exactly 4 spaces, got {} spaces: '{}'",
            line2_spaces,
            line2_text
        );
    }

    #[test]
    fn test_mixed_indentation_lists() {
        // Test various indentation scenarios
        let markdown = r#"1. Top level
    1. 4-space indent
        1. 8-space indent
            1. 12-space indent
    2. Back to 4-space
2. Back to top

* Bullet top
    * 4-space bullet
        * 8-space bullet
    * Back to 4-space"#;

        let lines = parse_markdown_to_lines(markdown);

        // Check specific lines
        // Note: line numbers may vary as pulldown-cmark adds blank lines
        let expected_patterns = vec![
            ("1. Top level", 0),
            ("1. 4-space indent", 2),
            ("1. 8-space indent", 4),
            ("1. 12-space indent", 6),
            ("2. Back to 4-space", 2),
            ("2. Back to top", 0),
            ("• Bullet top", 0),
            ("• 4-space bullet", 2),
            ("• 8-space bullet", 4),
            ("• Back to 4-space", 2),
        ];

        for (expected_content, expected_indent) in expected_patterns {
            let found = lines.iter().any(|line| {
                let text: String = line
                    .spans
                    .iter()
                    .map(|span| span.content.to_string())
                    .collect();
                let actual_indent = text.len() - text.trim_start().len();
                text.trim().contains(expected_content) && actual_indent == expected_indent
            });
            assert!(
                found,
                "Should find '{}' with {} spaces indent",
                expected_content, expected_indent
            );
        }
    }

    #[test]
    fn test_wrapping_functionality() {
        // Test that long lines get wrapped properly with list indentation
        let markdown = r#"• This is a very long bullet point that should definitely wrap to multiple lines when rendered in a narrow terminal width
  • This is a nested item that should also wrap with proper indentation alignment for continuation lines

1. This is a numbered list item with very long text that should wrap properly and maintain the numbered list indentation for wrapped lines
2. Another numbered item"#;

        let lines = parse_markdown_to_marked_lines_with_wrapping(markdown, Some(40));

        println!("\n=== Wrapping Test ===");
        println!("Input width: 40 characters");
        println!("Parsed into {} lines", lines.len());

        for (i, line) in lines.iter().enumerate() {
            let text: String = line
                .line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            println!("Line {}: '{}'", i, text);

            // Check that lines don't exceed the width (with some tolerance for edge cases)
            let visual_width = text.width();
            println!("  Width: {} chars", visual_width);

            // Most lines should be <= 40 chars (allowing some tolerance for edge cases)
            if visual_width > 45 {
                println!("  WARNING: Line {} exceeds expected width significantly", i);
            }
        }

        // Should have more lines than the original due to wrapping
        assert!(
            lines.len() >= 4,
            "Should have at least 4 lines after wrapping"
        );

        // Check that list indentation is preserved by looking for continuation lines
        let mut found_continuation = false;
        for line in &lines {
            let text: String = line.line.spans.iter().map(|s| s.content.as_ref()).collect();
            if text.starts_with("  ") && !text.starts_with("  •") && !text.trim().is_empty() {
                found_continuation = true;
                println!("Found continuation line: '{}'", text);
                break;
            }
        }

        // We should find at least one continuation line from the wrapping
        assert!(
            found_continuation,
            "Should find continuation lines with proper indentation"
        );
    }

    #[test]
    fn test_new_features() {
        // Test links without URLs and horizontal rules
        let markdown = r#"[This is a link](https://example.com) with no URL shown.

---

More content after the rule."#;

        let lines = parse_markdown_to_marked_lines_with_wrapping(markdown, Some(50));

        println!("\n=== New Features Test ===");
        for (i, line) in lines.iter().enumerate() {
            let text: String = line.line.spans.iter().map(|s| s.content.as_ref()).collect();
            println!("Line {}: '{}'", i, text);
        }

        let all_text: Vec<String> = lines
            .iter()
            .map(|line| line.line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();

        // Should find link text without URL
        assert!(all_text
            .iter()
            .any(|text| text.contains("This is a link") && !text.contains("example.com")));

        // Should find horizontal rule spanning width
        assert!(all_text
            .iter()
            .any(|text| text.contains("─") && text.len() >= 40));
    }

    #[test]
    fn test_all_list_types() {
        let markdown = r#"# List Test

## Bullet Lists with *
* First item
  * Nested item
    * Double nested
  * Back to single nested
* Second item

## Bullet Lists with -
- First item
  - Nested item
    - Double nested
  - Back to single nested
- Second item

## Numbered Lists
1. First item
   1. Nested item
      1. Double nested
   2. Back to single nested
2. Second item

## Mixed Lists
1. Numbered first
   * Bullet nested
     - Dash double nested
   * Back to bullet
2. Numbered second"#;

        let lines = parse_markdown_to_lines(markdown);

        // Count indented lines
        let indented_lines: Vec<(usize, String)> = lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let text: String = line
                    .spans
                    .iter()
                    .map(|span| span.content.to_string())
                    .collect();
                (i, text)
            })
            .filter(|(_, text)| text.starts_with("  ") || text.starts_with("    "))
            .collect();

        println!("Found {} indented lines:", indented_lines.len());
        for (i, text) in &indented_lines {
            println!("  Line {}: '{}'", i, text);
        }

        // We should have multiple indented lines
        assert!(
            indented_lines.len() >= 10,
            "Should have at least 10 indented lines, got {}",
            indented_lines.len()
        );

        // Check specific indentations
        // We use 2-space indents per level
        assert!(
            indented_lines
                .iter()
                .any(|(_, text)| text.starts_with("  • ")),
            "Should have 2-space indented bullet"
        );
        assert!(
            indented_lines
                .iter()
                .any(|(_, text)| text.starts_with("    ")),
            "Should have 4-space indented item"
        );
    }
}
