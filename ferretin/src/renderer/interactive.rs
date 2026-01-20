use crate::format_context::FormatContext;
use crate::styled_string::{
    Document, DocumentNode, HeadingLevel, NodePath, Span, SpanStyle, TruncationLevel, TuiAction,
};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEvent,
        MouseEventKind,
    },
    execute, queue,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
};
use std::io::{self, Write, stdout};
use syntect::easy::HighlightLines;
use syntect::util::LinesWithEndings;

/// Input mode for the interactive renderer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    /// Normal browsing mode
    Normal,
    /// Go-to mode (g pressed) - navigate to an item by path
    GoTo,
    /// Search mode (s pressed) - search for items
    Search,
}

/// Entry in navigation history
#[derive(Debug, Clone, PartialEq)]
enum HistoryEntry<'a> {
    /// Regular item navigation
    Item(ferretin_common::DocRef<'a, rustdoc_types::Item>),
    /// Search result page
    Search {
        query: String,
        crate_name: Option<String>,
    },
}

impl<'a> HistoryEntry<'a> {
    /// Get a display name for this history entry
    fn display_name(&self) -> String {
        match self {
            HistoryEntry::Item(item) => item.name().unwrap_or("<unnamed>").to_string(),
            HistoryEntry::Search { query, crate_name } => {
                if let Some(crate_name) = crate_name {
                    format!("\"{}\" in {}", query, crate_name)
                } else {
                    format!("\"{}\"", query)
                }
            }
        }
    }

    /// Get the crate name if this is an item entry
    fn crate_name(&self) -> Option<&str> {
        match self {
            HistoryEntry::Item(item) => Some(item.crate_docs().name()),
            HistoryEntry::Search { crate_name, .. } => crate_name.as_deref(),
        }
    }

