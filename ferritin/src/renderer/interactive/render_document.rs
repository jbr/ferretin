use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
};

use super::state::InteractiveState;
use crate::styled_string::NodePath;

impl<'a> InteractiveState<'a> {
    /// Render document nodes to buffer, updating action map
    pub(super) fn render_document(&mut self, area: Rect, buf: &mut Buffer) {
        self.render_cache.actions.clear();
        let mut pos = Position { x: 0, y: 0 };

        // Use raw pointer to avoid borrow checker issues when calling render_node
        let nodes_ptr = self.document.document.nodes.as_ptr();
        let node_count = self.document.document.nodes.len();

        for idx in 0..node_count {
            if pos.y >= area.height + self.viewport.scroll_offset {
                break; // Past visible area
            }

            // Create a fresh path for each top-level node
            let mut node_path = NodePath::new();
            node_path.push(idx);

            // SAFETY: idx is bounded by node_count, and nodes_ptr is valid for the duration of this method
            let node = unsafe { &*nodes_ptr.add(idx) };
            self.render_node(
                node, area, buf, &mut pos, &node_path,
                2, // 2-space left margin for breathing room
            );
        }
    }
}
