use ferritin_common::Navigator;
use ferritin_common::sources::{DocsRsSource, LocalSource, StdSource};
use std::ops::Deref;
use std::path::PathBuf;

/// MCP-specific wrapper around Navigator that adds formatting capabilities
pub(crate) struct Request {
    navigator: Navigator,
}

impl Deref for Request {
    type Target = Navigator;

    fn deref(&self) -> &Self::Target {
        &self.navigator
    }
}

impl Request {
    /// Create a new request for a manifest path
    pub(crate) fn new(manifest_path: PathBuf) -> Self {
        // Build Navigator with all sources (local will be loaded lazily)
        let navigator = Navigator::default()
            .with_std_source(StdSource::from_rustup())
            .with_local_source(LocalSource::load(&manifest_path).ok())
            .with_docsrs_source(DocsRsSource::from_default_cache());

        Self { navigator }
    }
}