    /// Render this history entry to a document
    fn render(&self, request: &'a crate::request::Request) -> Document<'a> {
        match self {
            HistoryEntry::Item(item) => {
                let doc_nodes = request.format_item(*item);
                Document::from(doc_nodes)
            }
            HistoryEntry::Search { query, crate_name } => {
                let (search_doc, _is_error) = crate::commands::search::execute(
                    request,
                    query,
                    20, // limit
                    crate_name.as_deref(),
                );
                search_doc
            }
        }
    }
}

/// Detect if the terminal supports mouse cursor shape changes
fn supports_cursor_shape() -> bool {
    // Kitty, WezTerm, and some other modern terminals support OSC 22
    std::env::var("TERM_PROGRAM")
        .map(|t| t == "kitty" || t == "WezTerm")
        .unwrap_or(false)
        || std::env::var("TERM")
            .map(|t| t.contains("kitty"))
            .unwrap_or(false)
}

/// Set the mouse cursor shape (for terminals that support it)
fn set_cursor_shape(
    backend: &mut CrosstermBackend<std::io::Stdout>,
    shape: &str,
) -> io::Result<()> {
    // OSC 22 sequence: \x1b]22;<shape>\x07
    // Supported shapes: default, pointer, text, etc.
    queue!(
        backend,
        crossterm::style::Print(format!("\x1b]22;{}\x07", shape))
    )?;
    backend.flush()
}

/// Render a document in interactive mode with scrolling and hover tracking
pub fn render_interactive<'a>(
    initial_document: &mut Document<'a>,
    request: &'a crate::request::Request,
    initial_item: Option<ferretin_common::DocRef<'a, rustdoc_types::Item>>,
) -> io::Result<()> {
    let document = initial_document;

    // Navigation history
    let mut history: Vec<HistoryEntry<'a>> = Vec::new();
    let mut history_index: usize = 0;

    // Initialize history with current item if provided
    if let Some(item) = initial_item {
        history.push(HistoryEntry::Item(item));
    }

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Track scroll position and cursor
    let mut scroll_offset = 0u16;
    let mut cursor_pos: Option<(u16, u16)> = None;
    let mut actions: Vec<(Rect, TuiAction)> = Vec::new();
    let mut clicked_position: Option<Position> = None;
    let mut breadcrumb_clickable_areas: Vec<(usize, std::ops::Range<u16>)> = Vec::new(); // (history_idx, col_range)
    let mut breadcrumb_hover_pos: Option<(u16, u16)> = None; // (col, row) for breadcrumb hover
    let supports_cursor = supports_cursor_shape();
    let mut is_hovering = false;
    let mut mouse_enabled = true;
    let mut debug_message =
        String::from("ferretin - q:quit ?:help ←/→:history g:go s:search");

    // Input mode state
    let mut input_mode = InputMode::Normal;
    let mut input_buffer = String::new();
    let mut search_all_crates = false; // Default to searching current crate only
    let mut show_help = false; // Help screen state

    // Main event loop
    let result = loop {
        let format_context = request.format_context();
        let _ = terminal.draw(|frame| {
            // Reserve last 2 lines for status bars
            let main_area = Rect {
                x: frame.area().x,
                y: frame.area().y,
                width: frame.area().width,
                height: frame.area().height.saturating_sub(2),
            };
            let breadcrumb_area = Rect {
                x: frame.area().x,
                y: frame.area().height.saturating_sub(2),
                width: frame.area().width,
                height: 1,
            };
            let status_area = Rect {
                x: frame.area().x,
                y: frame.area().height.saturating_sub(1),
                width: frame.area().width,
                height: 1,
            };

            if show_help {
                // Render help screen (covers entire area including status bars)
                let help_area = frame.area();
                render_help_screen(frame.buffer_mut(), help_area);
            } else {
                // Render main document
                actions = render_document(
                    &document.nodes,
                    &format_context,
                    main_area,
                    frame.buffer_mut(),
                    scroll_offset,
                    cursor_pos,
                );

                // Render breadcrumb bar with full history
                breadcrumb_clickable_areas.clear();
                render_breadcrumb_bar(
                    frame.buffer_mut(),
                    breadcrumb_area,
                    &history,
                    history_index,
                    &mut breadcrumb_clickable_areas,
                    breadcrumb_hover_pos,
                );

                // Get current crate name for search scope display
                let current_crate = history
                    .get(history_index)
                    .and_then(|entry| entry.crate_name());

                // Render status bar
                render_status_bar(
                    frame.buffer_mut(),
                    status_area,
                    &debug_message,
                    input_mode,
                    &input_buffer,
                    search_all_crates,
                    current_crate,
                );
            }
        })?;

        // Update cursor shape based on hover state (both content and breadcrumb)
        if supports_cursor {
            let content_hover = cursor_pos
                .map(|pos| {
                    actions
                        .iter()
                        .any(|(rect, _)| rect.contains(Position::new(pos.0, pos.1)))
                })
                .unwrap_or(false);

            let breadcrumb_hover = breadcrumb_clickable_areas.iter().any(|(_, range)| {
                breadcrumb_hover_pos
                    .map(|(col, _)| range.contains(&col))
                    .unwrap_or(false)
            });

            let now_hovering = content_hover || breadcrumb_hover;

            if now_hovering != is_hovering {
                is_hovering = now_hovering;
                let shape = if is_hovering { "pointer" } else { "default" };
                let _ = set_cursor_shape(terminal.backend_mut(), shape);
            }
        }

        // Update debug message with hover info
        if mouse_enabled {
            if let Some(pos) = cursor_pos {
                if let Some((_, action)) = actions
                    .iter()
                    .find(|(rect, _)| rect.contains(Position::new(pos.0, pos.1)))
                {
                    debug_message = match action {
                        TuiAction::Navigate(doc_ref) => {
                            if let Some(path) = doc_ref.path() {
                                format!("Navigate: {}", path)
                            } else if let Some(name) = doc_ref.name() {
                                format!("Navigate: {}", name)
                            } else {
                                "Navigate: <unknown>".to_string()
                            }
                        }
                        TuiAction::ExpandBlock(path) => {
                            format!("Expand: {:?}", path.indices())
                        }
                        TuiAction::OpenUrl(url) => {
                            format!("Open: {}", url)
                        }
                    };
                } else {
                    debug_message = format!(
                        "Pos: ({}, {}) | Scroll: {} | Mouse: ON (m to disable)",
                        pos.0, pos.1, scroll_offset
                    );
                }
            }
        } else {
            debug_message = "Mouse: OFF (text selection enabled - m to re-enable)".to_string();
        }

        // Handle any clicked action from previous iteration
        if let Some(click_pos) = clicked_position.take() {
            let action_opt = actions
                .iter()
                .find(|(rect, _)| rect.contains(click_pos))
                .map(|(_, action)| action.clone());

            if let Some(action) = action_opt {
                debug_message = format!(
                    "Clicked: {:?}",
                    match &action {
                        TuiAction::Navigate(doc_ref) => doc_ref
                            .path()
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "unknown".to_string()),
                        TuiAction::ExpandBlock(path) => format!("{:?}", path.indices()),
                        TuiAction::OpenUrl(url) => url.clone(),
                    }
                );

                if let Some((new_doc, doc_ref)) = handle_action(document, &action, request) {
                    *document = new_doc;
                    scroll_offset = 0; // Reset scroll to top of new document

                    let new_entry = HistoryEntry::Item(doc_ref);
                    // Add to history if not a duplicate of current position
                    if history.is_empty() || history.get(history_index) != Some(&new_entry) {
                        // Truncate history after current position (discard forward history)
                        history.truncate(history_index + 1);
                        // Add new item
                        history.push(new_entry);
                        history_index = history.len() - 1;
                    }

                    debug_message = format!(
                        "Navigated to: {}",
                        doc_ref
                            .path()
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "?".to_string())
                    );
                }
            }
        }

        // Handle events with timeout for hover updates
        if event::poll(std::time::Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    // Always allow Escape to exit help, cancel input mode, or quit
                    if key.code == KeyCode::Esc {
                        if show_help {
                            show_help = false;
                        } else if input_mode != InputMode::Normal {
                            input_mode = InputMode::Normal;
                            input_buffer.clear();
                            debug_message =
                                "ferretin - q:quit ?:help ←/→:history g:go s:search"
                                    .to_string();
                        } else {
                            break Ok(());
                        }
                    }
                    // Handle help screen
                    else if show_help {
                        // Any key (except Escape, handled above) exits help
                        show_help = false;
                    }
                    // Handle input mode
                    else if input_mode != InputMode::Normal {
                        match key.code {
                            KeyCode::Char(c) => {
                                input_buffer.push(c);
                            }
                            KeyCode::Backspace => {
                                input_buffer.pop();
                            }
                            KeyCode::Tab => {
                                // Toggle search scope (only in Search mode)
                                if input_mode == InputMode::Search {
                                    search_all_crates = !search_all_crates;
                                }
                            }
                            KeyCode::Enter => {
                                // Execute the command
                                match input_mode {
                                    InputMode::GoTo => {
                                        let mut suggestions = vec![];
                                        if let Some(item) =
                                            request.resolve_path(&input_buffer, &mut suggestions)
                                        {
                                            let doc_nodes = request.format_item(item);
                                            *document = Document::from(doc_nodes);
                                            scroll_offset = 0;

                                            let new_entry = HistoryEntry::Item(item);
                                            // Add to history
                                            if history.is_empty()
                                                || history.get(history_index) != Some(&new_entry)
                                            {
                                                history.truncate(history_index + 1);
                                                history.push(new_entry);
                                                history_index = history.len() - 1;
                                            }

                                            debug_message = format!(
                                                "Navigated to: {}",
                                                item.path()
                                                    .map(|p| p.to_string())
                                                    .unwrap_or_else(|| "?".to_string())
                                            );
                                        } else {
                                            debug_message = format!("Not found: {}", input_buffer);
                                        }
                                    }
                                    InputMode::Search => {
                                        // Determine search scope (clone to avoid borrow issues)
                                        let search_crate = if search_all_crates {
                                            None
                                        } else {
                                            history
                                                .get(history_index)
                                                .and_then(|entry| entry.crate_name())
                                                .map(|s| s.to_string())
                                        };

                                        // Execute search
                                        let (search_doc, is_error) =
                                            crate::commands::search::execute(
                                                request,
                                                &input_buffer,
                                                20, // limit
                                                search_crate.as_deref(),
                                            );
                                        *document = search_doc;
                                        scroll_offset = 0;

                                        if is_error {
                                            debug_message =
                                                format!("No results for: {}", input_buffer);
                                        } else {
                                            // Add search to history
                                            let new_entry = HistoryEntry::Search {
                                                query: input_buffer.clone(),
                                                crate_name: search_crate.clone(),
                                            };

                                            if history.is_empty()
                                                || history.get(history_index) != Some(&new_entry)
                                            {
                                                history.truncate(history_index + 1);
                                                history.push(new_entry);
                                                history_index = history.len() - 1;
                                            }

                                            let scope = if search_all_crates {
                                                "all crates"
                                            } else {
                                                search_crate.as_deref().unwrap_or("current crate")
                                            };
                                            debug_message = format!(
                                                "Search results in {}: {}",
                                                scope, input_buffer
                                            );
                                        }
                                    }
                                    InputMode::Normal => unreachable!(),
                                }
                                input_mode = InputMode::Normal;
                                input_buffer.clear();
                            }
                            _ => {}
                        }
                    }
                    // Normal mode keybindings
                    else {
                        match (key.code, key.modifiers) {
                            // Quit
                            (KeyCode::Char('q'), _)
                            | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                                break Ok(());
                            }

                            // Scroll down
                            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => {
                                scroll_offset = scroll_offset.saturating_add(1);
                            }

                            // Scroll up
                            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => {
                                scroll_offset = scroll_offset.saturating_sub(1);
                            }

                            // Page down
                            (KeyCode::Char('d'), KeyModifiers::CONTROL)
                            | (KeyCode::PageDown, _) => {
                                let page_size = terminal.size()?.height / 2;
                                scroll_offset = scroll_offset.saturating_add(page_size);
                            }

                            // Page up
                            (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
                                let page_size = terminal.size()?.height / 2;
                                scroll_offset = scroll_offset.saturating_sub(page_size);
                            }

                            // Jump to top
                            (KeyCode::Home, _) => {
                                scroll_offset = 0;
                            }

                            // Jump to bottom (will clamp in render)
                            (KeyCode::Char('G'), KeyModifiers::SHIFT) | (KeyCode::End, _) => {
                                scroll_offset = 10000; // Large number, will clamp
                            }

                            // Enter GoTo mode
                            (KeyCode::Char('g'), _) => {
                                input_mode = InputMode::GoTo;
                                input_buffer.clear();
                            }

                            // Enter Search mode
                            (KeyCode::Char('s'), _) => {
                                input_mode = InputMode::Search;
                                input_buffer.clear();
                                search_all_crates = false; // Default to current crate
                            }

                            // Toggle mouse mode for text selection
                            (KeyCode::Char('m'), _) => {
                                mouse_enabled = !mouse_enabled;
                                if mouse_enabled {
                                    let _ = execute!(terminal.backend_mut(), EnableMouseCapture);
                                    debug_message = "Mouse enabled (hover/click)".to_string();
                                } else {
                                    let _ = execute!(terminal.backend_mut(), DisableMouseCapture);
                                    cursor_pos = None; // Clear cursor position
                                    debug_message =
                                        "Mouse disabled (text selection enabled)".to_string();
                                }
                            }

                            // Show help
                            (KeyCode::Char('?'), _) | (KeyCode::Char('h'), _) => {
                                show_help = true;
                            }

                            // Navigate back
                            (KeyCode::Left, _) => {
                                if history_index > 0 {
                                    history_index -= 1;
                                    let entry = &history[history_index];
                                    *document = entry.render(request);
                                    scroll_offset = 0;
                                    debug_message = format!("Back to: {}", entry.display_name());
                                } else {
                                    debug_message = "Already at beginning of history".to_string();
                                }
                            }

                            // Navigate forward
                            (KeyCode::Right, _) => {
                                if history_index + 1 < history.len() {
                                    history_index += 1;
                                    let entry = &history[history_index];
                                    *document = entry.render(request);
                                    scroll_offset = 0;
                                    debug_message = format!("Forward to: {}", entry.display_name());
                                } else {
                                    debug_message = "Already at end of history".to_string();
                                }
                            }

                            _ => {}
                        }
                    }
                }

                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Moved,
                    column,
                    row,
                    ..
                }) if mouse_enabled => {
                    // Track cursor for hover effects
                    let terminal_height = terminal.size()?.height;
                    let content_height = terminal_height.saturating_sub(2); // Exclude 2 status lines
                    let breadcrumb_row = terminal_height.saturating_sub(2);

                    if row < content_height {
                        // Mouse in main content area
                        cursor_pos = Some((column, row + scroll_offset));
                        breadcrumb_hover_pos = None;
                    } else if row == breadcrumb_row {
                        // Mouse over breadcrumb bar - check if hovering over a clickable item
                        cursor_pos = None;
                        let hovering_breadcrumb = breadcrumb_clickable_areas
                            .iter()
                            .any(|(_, range)| range.contains(&column));

                        // Track hover position for visual feedback
                        breadcrumb_hover_pos = if hovering_breadcrumb {
                            Some((column, row))
                        } else {
                            None
                        };
                    } else {
                        // Mouse over status bar
                        cursor_pos = None;
                        breadcrumb_hover_pos = None;
                    }
                }

                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    ..
                }) if mouse_enabled => {
                    scroll_offset = scroll_offset.saturating_add(1);
                }

                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollUp,
                    ..
                }) if mouse_enabled => {
                    scroll_offset = scroll_offset.saturating_sub(1);
                }

                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(_),
                    column,
                    row,
                    ..
                }) if mouse_enabled => {
                    let terminal_height = terminal.size()?.height;
                    let content_height = terminal_height.saturating_sub(2); // Exclude 2 status lines
                    let breadcrumb_row = terminal_height.saturating_sub(2);

                    if row < content_height {
                        // Click in main content area
                        clicked_position = Some(Position::new(column, row + scroll_offset));
                    } else if row == breadcrumb_row {
                        // Click on breadcrumb bar - check if clicking a history item
                        if let Some((idx, _)) = breadcrumb_clickable_areas
                            .iter()
                            .find(|(_, range)| range.contains(&column))
                        {
                            // Jump to this history position
                            history_index = *idx;
                            let entry = &history[history_index];
                            *document = entry.render(request);
                            scroll_offset = 0;
                            debug_message = format!("Jumped to: {}", entry.display_name());
                        }
                    }
                }

                _ => {}
            }
        }
    };

    // Clean up terminal
    disable_raw_mode()?;

    // Restore default cursor shape before exiting
    if supports_cursor {
        let _ = set_cursor_shape(terminal.backend_mut(), "default");
    }

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Handle a TuiAction, returning (new Document, DocRef) if navigation occurred
fn handle_action<'a>(
    document: &mut Document<'a>,
    action: &TuiAction<'a>,
    request: &'a crate::request::Request,
) -> Option<(
    Document<'a>,
    ferretin_common::DocRef<'a, rustdoc_types::Item>,
)> {
    match action {
        TuiAction::ExpandBlock(path) => {
            // Find the node at this path and expand it
            if let Some(node) = find_node_at_path_mut(&mut document.nodes, path.indices())
                && let DocumentNode::TruncatedBlock { level, .. } = node
            {
                // Cycle through truncation levels: SingleLine -> Full
                *level = match level {
                    TruncationLevel::SingleLine | TruncationLevel::Brief => TruncationLevel::Full,
                    TruncationLevel::Full => TruncationLevel::Full, // Already expanded
                };
            }
            None // No new document, just mutated in place
        }
        TuiAction::Navigate(doc_ref) => {
            // Format the item directly without path lookup
            let doc_nodes = request.format_item(*doc_ref);
            Some((Document::from(doc_nodes), *doc_ref))
        }
        TuiAction::OpenUrl(url) => {
            // Open external URL in browser
            if let Err(e) = webbrowser::open(url) {
                eprintln!("[ERROR] Failed to open URL {}: {}", url, e);
            }
            None // No new document
        }
    }
}

