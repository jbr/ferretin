use fieldwork::Fieldwork;
use terminal_size::{Width, terminal_size};

use crate::color_scheme::ColorScheme;
use crate::renderer::{self, OutputMode};
use crate::styled_string::Document;
use crate::verbosity::Verbosity;

/// Context for formatting operations
#[derive(Debug, Clone, Fieldwork)]
#[fieldwork(get)]
pub(crate) struct FormatContext {
    /// Whether to include source code snippets
    include_source: bool,
    /// Whether to show recursive/nested content
    #[field = "is_recursive"]
    recursive: bool,
    /// Level of documentation detail to show
    #[field(copy)]
    verbosity: Verbosity,
    /// Color scheme for rendering (derived from syntect theme)
    #[fieldwork(skip)]
    color_scheme: ColorScheme,
    /// Terminal width for wrapping/layout
    terminal_width: usize,
    /// Output mode (TTY, Plain, TestMode)
    #[field(copy)]
    output_mode: OutputMode,
}

impl Default for FormatContext {
    fn default() -> Self {
        let output_mode = OutputMode::detect();
        let terminal_width = terminal_size()
            .map(|(Width(w), _)| w as usize)
            .unwrap_or(80);

        Self {
            include_source: false,
            recursive: false,
            verbosity: Verbosity::Full, // For humans, default to full docs
            color_scheme: ColorScheme::default(),
            terminal_width,
            output_mode,
        }
    }
}

impl FormatContext {
    pub(crate) fn new(include_source: bool, recursive: bool) -> Self {
        let output_mode = OutputMode::detect();
        let terminal_width = terminal_size()
            .map(|(Width(w), _)| w as usize)
            .unwrap_or(80);

        Self {
            include_source,
            recursive,
            verbosity: Verbosity::Full,
            color_scheme: ColorScheme::default(),
            terminal_width,
            output_mode,
        }
    }

    /// Render a Document to a String based on the output mode
    ///
    /// Accepts anything that can be converted into a Document:
    /// - `Document` directly
    /// - `Vec<Span>`
    /// - `Vec<DocumentNode>`
    /// - `&[Span]`
    pub(crate) fn render<'a>(&self, document: impl Into<Document<'a>>) -> String {
        renderer::render(&document.into(), self.output_mode)
    }
}
