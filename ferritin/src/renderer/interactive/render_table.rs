use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
};

use super::state::InteractiveState;
use crate::styled_string::TableCell;

impl<'a> InteractiveState<'a> {
    /// Render table with unicode borders
    pub(super) fn render_table(
        &mut self,
        header: Option<&[TableCell<'a>]>,
        rows: &[Vec<TableCell<'a>>],
        area: Rect,
        buf: &mut Buffer,
        pos: &mut Position,
    ) {
        if rows.is_empty() && header.is_none() {
            return;
        }

        let border_style = Style::default().fg(Color::Rgb(60, 60, 70));

        // Calculate column widths based on content
        let num_cols = header
            .map(|h| h.len())
            .or_else(|| rows.first().map(|r| r.len()))
            .unwrap_or(0);

        if num_cols == 0 {
            return;
        }

        let mut col_widths = vec![0usize; num_cols];

        // Measure header widths
        if let Some(header_cells) = header {
            for (col_idx, cell) in header_cells.iter().enumerate() {
                let width = cell.spans.iter().map(|s| s.text.len()).sum::<usize>();
                col_widths[col_idx] = col_widths[col_idx].max(width);
            }
        }

        // Measure row widths
        for row_cells in rows {
            for (col_idx, cell) in row_cells.iter().enumerate() {
                if col_idx < num_cols {
                    let width = cell.spans.iter().map(|s| s.text.len()).sum::<usize>();
                    col_widths[col_idx] = col_widths[col_idx].max(width);
                }
            }
        }

        // Cap column widths to reasonable sizes and calculate total width
        let max_col_width = 40;
        for width in &mut col_widths {
            *width = (*width).min(max_col_width);
        }

        // Top border: ┌─────┬─────┐
        if pos.y >= self.viewport.scroll_offset && pos.y < self.viewport.scroll_offset + area.height
        {
            let mut col_pos = 0u16;
            self.write_text(buf, pos.y, col_pos, "┌", area, border_style);
            col_pos += 1;

            for (idx, &width) in col_widths.iter().enumerate() {
                for _ in 0..width {
                    self.write_text(buf, pos.y, col_pos, "─", area, border_style);
                    col_pos += 1;
                }
                if idx < col_widths.len() - 1 {
                    self.write_text(buf, pos.y, col_pos, "┬", area, border_style);
                    col_pos += 1;
                }
            }

            self.write_text(buf, pos.y, col_pos, "┐", area, border_style);
        }
        pos.y += 1;

        // Render header if present
        if let Some(header_cells) = header {
            if pos.y >= self.viewport.scroll_offset
                && pos.y < self.viewport.scroll_offset + area.height
            {
                let mut col_pos = 0u16;
                self.write_text(buf, pos.y, col_pos, "│", area, border_style);
                col_pos += 1;

                for (col_idx, cell) in header_cells.iter().enumerate() {
                    // Render cell content (bold for headers)
                    let mut cell_col = col_pos;
                    for span in &cell.spans {
                        let span_text = if span.text.len() > col_widths[col_idx] {
                            &span.text[..col_widths[col_idx]]
                        } else {
                            &span.text
                        };

                        let mut style = self.style(span.style);
                        style = style.add_modifier(Modifier::BOLD);

                        self.write_text(buf, pos.y, cell_col, span_text, area, style);
                        cell_col += span_text.len() as u16;
                    }

                    // Pad to column width
                    while cell_col < col_pos + col_widths[col_idx] as u16 {
                        self.write_text(buf, pos.y, cell_col, " ", area, Style::default());
                        cell_col += 1;
                    }

                    col_pos = cell_col;
                    self.write_text(buf, pos.y, col_pos, "│", area, border_style);
                    col_pos += 1;
                }
            }
            pos.y += 1;

            // Header separator: ├─────┼─────┤
            if pos.y >= self.viewport.scroll_offset
                && pos.y < self.viewport.scroll_offset + area.height
            {
                let mut col_pos = 0u16;
                self.write_text(buf, pos.y, col_pos, "├", area, border_style);
                col_pos += 1;

                for (idx, &width) in col_widths.iter().enumerate() {
                    for _ in 0..width {
                        self.write_text(buf, pos.y, col_pos, "─", area, border_style);
                        col_pos += 1;
                    }
                    if idx < col_widths.len() - 1 {
                        self.write_text(buf, pos.y, col_pos, "┼", area, border_style);
                        col_pos += 1;
                    }
                }

                self.write_text(buf, pos.y, col_pos, "┤", area, border_style);
            }
            pos.y += 1;
        }

        // Render rows
        for row_cells in rows.iter() {
            if pos.y >= self.viewport.scroll_offset
                && pos.y < self.viewport.scroll_offset + area.height
            {
                let mut col_pos = 0u16;
                self.write_text(buf, pos.y, col_pos, "│", area, border_style);
                col_pos += 1;

                for (col_idx, cell) in row_cells.iter().enumerate() {
                    if col_idx >= num_cols {
                        break;
                    }

                    // Render cell content
                    let mut cell_col = col_pos;
                    for span in &cell.spans {
                        let span_text = if span.text.len() > col_widths[col_idx] {
                            &span.text[..col_widths[col_idx]]
                        } else {
                            &span.text
                        };

                        let style = self.style(span.style);
                        self.write_text(buf, pos.y, cell_col, span_text, area, style);
                        cell_col += span_text.len() as u16;
                    }

                    // Pad to column width
                    while cell_col < col_pos + col_widths[col_idx] as u16 {
                        self.write_text(buf, pos.y, cell_col, " ", area, Style::default());
                        cell_col += 1;
                    }

                    col_pos = cell_col;
                    self.write_text(buf, pos.y, col_pos, "│", area, border_style);
                    col_pos += 1;
                }
            }
            pos.y += 1;
        }

        // Bottom border: └─────┴─────┘
        if pos.y >= self.viewport.scroll_offset && pos.y < self.viewport.scroll_offset + area.height
        {
            let mut col_pos = 0u16;
            self.write_text(buf, pos.y, col_pos, "└", area, border_style);
            col_pos += 1;

            for (idx, &width) in col_widths.iter().enumerate() {
                for _ in 0..width {
                    self.write_text(buf, pos.y, col_pos, "─", area, border_style);
                    col_pos += 1;
                }
                if idx < col_widths.len() - 1 {
                    self.write_text(buf, pos.y, col_pos, "┴", area, border_style);
                    col_pos += 1;
                }
            }

            self.write_text(buf, pos.y, col_pos, "┘", area, border_style);
        }
    }
}
