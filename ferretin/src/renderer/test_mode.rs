use crate::styled_string::{Document, DocumentNode, HeadingLevel, ListItem, Span, SpanStyle};

/// Render a document with semantic XML-like tags for testing
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
            let tag = match level {
                HeadingLevel::Title => "title",
                HeadingLevel::Section => "section-heading",
            };
            output.push_str(&format!("<{tag}>"));
            render_spans(spans, output);
            output.push_str(&format!("</{tag}>\n"));
        }
        DocumentNode::Section { title, nodes } => {
            output.push_str("<section>");
            if let Some(title_spans) = title {
                output.push_str("<section-title>");
                render_spans(title_spans, output);
                output.push_str("</section-title>");
            }
            render_nodes(nodes, output);
            output.push_str("</section>");
        }
        DocumentNode::List { items } => {
            output.push_str("<list>\n");
            for item in items {
                render_list_item(item, output);
            }
            output.push_str("</list>\n");
        }
        DocumentNode::CodeBlock { lang, code } => {
            let lang_attr = lang
                .as_ref()
                .map(|l| format!(" lang=\"{}\"", l))
                .unwrap_or_default();
            output.push_str(&format!("<code-block{}>\n", lang_attr));
            output.push_str(code);
            if !code.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("</code-block>\n");
        }
        DocumentNode::Link { url, text } => {
            output.push_str(&format!("<link href=\"{}\">", url));
            render_spans(text, output);
            output.push_str("</link>");
        }
    }
}

fn render_spans(spans: &[Span], output: &mut String) {
    for span in spans {
        render_span(span, output);
    }
}

fn render_span(span: &Span, output: &mut String) {
    let tag = match span.style {
        SpanStyle::Keyword => "keyword",
        SpanStyle::TypeName => "type-name",
        SpanStyle::FunctionName => "function-name",
        SpanStyle::FieldName => "field-name",
        SpanStyle::Lifetime => "lifetime",
        SpanStyle::Generic => "generic",
        SpanStyle::Plain => {
            // Plain text has no tag
            output.push_str(&span.text);
            return;
        }
        SpanStyle::Punctuation => "punctuation",
        SpanStyle::Operator => "operator",
        SpanStyle::Comment => "comment",
        SpanStyle::InlineCode => "inline-code",
        SpanStyle::Unconverted => "unconverted",
    };

    output.push_str(&format!("<{tag}>{}</{tag}>", span.text));
}

fn render_list_item(item: &ListItem, output: &mut String) {
    output.push_str("  <item>");
    render_nodes(&item.nodes, output);
    output.push_str("</item>\n");
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
        assert_eq!(output, "<keyword>struct</keyword> <type-name>Foo</type-name>");
    }

    #[test]
    fn test_render_heading() {
        let doc = Document::with_nodes(vec![DocumentNode::heading(
            HeadingLevel::Title,
            vec![Span::plain("Item: "), Span::type_name("Vec")],
        )]);

        let output = render(&doc);
        assert!(output.contains("<title>"));
        assert!(output.contains("Item: "));
        assert!(output.contains("<type-name>Vec</type-name>"));
        assert!(output.contains("</title>"));
    }

    #[test]
    fn test_render_code_block() {
        let doc = Document::with_nodes(vec![DocumentNode::code_block(
            Some("rust".to_string()),
            "fn main() {}".to_string(),
        )]);

        let output = render(&doc);
        assert!(output.contains("<code-block lang=\"rust\">"));
        assert!(output.contains("fn main() {}"));
        assert!(output.contains("</code-block>"));
    }
}