/// Find a mutable node at the given path
fn find_node_at_path_mut<'a, 'b>(
    nodes: &'a mut [DocumentNode<'b>],
    path: &[u16],
) -> Option<&'a mut DocumentNode<'b>> {
    if path.is_empty() {
        return None;
    }

    let idx = path[0] as usize;
    if idx >= nodes.len() {
        return None;
    }

    if path.len() == 1 {
        // This is the target node
        return Some(&mut nodes[idx]);
    }

    // Recurse into children
    let remaining_path = &path[1..];
    match &mut nodes[idx] {
        DocumentNode::Section { nodes, .. }
        | DocumentNode::BlockQuote { nodes }
        | DocumentNode::TruncatedBlock { nodes, .. } => {
            find_node_at_path_mut(nodes, remaining_path)
        }
        DocumentNode::List { items } => {
            // Path into list items
            if remaining_path.is_empty() {
                return None;
            }
            let item_idx = remaining_path[0] as usize;
            if item_idx >= items.len() {
                return None;
            }
            find_node_at_path_mut(&mut items[item_idx].content, &remaining_path[1..])
        }
        _ => None,
    }
}

/// Find the best truncation point for Brief mode at second paragraph break
/// Returns the node index to stop at, or None to fall back to line-based truncation
fn find_paragraph_truncation_point(
    nodes: &[DocumentNode],
    max_lines: u16,
    screen_width: u16,
) -> Option<usize> {
    let mut paragraph_breaks = 0;
    let mut estimated_lines = 0u16;
    let mut consecutive_newlines = 0;

    for (idx, node) in nodes.iter().enumerate() {
        // Estimate lines this node will take
        estimated_lines += estimate_node_lines(node, screen_width);

        // Track newlines across span boundaries
        if let DocumentNode::Span(span) = node {
            // Count newlines at the start of this span
            for ch in span.text.chars() {
                if ch == '\n' {
                    consecutive_newlines += 1;
                    // Two or more consecutive newlines = paragraph break
                    if consecutive_newlines >= 2 {
                        paragraph_breaks += 1;

                        // Found second paragraph break - truncate here if within line limit
                        if paragraph_breaks >= 2 {
                            if estimated_lines <= max_lines {
                                return Some(idx);
                            } else {
                                // Second paragraph is too long, fall back to line limit
                                return None;
                            }
                        }

                        // Reset counter after detecting a break
                        consecutive_newlines = 0;
                    }
                } else if !ch.is_whitespace() {
                    // Non-whitespace resets the counter
                    consecutive_newlines = 0;
                }
            }
        } else {
            // Non-span nodes reset newline tracking
            consecutive_newlines = 0;
        }
    }

    // Didn't find second paragraph break
    None
}

