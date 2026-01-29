use crate::styled_string::Document;

/// Input mode for the interactive renderer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InputMode {
    /// Normal browsing mode
    Normal,
    /// Go-to mode (g pressed) - navigate to an item by path
    GoTo,
    /// Search mode (s pressed) - search for items
    Search,
}

/// Entry in navigation history
#[derive(Debug, Clone, PartialEq)]
pub enum HistoryEntry<'a> {
    /// Regular item navigation
    Item(ferritin_common::DocRef<'a, rustdoc_types::Item>),
    /// Search result page
    Search {
        query: String,
        crate_name: Option<String>,
    },
    /// List crates page
    List,
}

impl<'a> HistoryEntry<'a> {
    /// Get a display name for this history entry
    pub(super) fn display_name(&self) -> String {
        match self {
            HistoryEntry::Item(item) => item.name().unwrap_or("<unnamed>").to_string(),
            HistoryEntry::Search { query, crate_name } => {
                if let Some(crate_name) = crate_name {
                    format!("\"{}\" in {}", query, crate_name)
                } else {
                    format!("\"{}\"", query)
                }
            }
            HistoryEntry::List => "List".to_string(),
        }
    }

    /// Get the crate name if this is an item entry
    pub(super) fn crate_name(&self) -> Option<&str> {
        match self {
            HistoryEntry::Item(item) => Some(item.crate_docs().name()),
            HistoryEntry::Search { crate_name, .. } => crate_name.as_deref(),
            HistoryEntry::List => None,
        }
    }

    /// Render this history entry to a document
    pub(super) fn render(&self, request: &'a crate::request::Request) -> Document<'a> {
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
            HistoryEntry::List => {
                let (list_doc, _is_error) = crate::commands::list::execute(request);
                list_doc
            }
        }
    }
}
