//! Documentation sources
//!
//! This module defines different sources for rustdoc JSON data:
//! - StdSource: rustup-managed std library docs
//! - LocalSource: workspace-local crates (built on demand)
//! - DocsRsSource: fetched from docs.rs and cached
use crate::{CrateName, RustdocData, navigator::CrateInfo};
use semver::{Version, VersionReq};
use serde::{Deserialize, Deserializer};

mod docsrs;
mod local;
mod std;

use ::std::borrow::Cow;
pub use docsrs::DocsRsSource;
pub use local::LocalSource;
pub use std::StdSource;

#[derive(Deserialize, Debug)]
struct RustdocVersion {
    format_version: u32,
    #[serde(deserialize_with = "option_semver_lenient")]
    crate_version: Option<Version>,
}

fn option_semver_lenient<'de, D>(deserializer: D) -> Result<Option<Version>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<Cow<'de, str>>::deserialize(deserializer)?;
    Ok(opt.and_then(|s| Version::parse(&s).ok()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrateProvenance {
    Workspace,
    LocalDependency,
    Std,
    DocsRs,
}
impl CrateProvenance {
    pub fn is_workspace(&self) -> bool {
        matches!(self, Self::Workspace)
    }

    pub fn is_local_dependency(&self) -> bool {
        matches!(self, Self::LocalDependency)
    }

    pub fn is_std(&self) -> bool {
        matches!(self, Self::Std)
    }

    pub fn is_docs_rs(&self) -> bool {
        matches!(self, Self::DocsRs)
    }
}

/// Trait for documentation sources
///
/// Each source (std, local workspace, docs.rs) implements this trait to provide:
/// - Name lookup/normalization
/// - Crate loading
/// - Available crate listing (where applicable)
pub trait Source {
    /// Transform a crate name into an internal representation
    ///
    /// This should be cheap (local) and based on already-available information.
    /// Returning None indicates that this Source does not have any information with which to transform the provided name.
    fn canonicalize(&self, input_name: &str) -> Option<CrateName<'static>> {
        let _ = input_name;
        None
    }

    /// Look up a crate by name, returning canonical name and metadata if found
    fn lookup<'a>(&'a self, crate_name: &str, version: &VersionReq) -> Option<Cow<'a, CrateInfo>>;

    /// Load the rustdoc JSON data for a crate (by canonical name)
    fn load(&self, crate_name: &str, version: Option<&Version>) -> Option<RustdocData>;

    /// List all available crates from this source
    /// Returns None if this source doesn't support listing (e.g., DocsRsSource)
    fn list_available<'a>(&'a self) -> Box<dyn Iterator<Item = &'a CrateInfo> + '_> {
        Box::new(::std::iter::empty())
    }
}
