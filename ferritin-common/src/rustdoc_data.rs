use fieldwork::Fieldwork;
use rustdoc_types::{Crate, ExternalCrate, Id, Item};
use semver::{Version, VersionReq};
use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;
use std::path::PathBuf;

use crate::CrateProvenance;
use crate::doc_ref::{self, DocRef};
use crate::navigator::{Navigator, parse_docsrs_url};

/// Wrapper around rustdoc JSON data that provides convenient query methods
#[derive(Clone, Fieldwork, PartialEq, Eq)]
#[fieldwork(get, rename_predicates)]
pub struct RustdocData {
    pub(crate) crate_data: Crate,
    pub(crate) name: String,
    pub(crate) provenance: CrateProvenance,
    pub(crate) fs_path: PathBuf,
    pub(crate) version: Option<Version>,
}

impl Debug for RustdocData {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RustdocData")
            .field("name", &self.name)
            .field("crate_type", &self.provenance)
            .field("fs_path", &self.fs_path)
            .field("version", &self.version)
            .finish()
    }
}

impl Deref for RustdocData {
    type Target = Crate;

    fn deref(&self) -> &Self::Target {
        &self.crate_data
    }
}

impl RustdocData {
    pub(crate) fn get<'a>(&'a self, navigator: &'a Navigator, id: &Id) -> Option<DocRef<'a, Item>> {
        let item = self.crate_data.index.get(id)?;
        Some(DocRef::new(navigator, self, item))
    }

    pub fn path<'a>(&'a self, id: &Id) -> Option<doc_ref::Path<'a>> {
        self.paths.get(id).map(|summary| summary.into())
    }

    pub fn root_item<'a>(&'a self, navigator: &'a Navigator) -> DocRef<'a, Item> {
        DocRef::new(navigator, self, &self.index[&self.root])
    }

    pub fn traverse_to_crate_by_id<'a>(
        &'a self,
        navigator: &'a Navigator,
        id: u32,
    ) -> Option<&'a RustdocData> {
        if id == 0 {
            //special case: 0 is not in external crates, and it always means "this crate"
            return Some(self);
        }

        let ExternalCrate {
            name,
            html_root_url,
            ..
        } = self.external_crates.get(&id)?;

        let (name, version_req) = html_root_url.as_deref().and_then(parse_docsrs_url).map_or(
            (&**name, VersionReq::STAR),
            |(name, version)| {
                let version_req =
                    VersionReq::parse(&format!("={version}")).unwrap_or(VersionReq::STAR);

                (name, version_req)
            },
        );

        navigator.load_crate(name, &version_req)
    }

    pub(crate) fn get_path<'a>(
        &'a self,
        navigator: &'a Navigator,
        id: Id,
    ) -> Option<DocRef<'a, Item>> {
        let item_summary = self.paths.get(&id)?;
        let crate_ = self.traverse_to_crate_by_id(navigator, item_summary.crate_id)?;

        crate_
            .root_item(navigator)
            .find_by_path(item_summary.path.iter().skip(1))
    }
}