/// Estimate how many lines a node will consume when rendered
fn estimate_node_lines(node: &DocumentNode, screen_width: u16) -> u16 {
    match node {
        DocumentNode::Span(span) => {
            // Count explicit newlines + word wrapping
            let text_len = span.text.len() as u16;
            let newline_count = span.text.matches('\n').count() as u16;
            let wrapped_lines = if screen_width > 0 {
                (text_len + screen_width - 1) / screen_width // Ceiling division
            } else {
                1
            };
            newline_count.max(1) + wrapped_lines.saturating_sub(1)
        }
        DocumentNode::Heading { .. } => 3, // Title + underline + spacing
        DocumentNode::CodeBlock { code, .. } => {
            code.lines().count() as u16 + 2 // Lines + spacing
        }
        DocumentNode::HorizontalRule => 1,
        DocumentNode::List { items } => items.len() as u16, // Rough estimate
        _ => 2,                                             // Default estimate for other nodes
    }
}

/// Render breadcrumb bar showing full navigation history
fn render_breadcrumb_bar<'a>(
    buf: &mut Buffer,
    area: Rect,
    history: &[HistoryEntry<'a>],
    current_idx: usize,
    clickable_areas: &mut Vec<(usize, std::ops::Range<u16>)>,
    hover_pos: Option<(u16, u16)>,
) {
    let bg_style = Style::default().bg(Color::Blue).fg(Color::White);

    // Clear the breadcrumb line
    for x in 0..area.width {
        buf.cell_mut((x, area.y)).unwrap().reset();
        buf.cell_mut((x, area.y)).unwrap().set_style(bg_style);
    }

    if history.is_empty() {
        let text = "📍 <no history>";
        for (i, ch) in text.chars().enumerate() {
            if i >= area.width as usize {
                break;
            }
            buf.cell_mut((i as u16, area.y))
                .unwrap()
                .set_char(ch)
                .set_style(bg_style);
        }
        return;
    }

    // Build breadcrumb trail: a → b → c with current item italicized
    let mut col = 0u16;

    // Start with icon
    let icon = "📍 ";
    for ch in icon.chars() {
        if col >= area.width {
            break;
        }
        buf.cell_mut((col, area.y))
            .unwrap()
            .set_char(ch)
            .set_style(bg_style);
        col += 1;
    }

    for (idx, item) in history.iter().enumerate() {
        if col >= area.width {
            break;
        }

        // Add arrow separator (except for first item)
        if idx > 0 {
            let arrow = " → ";
            for ch in arrow.chars() {
                if col >= area.width {
                    break;
                }
                buf.cell_mut((col, area.y))
                    .unwrap()
                    .set_char(ch)
                    .set_style(bg_style);
                col += 1;
            }
        }

        // Render item name with appropriate style
        let name = item.display_name();
        let start_col = col;
        let name_len = name.chars().count().min((area.width - start_col) as usize);
        let end_col = start_col + name_len as u16;

        // Check if this item is being hovered
        let is_hovered = hover_pos.map_or(false, |(hover_col, _)| {
            hover_col >= start_col && hover_col < end_col
        });

        let item_style = if is_hovered {
            // Hovered: reversed colors for visual feedback
            bg_style.add_modifier(Modifier::REVERSED)
        } else if idx == current_idx {
            // Current item: italic
            bg_style.add_modifier(Modifier::ITALIC)
        } else {
            // Other items: normal
            bg_style
        };

        for ch in name.chars() {
            if col >= area.width {
                break;
            }
            buf.cell_mut((col, area.y))
                .unwrap()
                .set_char(ch)
                .set_style(item_style);
            col += 1;
        }

        // Track clickable area for this item
        if end_col > start_col {
            clickable_areas.push((idx, start_col..end_col));
        }
    }
}

