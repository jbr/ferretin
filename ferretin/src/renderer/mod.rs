use crate::styled_string::Document;
use std::io::{self, IsTerminal};

mod plain;
mod test_mode;
mod tty;

/// Output mode for rendering documents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// ANSI escape codes for terminal colors/styles
    Tty,
    /// Plain text, no decoration
    Plain,
    /// Pseudo-XML tags for testing (e.g., <keyword>struct</keyword>)
    TestMode,
}

impl OutputMode {
    /// Detect the appropriate output mode based on environment
    pub fn detect() -> Self {
        if std::env::var("FERRETIN_TEST_MODE").is_ok() {
            OutputMode::TestMode
        } else if io::stdout().is_terminal() {
            OutputMode::Tty
        } else {
            OutputMode::Plain
        }
    }
}

/// Render a document to a string based on the output mode
pub fn render(document: &Document, mode: OutputMode) -> String {
    match mode {
        OutputMode::Tty => tty::render(document),
        OutputMode::Plain => plain::render(document),
        OutputMode::TestMode => test_mode::render(document),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::styled_string::{DocumentNode, HeadingLevel, Span};

    #[test]
    fn test_render_modes() {
        let doc = Document::with_nodes(vec![
            DocumentNode::heading(
                HeadingLevel::Title,
                vec![Span::plain("Test"), Span::keyword("struct")],
            ),
            DocumentNode::Span(Span::type_name("Foo")),
        ]);

        // Test that all modes produce output without panicking
        let tty_output = render(&doc, OutputMode::Tty);
        let plain_output = render(&doc, OutputMode::Plain);
        let test_output = render(&doc, OutputMode::TestMode);

        assert!(!tty_output.is_empty());
        assert!(!plain_output.is_empty());
        assert!(!test_output.is_empty());
    }
}
