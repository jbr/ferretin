use crate::color_scheme::ColorScheme;
use crate::styled_string::{Document, DocumentNode, HeadingLevel, ListItem, Span, SpanStyle};
use owo_colors::OwoColorize;
use terminal_size::{Width, terminal_size};

/// Render a document with ANSI escape codes for terminal display
pub fn render(document: &Document) -> String {
    let terminal_width = terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80);

    // Use default color scheme (base16-ocean.dark, same as markdown renderer)
    let color_scheme = ColorScheme::default();

    let mut output = String::new();
    render_nodes(&document.nodes, &mut output, terminal_width, &color_scheme);
    output
}

fn render_nodes(
    nodes: &[DocumentNode],
    output: &mut String,
    terminal_width: usize,
    color_scheme: &ColorScheme,
) {
    for node in nodes {
        render_node(node, output, terminal_width, color_scheme);
    }
}

fn render_node(
    node: &DocumentNode,
    output: &mut String,
    terminal_width: usize,
    color_scheme: &ColorScheme,
) {
    match node {
        DocumentNode::Span(span) => render_span(span, output, color_scheme),
        DocumentNode::Heading { level, spans } => {
            render_spans(spans, output, true, color_scheme);
            output.push('\n');
            // Add decorative underlines
            match level {
                HeadingLevel::Title => {
                    output.push_str(&"═".repeat(terminal_width));
                    output.push('\n');
                }
                HeadingLevel::Section => {
                    output.push_str(&"─".repeat(terminal_width));
                    output.push('\n');
                }
            }
        }
        DocumentNode::Section { title, nodes } => {
            if let Some(title_spans) = title {
                render_spans(title_spans, output, true, color_scheme);
                output.push('\n');
            }
            render_nodes(nodes, output, terminal_width, color_scheme);
        }
        DocumentNode::List { items } => {
            for item in items {
                render_list_item(item, output, terminal_width, color_scheme);
            }
        }
        DocumentNode::CodeBlock { lang, code } => {
            // TODO: Integrate with syntect for syntax highlighting
            // For now, just use a simple background color
            let _ = lang; // Will be used for syntect
            let bg = color_scheme.default_background();
            output.push_str(&code.trim_end().on_truecolor(bg.r, bg.g, bg.b).to_string());
            output.push_str("\n\n");
        }
        DocumentNode::Link { url, text } => {
            // OSC 8 hyperlink: \x1b]8;;URL\x1b\\TEXT\x1b]8;;\x1b\\
            output.push_str(&format!("\x1b]8;;{}\x1b\\", url));
            render_spans(text, output, false, color_scheme);
            output.push_str("\x1b]8;;\x1b\\");
        }
    }
}

fn render_spans(spans: &[Span], output: &mut String, bold: bool, color_scheme: &ColorScheme) {
    for span in spans {
        render_span_with_bold(span, output, bold, color_scheme);
    }
}

fn render_span(span: &Span, output: &mut String, color_scheme: &ColorScheme) {
    render_span_with_bold(span, output, false, color_scheme);
}

fn render_span_with_bold(
    span: &Span,
    output: &mut String,
    force_bold: bool,
    color_scheme: &ColorScheme,
) {
    let text = &span.text;

    // Get color from the color scheme based on semantic style
    let color = color_scheme.color_for(span.style);

    let styled = match span.style {
        SpanStyle::Plain => {
            // Plain text uses default foreground
            let fg = color_scheme.default_foreground();
            if force_bold {
                output.push_str(&text.truecolor(fg.r, fg.g, fg.b).bold().to_string());
            } else {
                output.push_str(&text.truecolor(fg.r, fg.g, fg.b).to_string());
            }
            return;
        }
        SpanStyle::Punctuation => {
            // Punctuation uses default foreground, no color
            output.push_str(text);
            return;
        }
        SpanStyle::InlineCode => {
            // Inline code gets a subtle background (similar to code blocks)
            let bg = color_scheme.default_background();
            output.push_str(&text.on_truecolor(bg.r, bg.g, bg.b).to_string());
            return;
        }
        SpanStyle::Unconverted => {
            // Unconverted content gets a very visible white background with black text
            output.push_str(&text.black().on_white().to_string());
            return;
        }
        _ => {
            // All other styles use their theme color
            text.truecolor(color.r, color.g, color.b)
        }
    };

    if force_bold {
        output.push_str(&styled.bold().to_string());
    } else {
        output.push_str(&styled.to_string());
    }
}

fn render_list_item(
    item: &ListItem,
    output: &mut String,
    terminal_width: usize,
    color_scheme: &ColorScheme,
) {
    output.push_str("  • ");
    render_nodes(&item.nodes, output, terminal_width, color_scheme);
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_spans() {
        let doc = Document::with_nodes(vec![
            DocumentNode::Span(Span::keyword("struct")),
            DocumentNode::Span(Span::plain(" ")),
            DocumentNode::Span(Span::type_name("Foo")),
        ]);

        let output = render(&doc);
        // Should contain ANSI codes
        assert!(output.contains("\x1b"));
        // Should contain the actual text
        assert!(output.contains("struct"));
        assert!(output.contains("Foo"));
    }

    #[test]
    fn test_render_link() {
        let doc = Document::with_nodes(vec![DocumentNode::link(
            "https://example.com".to_string(),
            vec![Span::plain("Click here")],
        )]);

        let output = render(&doc);
        // Should contain OSC 8 escape sequence
        assert!(output.contains("\x1b]8;;"));
        assert!(output.contains("https://example.com"));
        assert!(output.contains("Click here"));
    }

    #[test]
    fn test_render_heading() {
        let doc = Document::with_nodes(vec![DocumentNode::heading(
            HeadingLevel::Title,
            vec![Span::plain("Test")],
        )]);

        let output = render(&doc);
        assert!(output.contains("Test"));
        // Should have decorative underline
        assert!(output.contains("═"));
    }
}
