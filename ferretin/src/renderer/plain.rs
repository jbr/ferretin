use crate::styled_string::{Document, DocumentNode, HeadingLevel, ListItem, Span};

/// Render a document as plain text without any styling
pub fn render(document: &Document) -> String {
    let mut output = String::new();
    render_nodes(&document.nodes, &mut output);
    output
}

fn render_nodes(nodes: &[DocumentNode], output: &mut String) {
    for node in nodes {
        render_node(node, output);
    }
}

fn render_node(node: &DocumentNode, output: &mut String) {
    match node {
        DocumentNode::Span(span) => render_span(span, output),
        DocumentNode::Heading { level, spans } => {
            render_spans(spans, output);
            output.push('\n');
            // Add underlines for headings
            match level {
                HeadingLevel::Title => {
                    output.push_str(&"=".repeat(80));
                    output.push('\n');
                }
                HeadingLevel::Section => {
                    output.push_str(&"-".repeat(80));
                    output.push('\n');
                }
            }
        }
        DocumentNode::Section { title, nodes } => {
            if let Some(title_spans) = title {
                render_spans(title_spans, output);
                output.push('\n');
            }
            render_nodes(nodes, output);
        }
        DocumentNode::List { items } => {
            for item in items {
                render_list_item(item, output);
            }
        }
        DocumentNode::CodeBlock { code, .. } => {
            output.push_str("```\n");
            output.push_str(code);
            if !code.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("```\n");
        }
        DocumentNode::Link { text, .. } => {
            // In plain mode, just render the link text
            render_spans(text, output);
        }
    }
}

fn render_spans(spans: &[Span], output: &mut String) {
    for span in spans {
        render_span(span, output);
    }
}

fn render_span(span: &Span, output: &mut String) {
    output.push_str(&span.text);
}

fn render_list_item(item: &ListItem, output: &mut String) {
    output.push_str("  • ");
    render_nodes(&item.nodes, output);
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
        assert_eq!(output, "struct Foo");
    }

    #[test]
    fn test_render_heading() {
        let doc = Document::with_nodes(vec![DocumentNode::heading(
            HeadingLevel::Title,
            vec![Span::plain("Item: "), Span::type_name("Vec")],
        )]);

        let output = render(&doc);
        assert!(output.contains("Item: Vec"));
        assert!(output.contains("===="));
    }

    #[test]
    fn test_render_list() {
        let doc = Document::with_nodes(vec![DocumentNode::list(vec![
            ListItem::from_span(Span::plain("First")),
            ListItem::from_span(Span::plain("Second")),
        ])]);

        let output = render(&doc);
        assert!(output.contains("  • First"));
        assert!(output.contains("  • Second"));
    }
}
