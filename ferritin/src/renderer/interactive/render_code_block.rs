use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Style},
};
use syntect::easy::HighlightLines;
use syntect::util::LinesWithEndings;

use super::state::InteractiveState;

impl<'a> InteractiveState<'a> {
    /// Render code block with syntax highlighting
    pub(super) fn render_code_block(
        &mut self,
        lang: Option<&str>,
        code: &str,
        area: Rect,
        buf: &mut Buffer,
        pos: &mut Position,
        left_margin: u16,
    ) {
        let lang_display = match lang {
            Some("no_run") | Some("should_panic") | Some("ignore") | Some("compile_fail")
            | Some("edition2015") | Some("edition2018") | Some("edition2021")
            | Some("edition2024") => "rust",
            Some(l) => l,
            None => "rust",
        };

        // Calculate code block dimensions accounting for left margin
        let available_width = area.width.saturating_sub(left_margin);
        let max_line_width = code
            .lines()
            .map(|line| line.len())
            .max()
            .unwrap_or(0)
            .min((available_width.saturating_sub(4)) as usize); // Leave room for border and padding

        // Account for language label in border width: ╭───❬rust❭─╮
        let lang_label = format!("❬{}❭", lang_display);
        // Count actual display width (number of grapheme clusters, not bytes)
        let label_display_width = lang_label.chars().count();
        let min_border_for_label = label_display_width as u16 + 6; // label + some padding
        let border_width = ((max_line_width + 4).max(min_border_for_label as usize))
            .min(available_width as usize) as u16;

        let border_style = self.theme.code_block_border_style;

        // Top border with language label: ╭─────❬rust❭─╮
        if pos.y >= self.viewport.scroll_offset && pos.y < self.viewport.scroll_offset + area.height
        {
            self.write_text(buf, pos.y, left_margin, "╭", area, border_style);

            // Calculate position for language label (right side, with one dash before corner)
            // Label ends at border_width - 2 (leaving space for ─╮)
            let label_start =
                left_margin + border_width.saturating_sub(label_display_width as u16 + 2);

            // Draw left dashes (up to label)
            for i in 1..label_start.saturating_sub(left_margin) {
                self.write_text(buf, pos.y, left_margin + i, "─", area, border_style);
            }

            // Draw language label
            self.write_text(buf, pos.y, label_start, &lang_label, area, border_style);

            // Draw dashes from end of label to corner
            // The label takes label_display_width columns, so next position is label_start + label_display_width
            let label_end_col = label_start + label_display_width as u16;
            for i in label_end_col..left_margin + border_width.saturating_sub(1) {
                self.write_text(buf, pos.y, i, "─", area, border_style);
            }

            // Draw corner
            self.write_text(
                buf,
                pos.y,
                left_margin + border_width.saturating_sub(1),
                "╮",
                area,
                border_style,
            );
        }
        pos.y += 1;

        // Render code content with side borders (no background color)
        if let Some(syntax) = self
            .ui_config
            .syntax_set()
            .find_syntax_by_token(lang_display)
        {
            let theme = self.ui_config.theme();
            let mut highlighter = HighlightLines::new(syntax, theme);

            for line in LinesWithEndings::from(code) {
                if pos.y >= self.viewport.scroll_offset
                    && pos.y < self.viewport.scroll_offset + area.height
                {
                    // Left border and padding
                    self.write_text(buf, pos.y, left_margin, "│ ", area, border_style);

                    let mut col = left_margin + 2;

                    if let Ok(ranges) =
                        highlighter.highlight_line(line, self.ui_config.syntax_set())
                    {
                        for (style, text) in ranges {
                            let fg = style.foreground;
                            let ratatui_style = Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b));
                            let text = text.trim_end_matches('\n');

                            self.write_text(buf, pos.y, col, text, area, ratatui_style);
                            col += text.len() as u16;
                        }
                    } else {
                        self.write_text(
                            buf,
                            pos.y,
                            left_margin + 2,
                            line.trim_end_matches('\n'),
                            area,
                            Style::default(),
                        );
                    }

                    // Right border and padding
                    self.write_text(
                        buf,
                        pos.y,
                        left_margin + border_width.saturating_sub(2),
                        " │",
                        area,
                        border_style,
                    );
                }

                pos.y += 1;
            }
        } else {
            for line in code.lines() {
                if pos.y >= self.viewport.scroll_offset
                    && pos.y < self.viewport.scroll_offset + area.height
                {
                    // Left border and padding
                    self.write_text(buf, pos.y, left_margin, "│ ", area, border_style);

                    // Code content
                    self.write_text(buf, pos.y, left_margin + 2, line, area, Style::default());

                    // Right border and padding
                    self.write_text(
                        buf,
                        pos.y,
                        left_margin + border_width.saturating_sub(2),
                        " │",
                        area,
                        border_style,
                    );
                }
                pos.y += 1;
            }
        }

        // Bottom border: ╰─────╯
        if pos.y >= self.viewport.scroll_offset && pos.y < self.viewport.scroll_offset + area.height
        {
            self.write_text(buf, pos.y, left_margin, "╰", area, border_style);
            for i in 1..border_width.saturating_sub(1) {
                self.write_text(buf, pos.y, left_margin + i, "─", area, border_style);
            }
            self.write_text(
                buf,
                pos.y,
                left_margin + border_width.saturating_sub(1),
                "╯",
                area,
                border_style,
            );
        }
        pos.y += 1;
    }
}