/// Render status bar at the bottom of the screen
fn render_status_bar(
    buf: &mut Buffer,
    area: Rect,
    message: &str,
    input_mode: InputMode,
    input_buffer: &str,
    search_all_crates: bool,
    current_crate: Option<&str>,
) {
    let style = Style::default().bg(Color::DarkGray).fg(Color::White);
    let hint_style = Style::default().bg(Color::DarkGray).fg(Color::Gray);

    // Clear the status line
    for x in 0..area.width {
        buf.cell_mut((x, area.y)).unwrap().reset();
        buf.cell_mut((x, area.y)).unwrap().set_style(style);
    }

    // Determine what to display based on input mode
    let (display_text, hint_text) = match input_mode {
        InputMode::Normal => (message.to_string(), None),
        InputMode::GoTo => (format!("Go to: {}", input_buffer), None),
        InputMode::Search => {
            let scope = if search_all_crates {
                "all crates".to_string()
            } else {
                current_crate
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "current crate".to_string())
            };
            (
                format!("Search in {}: {}", scope, input_buffer),
                Some("[tab] toggle scope"),
            )
        }
    };

    // Calculate space for hint text
    let hint_len = hint_text.as_ref().map(|h| h.len()).unwrap_or(0);
    let available_width = area.width as usize;
    let text_max_width = if hint_len > 0 {
        available_width.saturating_sub(hint_len + 2) // +2 for spacing
    } else {
        available_width
    };

    // Render main text (truncate if needed)
    let truncated = if display_text.len() > text_max_width {
        &display_text[..text_max_width]
    } else {
        &display_text
    };

    let mut col = 0u16;
    for ch in truncated.chars() {
        if col >= area.width {
            break;
        }
        buf.cell_mut((col, area.y))
            .unwrap()
            .set_char(ch)
            .set_style(style);
        col += 1;
    }

    // Render right-justified hint text if present
    if let Some(hint) = hint_text {
        let hint_start = (area.width as usize).saturating_sub(hint.len()) as u16;
        let mut hint_col = hint_start;
        for ch in hint.chars() {
            if hint_col >= area.width {
                break;
            }
            buf.cell_mut((hint_col, area.y))
                .unwrap()
                .set_char(ch)
                .set_style(hint_style);
            hint_col += 1;
        }
    }
}

