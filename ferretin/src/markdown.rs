use owo_colors::OwoColorize;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::io::{self, IsTerminal};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use terminal_size::{Width, terminal_size};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    /// ANSI escape codes for terminal colors/styles
    Tty,
    /// Plain text, no decoration
    Plain,
    /// Pseudo-XML tags for testing (e.g., <bold>text</bold>)
    TestTags,
}

pub struct MarkdownRenderer {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    output_mode: OutputMode,
    terminal_width: usize,
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        let output_mode = if std::env::var("FERRETIN_TEST_MODE").is_ok() {
            OutputMode::TestTags
        } else if io::stdout().is_terminal() {
            OutputMode::Tty
        } else {
            OutputMode::Plain
        };

        // Get terminal width, default to 80 if unavailable
        let terminal_width = terminal_size()
            .map(|(Width(w), _)| w as usize)
            .unwrap_or(80);

        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            output_mode,
            terminal_width,
        }
    }

    pub fn render_with_resolver<F>(&self, markdown: &str, link_resolver: F) -> String
    where
        F: Fn(&str) -> Option<String>,
    {
        // Preprocess: Convert [`Type`] to [Type] so pulldown-cmark sees it as a broken link
        // This regex matches [`...`] and captures the content between backticks
        let backtick_link_re = regex::Regex::new(r"\[`([^`]+)`\]").unwrap();
        let preprocessed = backtick_link_re.replace_all(markdown, "[$1]");

        // Use broken_link_callback to resolve intra-doc links like [Type] and [`Type`]
        let callback = |broken_link: pulldown_cmark::BrokenLink| {
            link_resolver(broken_link.reference.as_ref())
                .map(|url| (url.into(), broken_link.reference.to_string().into()))
        };

        let parser =
            Parser::new_with_broken_link_callback(&preprocessed, Options::empty(), Some(&callback));
        let mut output = String::new();
        let mut in_code_block = false;
        let mut code_block_lang: Option<String> = None;
        let mut code_block_content = String::new();
        let mut in_heading = false;
        let mut heading_level = HeadingLevel::H1;
        let mut in_emphasis = false;
        let mut in_strong = false;
        let in_code = false;
        let mut _current_link_url: Option<String> = None;

        for event in parser {
            match event {
                Event::Start(tag) => match tag {
                    Tag::Heading { level, .. } => {
                        in_heading = true;
                        heading_level = level;
                    }
                    Tag::CodeBlock(kind) => {
                        in_code_block = true;
                        code_block_lang = match kind {
                            CodeBlockKind::Fenced(lang) => {
                                if lang.is_empty() {
                                    None
                                } else {
                                    Some(lang.to_string())
                                }
                            }
                            CodeBlockKind::Indented => None,
                        };
                        code_block_content.clear();
                    }
                    Tag::Emphasis => {
                        in_emphasis = true;
                    }
                    Tag::Strong => {
                        in_strong = true;
                    }
                    Tag::Paragraph => {
                        // Paragraphs just need spacing
                    }
                    Tag::List(_) => {
                        output.push('\n');
                    }
                    Tag::Item => {
                        output.push_str("  • ");
                    }
                    Tag::Link { dest_url, .. } => {
                        let resolved_url = link_resolver(dest_url.as_ref())
                            .unwrap_or_else(|| dest_url.to_string());

                        // Capture the resolved URL (for future use)
                        _current_link_url = Some(resolved_url.clone());

                        match self.output_mode {
                            OutputMode::TestTags => {
                                output.push_str(&format!("<link href=\"{}\">", resolved_url));
                            }
                            OutputMode::Tty => {
                                // Start OSC 8 hyperlink
                                output.push_str(&format!("\x1b]8;;{}\x1b\\", resolved_url));
                            }
                            OutputMode::Plain => {
                                output.push('[');
                            }
                        }
                    }
                    Tag::BlockQuote(..) => {
                        output.push_str("  > ");
                    }
                    _ => {}
                },
                Event::End(tag_end) => match tag_end {
                    TagEnd::Heading(_) => {
                        if in_heading {
                            output.push('\n');
                            match self.output_mode {
                                OutputMode::Tty => {
                                    // Add underline for headings using terminal width
                                    match heading_level {
                                        HeadingLevel::H1 => {
                                            output.push_str(&"═".repeat(self.terminal_width))
                                        }
                                        HeadingLevel::H2 => {
                                            output.push_str(&"─".repeat(self.terminal_width))
                                        }
                                        _ => {}
                                    }
                                    output.push('\n');
                                }
                                OutputMode::TestTags | OutputMode::Plain => {}
                            }
                            in_heading = false;
                        }
                    }
                    TagEnd::CodeBlock => {
                        if in_code_block {
                            output.push('\n');
                            output.push_str(
                                self.render_code_block(
                                    &code_block_content,
                                    code_block_lang.as_deref(),
                                )
                                .trim_end(),
                            );
                            output.push_str("\n\n");
                            in_code_block = false;
                        }
                    }
                    TagEnd::Emphasis => {
                        in_emphasis = false;
                    }
                    TagEnd::Strong => {
                        in_strong = false;
                    }
                    TagEnd::Paragraph => {
                        output.push('\n');
                        output.push('\n');
                    }
                    TagEnd::Link => {
                        match self.output_mode {
                            OutputMode::TestTags => {
                                output.push_str("</link>");
                            }
                            OutputMode::Tty => {
                                // End OSC 8 hyperlink
                                output.push_str("\x1b]8;;\x1b\\");
                            }
                            OutputMode::Plain => {
                                output.push(']');
                            }
                        }

                        // Clear the current link URL
                        _current_link_url = None;
                    }
                    TagEnd::BlockQuote(..) => {
                        output.push('\n');
                    }

                    TagEnd::Item => {
                        output.push('\n');
                    }

                    TagEnd::List(_) => {
                        output.push('\n');
                    }
                    _ => {}
                },
                Event::Text(text) => {
                    if in_code_block {
                        code_block_content.push_str(&text);
                    } else {
                        let styled_text = match self.output_mode {
                            OutputMode::TestTags => {
                                let mut result = text.to_string();
                                if in_strong {
                                    result = format!("<bold>{}</bold>", result);
                                }
                                if in_emphasis {
                                    result = format!("<italic>{}</italic>", result);
                                }
                                result
                            }
                            OutputMode::Tty => {
                                if in_heading {
                                    match heading_level {
                                        HeadingLevel::H1 => text.bold().to_string(),
                                        HeadingLevel::H2 => text.bold().to_string(),
                                        _ => text.to_string(),
                                    }
                                } else if in_strong {
                                    text.bold().to_string()
                                } else if in_emphasis {
                                    text.italic().to_string()
                                } else if in_code {
                                    text.on_truecolor(40, 40, 40).to_string()
                                } else {
                                    text.to_string()
                                }
                            }
                            OutputMode::Plain => text.to_string(),
                        };
                        output.push_str(&styled_text);
                    }
                }
                Event::Code(code) => match self.output_mode {
                    OutputMode::TestTags => {
                        output.push_str(&format!("<code>{}</code>", code));
                    }
                    OutputMode::Tty => {
                        output.push_str(&format!("{}", code.on_truecolor(40, 40, 40)));
                    }
                    OutputMode::Plain => {
                        output.push('`');
                        output.push_str(&code);
                        output.push('`');
                    }
                },
                Event::SoftBreak => {
                    output.push(' ');
                }
                Event::HardBreak => {
                    output.push('\n');
                }
                _ => {}
            }
        }

        output
    }

    fn render_code_block(&self, code: &str, lang: Option<&str>) -> String {
        // Normalize rustdoc pseudo-languages to "rust"
        // These are test attributes, not actual language identifiers
        let lang = match lang {
            Some("no_run") | Some("should_panic") | Some("ignore") | Some("compile_fail")
            | Some("edition2015") | Some("edition2018") | Some("edition2021")
            | Some("edition2024") => "rust",
            Some(l) => l,
            None => "rust", // Default to Rust for rustdoc
        };

        // Strip hidden lines (lines starting with `# `) for Rust code blocks
        let code = if lang == "rust" || lang.starts_with("rust,") {
            self.strip_hidden_lines(code)
        } else {
            code.to_string()
        };

        match self.output_mode {
            OutputMode::TestTags => {
                // For test mode, emit pseudo-XML with language info
                format!("<pre lang=\"{}\">\n{}\n</pre>", lang, code)
            }
            OutputMode::Plain => {
                // Plain mode: just the code with backtick fences
                let mut result = String::from("```\n");
                result.push_str(&code);
                if !code.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str("```");
                result
            }
            OutputMode::Tty => {
                // Try to syntax highlight with the language (defaulting to Rust)
                if let Some(syntax) = self.syntax_set.find_syntax_by_token(lang) {
                    let theme = &self.theme_set.themes["base16-ocean.dark"];
                    let mut highlighter = HighlightLines::new(syntax, theme);
                    let mut result = String::new();

                    for line in LinesWithEndings::from(&code) {
                        if let Ok(ranges) = highlighter.highlight_line(line, &self.syntax_set) {
                            for (style, text) in ranges {
                                result.push_str(&self.styled_text(text, style));
                            }
                        } else {
                            result.push_str(line);
                        }
                    }

                    return result;
                }

                // Fallback: just return the code with a grey background
                code.on_truecolor(40, 40, 40).to_string()
            }
        }
    }

    fn styled_text(&self, text: &str, style: SyntectStyle) -> String {
        let fg = style.foreground;
        text.truecolor(fg.r, fg.g, fg.b).to_string()
    }

    // /// Create an OSC 8 hyperlink (clickable in modern terminals)
    // /// Format: \x1b]8;;URL\x1b\\TEXT\x1b]8;;\x1b\\
    // fn osc8_link(url: &str, text: &str) -> String {
    //     format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, text)
    // }

    /// Strip hidden lines from Rust code examples
    /// Lines starting with `# ` (hash followed by space) are hidden from display
    /// but included in doctests for completeness
    /// Skip lines that start with "# " (hash followed by space)
    /// But keep lines like "#[derive(...)]" or "#![feature(...)]"
    fn strip_hidden_lines(&self, code: &str) -> String {
        code.lines()
            .filter(|line| !line.starts_with("# "))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_markdown() {
        let renderer = MarkdownRenderer::new();
        let input = "This is **bold** and this is *italic*.";
        let output = renderer.render_with_resolver(input, |_| None);
        // Just test that it doesn't panic for now
        assert!(!output.is_empty());
    }

    #[test]
    fn test_code_block() {
        let renderer = MarkdownRenderer::new();
        let input = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let output = renderer.render_with_resolver(input, |_| None);
        assert!(!output.is_empty());
    }
}
