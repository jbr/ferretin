use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::Modifier,
};

use super::{state::InteractiveState, utils::find_paragraph_truncation_point};
use crate::styled_string::{
    DocumentNode, HeadingLevel, NodePath, ShowWhen, Span, TruncationLevel, TuiAction,
};

impl<'a> InteractiveState<'a> {
    /// Render a single node
    pub(super) fn render_node(
        &mut self,
        node: &DocumentNode<'a>,
        area: Rect,
        buf: &mut Buffer,
        pos: &mut Position,
        path: &NodePath,
        left_margin: u16,
    ) {
        match node {
            DocumentNode::Span(span) => {
                self.render_span(span, area, buf, pos, left_margin);
            }

            DocumentNode::Heading { level, spans } => {
                // Start new line if not at beginning
                if pos.x > 0 {
                    pos.y += 1;
                    pos.x = left_margin;
                }

                // Render heading spans (bold)
                for span in spans {
                    self.render_span_with_modifier(
                        span,
                        Modifier::BOLD,
                        area,
                        buf,
                        pos,
                        left_margin,
                    );
                }

                // New line after heading
                pos.y += 1;
                pos.x = left_margin;

                // Add decorative underline (respecting left margin)
                let underline_char = match level {
                    HeadingLevel::Title => '=',
                    HeadingLevel::Section => '-',
                };

                if pos.y >= self.viewport.scroll_offset
                    && pos.y < self.viewport.scroll_offset + area.height
                {
                    for c in left_margin..area.width {
                        buf.cell_mut((c, pos.y - self.viewport.scroll_offset))
                            .unwrap()
                            .set_char(underline_char);
                    }
                }

                pos.y += 1;
                pos.x = left_margin;
            }

            DocumentNode::List { items } => {
                for (item_idx, item) in items.iter().enumerate() {
                    // Start new line
                    if pos.x > 0 {
                        pos.y += 1;
                        pos.x = left_margin;
                    }

                    // Bullet with nice unicode character
                    let bullet_style = self.theme.muted_style;
                    self.write_text(buf, pos.y, pos.x, "  ◦ ", area, bullet_style);
                    pos.x += 4;

                    // Capture left margin for content right after bullet
                    let content_left_margin = pos.x;

                    // Label (if any) - should wrap to content_left_margin, not parent's left_margin
                    if let Some(label_spans) = &item.label {
                        for span in label_spans {
                            self.render_span_with_modifier(
                                span,
                                Modifier::BOLD,
                                area,
                                buf,
                                pos,
                                content_left_margin,
                            );
                        }
                    }
                    for (content_idx, content_node) in item.content.iter().enumerate() {
                        let mut content_path = *path;
                        content_path.push(item_idx);
                        content_path.push(content_idx);
                        self.render_node(
                            content_node,
                            area,
                            buf,
                            pos,
                            &content_path,
                            content_left_margin,
                        );
                    }

                    pos.y += 1;
                    pos.x = left_margin;
                }
            }

            DocumentNode::Section { title, nodes } => {
                if let Some(title_spans) = title {
                    if pos.x > 0 {
                        pos.y += 1;
                        pos.x = left_margin;
                    }

                    for span in title_spans {
                        self.render_span_with_modifier(
                            span,
                            Modifier::BOLD,
                            area,
                            buf,
                            pos,
                            left_margin,
                        );
                    }

                    pos.y += 1;
                    pos.x = left_margin;
                }

                for (idx, child_node) in nodes.iter().enumerate() {
                    let mut child_path = *path;
                    child_path.push(idx);
                    self.render_node(child_node, area, buf, pos, &child_path, left_margin);
                }
            }

            DocumentNode::CodeBlock { lang, code } => {
                if pos.x > 0 {
                    pos.y += 1;
                    pos.x = left_margin;
                }

                self.render_code_block(lang.as_deref(), code, area, buf, pos, left_margin);

                pos.y += 1;
                pos.x = left_margin;
            }

            DocumentNode::Link { url, text, target } => {
                use crate::styled_string::LinkTarget;
                // Determine the action based on the link target
                let action = match target {
                    Some(LinkTarget::Resolved(doc_ref)) => {
                        // Already resolved - navigate directly
                        TuiAction::Navigate(*doc_ref)
                    }
                    Some(LinkTarget::Path(path)) => {
                        // Unresolved path - navigate by path (lazy resolution)
                        TuiAction::NavigateToPath(path.clone())
                    }
                    None => {
                        // External link - open in browser
                        TuiAction::OpenUrl(url.clone())
                    }
                };

                // Calculate total length of link text to avoid splitting it across lines
                let total_length: usize = text.iter().map(|s| s.text.len()).sum();
                let available_width = area.width.saturating_sub(pos.x);

                // If link won't fit on current line, wrap to next line first
                if total_length as u16 > available_width && pos.x > left_margin {
                    pos.y += 1;
                    pos.x = left_margin;
                }

                // Render underlined text with the action attached
                for span in text {
                    let span_with_action = Span {
                        text: span.text.clone(),
                        style: span.style,
                        action: Some(action.clone()),
                    };
                    self.render_span_with_modifier(
                        &span_with_action,
                        Modifier::UNDERLINED,
                        area,
                        buf,
                        pos,
                        left_margin,
                    );
                }
            }

            DocumentNode::HorizontalRule => {
                if pos.x > 0 {
                    pos.y += 1;
                    pos.x = left_margin;
                }

                if pos.y >= self.viewport.scroll_offset
                    && pos.y < self.viewport.scroll_offset + area.height
                {
                    let rule_style = self.theme.muted_style;
                    // Use a decorative pattern: ─── • ───
                    let pattern = ['─', '─', '─', ' ', '•', ' '];
                    for c in 0..area.width {
                        let ch = pattern[(c as usize) % pattern.len()];
                        if let Some(cell) = buf.cell_mut((c, pos.y - self.viewport.scroll_offset)) {
                            cell.set_char(ch);
                            cell.set_style(rule_style);
                        }
                    }
                }

                pos.y += 1;
                pos.x = left_margin;
            }

            DocumentNode::BlockQuote { nodes } => {
                for (idx, child_node) in nodes.iter().enumerate() {
                    if pos.x == left_margin {
                        // Use a thicker vertical bar for quotes
                        let quote_style = self.theme.muted_style;
                        self.write_text(buf, pos.y, pos.x, "  ┃ ", area, quote_style);
                        pos.x += 4;
                    }

                    let mut child_path = *path;
                    child_path.push(idx);
                    self.render_node(child_node, area, buf, pos, &child_path, left_margin);
                }
            }

            DocumentNode::Table { header, rows } => {
                if pos.x > 0 {
                    pos.y += 1;
                    pos.x = left_margin;
                }

                self.render_table(header.as_deref(), rows, area, buf, pos);

                pos.y += 1;
                pos.x = left_margin;
            }

            DocumentNode::TruncatedBlock { nodes, level } => {
                // Determine line limit based on truncation level
                let line_limit = match level {
                    TruncationLevel::SingleLine => 3,  // Show ~3 lines for single-line
                    TruncationLevel::Brief => 8, // Show ~8 lines for brief (actual wrapped lines)
                    TruncationLevel::Full => u16::MAX, // Show everything
                };

                let start_row = pos.y;
                let mut rendered_all = true;
                let border_style = self.theme.muted_style;

                // For SingleLine with heading as first node, just show the heading text
                let render_nodes = if matches!(level, TruncationLevel::SingleLine) {
                    // Check if first node is a heading
                    if let Some(DocumentNode::Heading { spans, .. }) = nodes.first() {
                        // Just render the spans without the heading decoration
                        for span in spans {
                            self.render_span(span, area, buf, pos, left_margin);
                        }
                        rendered_all = nodes.len() <= 1;
                        false // Skip normal rendering
                    } else {
                        true
                    }
                } else {
                    true
                };

                if render_nodes {
                    // For Brief mode, try to find a good truncation point at second paragraph break
                    let truncate_at = if matches!(level, TruncationLevel::Brief) {
                        find_paragraph_truncation_point(nodes, line_limit, area.width)
                    } else {
                        None
                    };

                    // Increase left margin for content to make room for border
                    let content_left_margin = if !matches!(level, TruncationLevel::Full) {
                        left_margin + 2
                    } else {
                        left_margin
                    };

                    // Track last row with actual content (to trim trailing blank lines)
                    let mut last_content_row = start_row;

                    // Render nodes
                    for (idx, child_node) in nodes.iter().enumerate() {
                        // Check if we've hit our truncation point
                        if let Some(cutoff) = truncate_at
                            && idx >= cutoff
                        {
                            rendered_all = false;
                            break;
                        }

                        // Check if we've exceeded the line limit (fallback)
                        if pos.y - start_row >= line_limit
                            && !matches!(level, TruncationLevel::Full)
                        {
                            rendered_all = false;
                            break;
                        }

                        // Skip headings in the middle of truncated content (not first node)
                        // Only do this for Brief/SingleLine, not Full
                        if idx > 0
                            && !matches!(level, TruncationLevel::Full)
                            && matches!(child_node, DocumentNode::Heading { .. })
                        {
                            rendered_all = false;
                            break;
                        }

                        // If we're at the left margin, move to content area
                        if !matches!(level, TruncationLevel::Full) && pos.x == left_margin {
                            pos.x = content_left_margin;
                        }

                        let mut child_path = *path;
                        child_path.push(idx);
                        self.render_node(
                            child_node,
                            area,
                            buf,
                            pos,
                            &child_path,
                            content_left_margin,
                        );

                        // Track last row with content (not just blank lines)
                        if pos.x > content_left_margin {
                            last_content_row = pos.y;
                        }

                        // If this is the last node, we rendered everything
                        if idx == nodes.len() - 1 {
                            rendered_all = true;
                        }
                    }

                    // Draw left border on all lines with content (trim trailing blank lines)
                    if !matches!(level, TruncationLevel::Full) {
                        // Draw borders only up to the last row with actual content
                        let end_row = last_content_row + 1;

                        for r in start_row..end_row {
                            if r >= self.viewport.scroll_offset
                                && r < self.viewport.scroll_offset + area.height
                            {
                                self.write_text(buf, r, left_margin, "│ ", area, border_style);
                            }
                        }

                        // Move to the row after last content for the closing border
                        pos.y = last_content_row + 1;
                        pos.x = left_margin;
                    }
                }

                // Show bottom border with [...] if we didn't render all nodes
                if !rendered_all && !matches!(level, TruncationLevel::Full) {
                    let ellipsis_text = "╰─[...]";
                    let ellipsis_row = pos.y;

                    // Check if hovered
                    let is_hovered = self.viewport.cursor_pos.map_or_else(
                        || false,
                        |pos| {
                            pos.y == ellipsis_row
                                && pos.x >= left_margin
                                && pos.x < left_margin + ellipsis_text.len() as u16
                        },
                    );

                    let final_style = if is_hovered {
                        border_style.add_modifier(Modifier::REVERSED)
                    } else {
                        border_style
                    };

                    // Write the border with ellipsis
                    self.write_text(
                        buf,
                        ellipsis_row,
                        left_margin,
                        ellipsis_text,
                        area,
                        final_style,
                    );
                    pos.x = left_margin + ellipsis_text.len() as u16;

                    // Track the action with the current path
                    let rect = Rect::new(left_margin, ellipsis_row, ellipsis_text.len() as u16, 1);
                    self.render_cache
                        .actions
                        .push((rect, TuiAction::ExpandBlock(*path)));
                }
            }

            DocumentNode::Conditional { show_when, nodes } => {
                // Interactive renderer is always in interactive mode
                let should_show = match show_when {
                    ShowWhen::Always => true,
                    ShowWhen::Interactive => true,
                    ShowWhen::NonInteractive => false,
                };

                if should_show {
                    for node in nodes {
                        self.render_node(node, area, buf, pos, path, left_margin);
                    }
                }
            }
        }
    }
}
