use ratatui::layout::{Position, Rect};
use std::borrow::Cow;
use std::time::Instant;

use super::channels::{RequestResponse, UiCommand};
use super::history::{History, HistoryEntry};
use super::theme::InteractiveTheme;
use super::utils::supports_cursor_shape;
use crate::logging::LogReader;
use crate::render_context::{RenderContext, ThemeError};
use crate::styled_string::{Document, NodePath, TuiAction};
use crossbeam_channel::{Receiver, Sender};

/// UI mode - makes the modal structure of the interface explicit
#[derive(Debug)]
pub(super) enum UiMode<'a> {
    /// Normal browsing mode
    Normal,
    /// Help screen
    Help,
    /// Developer log viewer (undocumented debug feature)
    /// Stores the previous state so we can restore it on exit
    DevLog {
        previous_document: Document<'a>,
        previous_scroll: u16,
    },
    /// Input mode (go-to or search)
    Input(InputMode),
    /// Theme picker modal
    ThemePicker {
        /// Index of currently selected theme
        selected_index: usize,
        /// Theme name to restore on cancel
        saved_theme_name: String,
    },
}

/// Input mode with mode-specific state
#[derive(Debug)]
pub(super) enum InputMode {
    /// Go-to mode (g pressed) - navigate to an item by path
    GoTo { buffer: String },
    /// Search mode (s pressed) - search for items
    Search { buffer: String, all_crates: bool },
}

/// Document and navigation state
#[derive(Debug)]
pub(super) struct DocumentState<'a> {
    pub document: Document<'a>,
    pub history: History<'a>,
}

/// Cached document layout information
#[derive(Debug, Clone, Copy)]
pub(super) struct DocumentLayoutCache {
    pub render_width: u16,
    pub document_height: u16,
}

/// Viewport and scroll tracking
#[derive(Debug)]
pub(super) struct ViewportState {
    pub scroll_offset: u16,
    pub cursor_pos: Option<Position>,
    pub clicked_position: Option<Position>,
    pub cached_layout: Option<DocumentLayoutCache>,
    /// Last known viewport height for scroll clamping
    pub last_viewport_height: u16,
    /// Scrollbar hover/drag state
    pub scrollbar_hovered: bool,
    pub scrollbar_dragging: bool,
}

/// Rendering state computed each frame
#[derive(Debug)]
pub(super) struct RenderCache<'a> {
    pub actions: Vec<(Rect, TuiAction<'a>)>,
}

/// UI display state
#[derive(Debug)]
pub(super) struct UiState {
    pub mouse_enabled: bool,
    pub debug_message: Cow<'static, str>,
    pub is_hovering: bool,
    pub supports_cursor: bool,
    pub include_source: bool,
}

/// Request/response tracking state
#[derive(Debug)]
pub(super) struct LoadingState {
    pub pending_request: bool,
    pub was_loading: bool,
    pub started_at: Instant,
}

impl LoadingState {
    pub fn start(&mut self) {
        self.pending_request = true;
        self.started_at = Instant::now();
    }
}

/// Layout state - cursor position, indentation, and viewport
/// Reset at the start of each frame render
#[derive(Debug)]
pub(super) struct LayoutState {
    pub pos: Position,
    pub indent: u16,
    pub node_path: NodePath,
    pub area: Rect,
    /// Stack of x positions where blockquote markers should be drawn
    /// When rendering content, markers are drawn at each of these positions
    pub blockquote_markers: Vec<u16>,
}

/// Main interactive state - composes all UI state
#[derive(Debug)]
pub(super) struct InteractiveState<'a> {
    pub document: DocumentState<'a>,
    pub viewport: ViewportState,
    pub render_cache: RenderCache<'a>,
    pub layout: LayoutState,
    pub ui_mode: UiMode<'a>,
    pub ui: UiState,
    pub loading: LoadingState,

    // Thread communication
    pub cmd_tx: Sender<UiCommand<'a>>,
    pub resp_rx: Receiver<RequestResponse<'a>>,
    pub log_reader: LogReader,

    // Rendering config
    pub render_context: RenderContext,
    pub theme: InteractiveTheme,
    pub current_theme_name: Option<String>,
}

impl<'a> InteractiveState<'a> {
    /// Create new interactive state from initial components
    pub(super) fn new(
        initial_document: Document<'a>,
        initial_entry: Option<HistoryEntry<'a>>,
        cmd_tx: Sender<UiCommand<'a>>,
        resp_rx: Receiver<RequestResponse<'a>>,
        render_context: RenderContext,
        theme: InteractiveTheme,
        log_reader: LogReader,
    ) -> Self {
        let current_theme_name = render_context
            .current_theme_name()
            .as_ref()
            .map(|s| s.to_string());
        Self {
            document: DocumentState {
                document: initial_document,
                history: History::new(initial_entry),
            },
            viewport: ViewportState {
                scroll_offset: 0,
                cursor_pos: None,
                clicked_position: None,
                cached_layout: None,
                last_viewport_height: 0,
                scrollbar_hovered: false,
                scrollbar_dragging: false,
            },
            render_cache: RenderCache {
                actions: Vec::new(),
            },
            layout: LayoutState {
                pos: Position::default(),
                indent: 0,
                node_path: NodePath::new(),
                area: Rect::default(),
                blockquote_markers: Vec::new(),
            },
            ui_mode: UiMode::Normal,
            ui: UiState {
                mouse_enabled: true,
                debug_message: "ferritin - q:quit ?:help ←/→:history g:go s:search l:list c:code"
                    .into(),
                is_hovering: false,
                supports_cursor: supports_cursor_shape(),
                include_source: false,
            },
            loading: LoadingState {
                pending_request: true,
                was_loading: false,
                started_at: Instant::now(),
            },
            cmd_tx,
            resp_rx,
            log_reader,
            render_context,
            theme,
            current_theme_name,
        }
    }

    pub(super) fn set_debug_message(&mut self, message: impl Into<Cow<'static, str>>) {
        if !self.loading.pending_request {
            self.ui.debug_message = message.into();
        }
    }

    /// Apply a theme by name, rebuilding the interactive theme
    pub(super) fn apply_theme(&mut self, theme_name: &str) -> Result<(), ThemeError> {
        self.render_context.set_theme_name(theme_name)?;
        self.theme = InteractiveTheme::from_render_context(&self.render_context);
        self.current_theme_name = Some(theme_name.to_string());
        Ok(())
    }

    /// Set scroll offset with automatic clamping to valid range
    pub(super) fn set_scroll_offset(&mut self, offset: u16) {
        self.viewport.scroll_offset = offset;
        // Clamp to valid range if we have layout info
        if let Some(cache) = self.viewport.cached_layout {
            let max_scroll = cache
                .document_height
                .saturating_sub(self.viewport.last_viewport_height);
            self.viewport.scroll_offset = self.viewport.scroll_offset.min(max_scroll);
        }
    }

    /// Check if position is in the scrollbar column
    pub(super) fn is_in_scrollbar(&self, pos: Position, content_area_width: u16) -> bool {
        // Scrollbar is at content_area_width (which is frame.width - 1)
        pos.x == content_area_width && pos.y < self.viewport.last_viewport_height
    }

    /// Check if scrollbar should be visible (document taller than viewport)
    pub(super) fn scrollbar_visible(&self) -> bool {
        self.viewport
            .cached_layout
            .map(|cache| cache.document_height > self.viewport.last_viewport_height)
            .unwrap_or(false)
    }
}