/// Render help screen showing all available keybindings
fn render_help_screen(buf: &mut Buffer, area: Rect) {
    let bg_style = Style::default().bg(Color::Black).fg(Color::White);
    let title_style = Style::default()
        .bg(Color::Black)
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default()
        .bg(Color::Black)
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().bg(Color::Black).fg(Color::White);

    // Clear the entire screen
    for y in 0..area.height {
        for x in 0..area.width {
            buf.cell_mut((x, y)).unwrap().reset();
            buf.cell_mut((x, y)).unwrap().set_style(bg_style);
        }
    }

    let help_text = vec![
        ("", "FERRETIN INTERACTIVE MODE - KEYBINDINGS", title_style),
        ("", "", bg_style),
        ("Navigation:", "", title_style),
        ("  j, ↓", "Scroll down", key_style),
        ("  k, ↑", "Scroll up", key_style),
        ("  Ctrl+d, PgDn", "Page down", key_style),
        ("  Ctrl+u, PgUp", "Page up", key_style),
        ("  Home", "Jump to top", key_style),
        ("  Shift+G, End", "Jump to bottom", key_style),
        ("  ←", "Navigate back in history", key_style),
        ("  →", "Navigate forward in history", key_style),
        ("", "", bg_style),
        ("Commands:", "", title_style),
        ("  g", "Go to item by path", key_style),
        ("  s", "Search (scoped to current crate)", key_style),
        ("    Tab", "  Toggle search scope (current/all crates)", key_style),
        ("  Esc", "Cancel input mode / Exit help / Quit", key_style),
        ("", "", bg_style),
        ("Mouse:", "", title_style),
        ("  m", "Toggle mouse mode (for text selection)", key_style),
        ("  Click", "Navigate to item / Expand block", key_style),
        ("  Hover", "Show preview in status bar", key_style),
        ("  Scroll", "Scroll content", key_style),
        ("", "", bg_style),
        ("Help:", "", title_style),
        ("  ?, h", "Show this help screen", key_style),
        ("", "", bg_style),
        ("Other:", "", title_style),
        ("  q, Ctrl+c", "Quit", key_style),
        ("", "", bg_style),
        ("", "Press any key to close help", desc_style),
    ];

    // Calculate maximum width for consistent formatting
    let max_width = help_text
        .iter()
        .map(|(key, desc, _)| {
            if key.is_empty() {
                desc.len()
            } else {
                format!("{:20} {}", key, desc).len()
            }
        })
        .max()
        .unwrap_or(60);

    let start_row = (area.height.saturating_sub(help_text.len() as u16)) / 2;
    let start_col = (area.width.saturating_sub(max_width as u16)) / 2;

    for (i, (key, desc, style)) in help_text.iter().enumerate() {
        let row = start_row + i as u16;
        if row >= area.height {
            break;
        }

        let text = if key.is_empty() {
            format!("{:width$}", desc, width = max_width)
        } else {
            format!("{:20} {:width$}", key, desc, width = max_width - 21)
        };

        let mut col = start_col;
        for ch in text.chars() {
            if col >= area.width {
                break;
            }
            buf.cell_mut((col, row))
                .unwrap()
                .set_char(ch)
                .set_style(*style);
            col += 1;
        }
    }
}

/// Render document nodes to buffer, returning action map
/// The lifetime 'doc is for the document nodes, 'action is for the TuiActions (from Request)
fn render_document<'a>(
    nodes: &[DocumentNode<'a>],
    format_context: &FormatContext,
    area: Rect,
    buf: &mut Buffer,
    scroll: u16,
    cursor_pos: Option<(u16, u16)>,
) -> Vec<(Rect, TuiAction<'a>)> {
    let mut actions = Vec::new();
    let mut row = 0u16;
    let mut col = 0u16;

    for (idx, node) in nodes.iter().enumerate() {
        if row >= area.height + scroll {
            break; // Past visible area
        }

        // Create a fresh path for each top-level node
        let mut node_path = NodePath::new();
        node_path.push(idx);
        render_node(
            node,
            format_context,
            area,
            buf,
            &mut row,
            &mut col,
            scroll,
            cursor_pos,
            &mut actions,
            &node_path,
        );
    }

    actions
}

/// Render a single node
#[allow(clippy::too_many_arguments)]
fn render_node<'a>(
    node: &DocumentNode<'a>,
    format_context: &FormatContext,
    area: Rect,
    buf: &mut Buffer,
    row: &mut u16,
    col: &mut u16,
    scroll: u16,
    cursor_pos: Option<(u16, u16)>,
    actions: &mut Vec<(Rect, TuiAction<'a>)>,
    path: &crate::styled_string::NodePath,
) {
    match node {
        DocumentNode::Span(span) => {
            render_span(
                span,
                format_context,
                area,
                buf,
                row,
                col,
                scroll,
                cursor_pos,
                actions,
            );
        }

        DocumentNode::Heading { level, spans } => {
            // Start new line if not at beginning
            if *col > 0 {
                *row += 1;
                *col = 0;
            }

            // Render heading spans (bold)
            for span in spans {
                render_span_with_modifier(
                    span,
                    Modifier::BOLD,
                    format_context,
                    area,
                    buf,
                    row,
                    col,
                    scroll,
                    cursor_pos,
                    actions,
                );
            }

            // New line after heading
            *row += 1;
            *col = 0;

            // Add decorative underline
            let underline_char = match level {
                HeadingLevel::Title => '=',
                HeadingLevel::Section => '-',
            };

            if *row >= scroll && *row < scroll + area.height {
                for c in 0..area.width {
                    buf.cell_mut((c, *row - scroll))
                        .unwrap()
                        .set_char(underline_char);
                }
            }

            *row += 1;
            *col = 0;
        }

        DocumentNode::List { items } => {
            for (item_idx, item) in items.iter().enumerate() {
                // Start new line
                if *col > 0 {
                    *row += 1;
                    *col = 0;
                }

                // Bullet
                write_text(buf, *row, *col, "  • ", scroll, area, Style::default());
                *col += 4;

                // Label (if any)
                if let Some(label_spans) = &item.label {
                    for span in label_spans {
                        render_span_with_modifier(
                            span,
                            Modifier::BOLD,
                            format_context,
                            area,
                            buf,
                            row,
                            col,
                            scroll,
                            cursor_pos,
                            actions,
                        );
                    }
                }

                // Content
                for (content_idx, content_node) in item.content.iter().enumerate() {
                    let mut content_path = *path;
                    content_path.push(item_idx);
                    content_path.push(content_idx);
                    render_node(
                        content_node,
                        format_context,
                        area,
                        buf,
                        row,
                        col,
                        scroll,
                        cursor_pos,
                        actions,
                        &content_path,
                    );
                }

                *row += 1;
                *col = 0;
            }
        }

        DocumentNode::Section { title, nodes } => {
            if let Some(title_spans) = title {
                if *col > 0 {
                    *row += 1;
                    *col = 0;
                }

                for span in title_spans {
                    render_span_with_modifier(
                        span,
                        Modifier::BOLD,
                        format_context,
                        area,
                        buf,
                        row,
                        col,
                        scroll,
                        cursor_pos,
                        actions,
                    );
                }

                *row += 1;
                *col = 0;
            }

            for (idx, child_node) in nodes.iter().enumerate() {
                let mut child_path = *path;
                child_path.push(idx);
                render_node(
                    child_node,
                    format_context,
                    area,
                    buf,
                    row,
                    col,
                    scroll,
                    cursor_pos,
                    actions,
                    &child_path,
                );
            }
        }

        DocumentNode::CodeBlock { lang, code } => {
            if *col > 0 {
                *row += 1;
                *col = 0;
            }

            render_code_block(
                lang.as_deref(),
                code,
                format_context,
                area,
                buf,
                row,
                scroll,
            );

            *row += 1;
            *col = 0;
        }

        DocumentNode::Link { url, text, item } => {
            // Determine the action based on whether this is an internal or external link
            let action = if let Some(doc_ref) = item {
                TuiAction::Navigate(*doc_ref)
            } else {
                TuiAction::OpenUrl(url.clone())
            };

            // Render underlined text with the action attached
            for span in text {
                let span_with_action = Span {
                    text: span.text.clone(),
                    style: span.style,
                    action: Some(action.clone()),
                };
                render_span_with_modifier(
                    &span_with_action,
                    Modifier::UNDERLINED,
                    format_context,
                    area,
                    buf,
                    row,
                    col,
                    scroll,
                    cursor_pos,
                    actions,
                );
            }
        }

        DocumentNode::HorizontalRule => {
            if *col > 0 {
                *row += 1;
                *col = 0;
            }

            if *row >= scroll && *row < scroll + area.height {
                for c in 0..area.width {
                    buf.cell_mut((c, *row - scroll)).unwrap().set_char('─');
                }
            }

            *row += 1;
            *col = 0;
        }

        DocumentNode::BlockQuote { nodes } => {
            for (idx, child_node) in nodes.iter().enumerate() {
                if *col == 0 {
                    write_text(buf, *row, *col, "  │ ", scroll, area, Style::default());
                    *col += 4;
                }

                let mut child_path = *path;
                child_path.push(idx);
                render_node(
                    child_node,
                    format_context,
                    area,
                    buf,
                    row,
                    col,
                    scroll,
                    cursor_pos,
                    actions,
                    &child_path,
                );
            }
        }

        DocumentNode::Table { .. } => {
            if *col > 0 {
                *row += 1;
                *col = 0;
            }

            write_text(buf, *row, *col, "[Table]", scroll, area, Style::default());

            *row += 1;
            *col = 0;
        }

        DocumentNode::TruncatedBlock { nodes, level } => {
            // Determine line limit based on truncation level
            let line_limit = match level {
                TruncationLevel::SingleLine => 3,  // Show ~3 lines for single-line
                TruncationLevel::Brief => 8,       // Show ~8 lines for brief (actual wrapped lines)
                TruncationLevel::Full => u16::MAX, // Show everything
            };

            let start_row = *row;
            let mut rendered_all = true;

            // For Brief mode, try to find a good truncation point at second paragraph break
            let truncate_at = if matches!(level, TruncationLevel::Brief) {
                find_paragraph_truncation_point(nodes, line_limit, area.width)
            } else {
                None
            };

            // Render nodes until we hit the truncation point or line limit
            for (idx, child_node) in nodes.iter().enumerate() {
                // Check if we've hit our truncation point
                if let Some(cutoff) = truncate_at {
                    if idx >= cutoff {
                        rendered_all = false;
                        break;
                    }
                }

                // Check if we've exceeded the line limit (fallback)
                if *row - start_row >= line_limit && !matches!(level, TruncationLevel::Full) {
                    rendered_all = false;
                    break;
                }

                let mut child_path = *path;
                child_path.push(idx);
                render_node(
                    child_node,
                    format_context,
                    area,
                    buf,
                    row,
                    col,
                    scroll,
                    cursor_pos,
                    actions,
                    &child_path,
                );

                // If this is the last node, we rendered everything
                if idx == nodes.len() - 1 {
                    rendered_all = true;
                }
            }

            // Only show [...] if we didn't render all nodes and not already Full
            if !rendered_all && !matches!(level, TruncationLevel::Full) {
                let start_col = *col;
                let ellipsis_row = *row;
                let ellipsis_text = " [...]";

                // Style for dimmed ellipsis
                let style = Style::default().fg(Color::DarkGray);

                // Check if hovered
                let is_hovered = cursor_pos.map_or_else(
                    || false,
                    |(cx, cy)| {
                        cy == ellipsis_row
                            && cx >= start_col
                            && cx < start_col + ellipsis_text.len() as u16
                    },
                );

                let final_style = if is_hovered {
                    style.add_modifier(Modifier::REVERSED)
                } else {
                    style
                };

                // Write the text
                write_text(
                    buf,
                    ellipsis_row,
                    *col,
                    ellipsis_text,
                    scroll,
                    area,
                    final_style,
                );
                *col += ellipsis_text.len() as u16;

                // Track the action with the current path
                let rect = Rect::new(start_col, ellipsis_row, ellipsis_text.len() as u16, 1);
                actions.push((rect, TuiAction::ExpandBlock(*path)));
            }
        }
    }
}

/// Render a span with optional action tracking
#[allow(clippy::too_many_arguments)]
fn render_span<'a>(
    span: &Span<'a>,
    format_context: &FormatContext,
    area: Rect,
    buf: &mut Buffer,
    row: &mut u16,
    col: &mut u16,
    scroll: u16,
    cursor_pos: Option<(u16, u16)>,
    actions: &mut Vec<(Rect, TuiAction<'a>)>,
) {
    render_span_with_modifier(
        span,
        Modifier::empty(),
        format_context,
        area,
        buf,
        row,
        col,
        scroll,
        cursor_pos,
        actions,
    );
}

/// Render a span with additional style modifier
#[allow(clippy::too_many_arguments)]
fn render_span_with_modifier<'a>(
    span: &Span<'a>,
    modifier: Modifier,
    format_context: &FormatContext,
    area: Rect,
    buf: &mut Buffer,
    row: &mut u16,
    col: &mut u16,
    scroll: u16,
    cursor_pos: Option<(u16, u16)>,
    actions: &mut Vec<(Rect, TuiAction<'a>)>,
) {
    let mut style = span_style_to_ratatui(span.style, format_context);
    style = style.add_modifier(modifier);

    let start_col = *col;
    let start_row = *row;

    // Check if this span is hovered
    let is_hovered = if span.action.is_some() {
        cursor_pos.map_or_else(
            || false,
            |(cx, cy)| cy == *row && cx >= *col && cx < *col + span.text.len() as u16,
        )
    } else {
        false
    };

    // If hovered, invert colors
    if is_hovered {
        style = style.add_modifier(Modifier::REVERSED);
    }

    // Handle newlines in span text
    for (line_idx, line) in span.text.split('\n').enumerate() {
        if line_idx > 0 {
            *row += 1;
            *col = 0;
        }

        // Word wrap if line is too long
        let mut remaining = line;
        while !remaining.is_empty() {
            let available_width = area.width.saturating_sub(*col);

            if available_width == 0 {
                // No space left on this line, wrap to next
                *row += 1;
                *col = 0;
                continue;
            }

            if remaining.len() <= available_width as usize {
                // Fits on current line
                write_text(buf, *row, *col, remaining, scroll, area, style);
                *col += remaining.len() as u16;
                break;
            } else {
                // Need to wrap - find last space within available width
                let truncate_at = available_width as usize;
                if let Some(wrap_pos) = remaining[..truncate_at].rfind(char::is_whitespace) {
                    // Wrap at word boundary
                    let (chunk, rest) = remaining.split_at(wrap_pos);
                    write_text(buf, *row, *col, chunk, scroll, area, style);
                    *row += 1;
                    *col = 0;
                    remaining = rest.trim_start(); // Skip leading whitespace on next line
                } else {
                    // No spaces found, hard wrap
                    let (chunk, rest) = remaining.split_at(truncate_at);
                    write_text(buf, *row, *col, chunk, scroll, area, style);
                    *row += 1;
                    *col = 0;
                    remaining = rest;
                }
            }
        }
    }

    // Track action if present
    if let Some(action) = &span.action {
        // Calculate width handling wrapping (col might be less than start_col if we wrapped)
        let width = if *row > start_row {
            // Multi-line span - use full width of first line as clickable area
            area.width.saturating_sub(start_col).max(1)
        } else {
            // Single line - use actual span width
            col.saturating_sub(start_col).max(1)
        };

        let rect = Rect::new(start_col, start_row, width, (*row - start_row + 1).max(1));
        actions.push((rect, action.clone()));
    }
}

/// Write text to buffer at position
fn write_text(
    buf: &mut Buffer,
    row: u16,
    col: u16,
    text: &str,
    scroll: u16,
    area: Rect,
    style: Style,
) {
    if row < scroll || row >= scroll + area.height {
        return; // Outside visible area
    }

    let screen_row = row - scroll;
    let mut current_col = col;

    for ch in text.chars() {
        if current_col >= area.width {
            break; // Past right edge
        }

        if let Some(cell) = buf.cell_mut((current_col, screen_row)) {
            cell.set_char(ch);
            cell.set_style(style);
        }

        current_col += 1;
    }
}

/// Render code block with syntax highlighting
fn render_code_block(
    lang: Option<&str>,
    code: &str,
    format_context: &FormatContext,
    area: Rect,
    buf: &mut Buffer,
    row: &mut u16,
    scroll: u16,
) {
    let lang = match lang {
        Some("no_run") | Some("should_panic") | Some("ignore") | Some("compile_fail")
        | Some("edition2015") | Some("edition2018") | Some("edition2021") | Some("edition2024") => {
            "rust"
        }
        Some(l) => l,
        None => "rust",
    };

    if let Some(syntax) = format_context.syntax_set().find_syntax_by_token(lang) {
        let theme = format_context.theme();
        let mut highlighter = HighlightLines::new(syntax, theme);

        for line in LinesWithEndings::from(code) {
            if *row >= scroll && *row < scroll + area.height {
                let mut col = 0u16;

                if let Ok(ranges) = highlighter.highlight_line(line, format_context.syntax_set()) {
                    for (style, text) in ranges {
                        let fg = style.foreground;
                        let ratatui_style = Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b));
                        write_text(
                            buf,
                            *row,
                            col,
                            text.trim_end_matches('\n'),
                            scroll,
                            area,
                            ratatui_style,
                        );
                        col += text.len() as u16;
                    }
                } else {
                    write_text(
                        buf,
                        *row,
                        0,
                        line.trim_end_matches('\n'),
                        scroll,
                        area,
                        Style::default(),
                    );
                }
            }

            *row += 1;
        }
    } else {
        for line in code.lines() {
            if *row >= scroll && *row < scroll + area.height {
                write_text(buf, *row, 0, line, scroll, area, Style::default());
            }
            *row += 1;
        }
    }
}

/// Convert SpanStyle to ratatui Style
fn span_style_to_ratatui(span_style: SpanStyle, format_context: &FormatContext) -> Style {
    match span_style {
        SpanStyle::Plain => {
            let fg = format_context.color_scheme().default_foreground();
            Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b))
        }
        SpanStyle::Punctuation => Style::default(),
        SpanStyle::Strong => Style::default().add_modifier(Modifier::BOLD),
        SpanStyle::Emphasis => Style::default().add_modifier(Modifier::ITALIC),
        SpanStyle::Strikethrough => Style::default().add_modifier(Modifier::CROSSED_OUT),
        SpanStyle::InlineCode | SpanStyle::InlineRustCode => {
            let color = format_context.color_scheme().color_for(span_style);
            Style::default().fg(Color::Rgb(color.r, color.g, color.b))
        }
        _ => {
            let color = format_context.color_scheme().color_for(span_style);
            Style::default().fg(Color::Rgb(color.r, color.g, color.b))
        }
    }
}
