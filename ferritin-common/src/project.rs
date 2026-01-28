// use crate::navigator::CrateInfo;
// use crate::sources::CrateProvenance;
// use anyhow::{Result, anyhow};
// use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};
// use cargo_toml::Manifest;
// use fieldwork::Fieldwork;
// use std::collections::BTreeMap;
// use std::fmt::{self, Debug, Formatter};
// use std::path::{Path, PathBuf};
// use std::process::Command;

// use crate::crate_name::CrateName;

// /// Metadata about the local workspace - doesn't own documentation data
// /// This can be cached to disk with content hash over Cargo.lock/Cargo.toml

// /// Manages a Cargo project and its rustdoc JSON files
// #[derive(Fieldwork)]
// #[fieldwork(get)]
// pub struct RustdocProject {
//     manifest_path: PathBuf,
//     target_dir: PathBuf,
//     manifest: Manifest,
//     metadata: Metadata,
//     #[field = false]
//     crate_info: Vec<CrateInfo>,
//     workspace_packages: Box<[String]>,
//     #[field = false]
//     available_crates: Vec<String>,
// }

// impl Debug for RustdocProject {
//     fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
//         f.debug_struct("RustdocProject")
//             .field("manifest_path", &self.manifest_path)
//             .field("target_dir", &self.target_dir)
//             .field("crate_info", &self.crate_info)
//             .finish_non_exhaustive()
//     }
// }

// impl RustdocProject {
//     /// Create a new project from a path (directory or Cargo.toml)
//     ///
//     /// If given a directory, walks up to find the workspace root like cargo does.
//     /// If given a Cargo.toml path, uses that directly.
//     pub fn load(path: PathBuf) -> Result<Self> {
//         // Run MetadataCommand once, using either current_dir or manifest_path
//         let metadata = if path.is_dir() {
//             // Use cargo_metadata to find the manifest, which will walk up from current_dir
//             MetadataCommand::new().current_dir(&path).exec()?
//         } else if path.file_name().and_then(|n| n.to_str()) == Some("Cargo.toml") {
//             // It's already a Cargo.toml path
//             if !path.exists() {
//                 return Err(anyhow!("Cargo.toml not found at {}", path.display()));
//             }
//             MetadataCommand::new().manifest_path(&path).exec()?
//         } else {
//             return Err(anyhow!(
//                 "Path must be a directory or Cargo.toml file, got: {}",
//                 path.display()
//             ));
//         };

//         // Get the manifest path from metadata (workspace root)
//         let manifest_path: PathBuf = metadata.workspace_root.join("Cargo.toml").into();

//         let manifest = Manifest::from_path(&manifest_path)?;
//         let project_root = manifest_path
//             .parent()
//             .ok_or_else(|| anyhow!("Invalid manifest path"))?;

//         let target_dir: PathBuf = project_root.join("target");

//         let workspace_packages: Vec<String> = metadata
//             .workspace_packages()
//             .iter()
//             .map(|package| package.name.to_string())
//             .collect();

//         let mut project = Self {
//             manifest_path,
//             manifest,
//             target_dir,
//             metadata,
//             crate_info: vec![],
//             workspace_packages: workspace_packages.into(),
//             available_crates: vec![],
//         };

//         project.crate_info = project.generate_crate_info();
//         project.available_crates = project
//             .crate_info(None)
//             .map(|c| c.name().to_owned())
//             .collect();
//         Ok(project)
//     }

//     pub fn is_workspace_package(&self, crate_name: CrateName<'_>) -> bool {
//         self.workspace_packages
//             .iter()
//             .any(|c| eq_ignoring_dash_underscore(c, &crate_name))
//     }

//     /// Generate documentation for the project or a specific package
//     pub(crate) fn rebuild_docs(&self, crate_name: CrateName<'_>) -> Result<()> {
//         let project_root = self.project_root();

//         let output = Command::new("rustup")
//             .arg("run")
//             .args([
//                 "nightly",
//                 "cargo",
//                 "doc",
//                 "--no-deps",
//                 "--package",
//                 &*crate_name,
//             ])
//             .env("RUSTDOCFLAGS", "-Z unstable-options --output-format=json")
//             .current_dir(project_root)
//             .output()?;

//         if !output.status.success() {
//             let stderr = String::from_utf8_lossy(&output.stderr);
//             return Err(anyhow!("cargo doc failed: {}", stderr));
//         }
//         Ok(())
//     }

//     /// Get available crate names and optional descriptions
//     /// Always generates full workspace view with used_by tracking
//     fn generate_crate_info(&self) -> Vec<CrateInfo> {
//         let mut crates = vec![];
//         let default_crate = self.default_crate_name();

//         // In workspace contexts (>1 package), never alias any crate as "crate"
//         let workspace_packages = self.metadata.workspace_packages();
//         let is_workspace = workspace_packages.len() > 1;

//         // Add workspace members
//         for package in &workspace_packages {
//             crates.push(CrateInfo {
//                 crate_type: CrateProvenance::Workspace,
//                 name: package.name.to_string(),
//                 description: package.description.clone(),
//                 version: Some(package.version.to_string()),
//                 dev_dep: false,
//                 default_crate: !is_workspace
//                     && default_crate
//                         .is_some_and(|dc| eq_ignoring_dash_underscore(&dc, &package.name)),
//                 used_by: vec![], // Workspace members aren't "used by" anyone
//             });
//         }

//         // Collect all dependencies with tracking of which workspace members use them
//         let mut dep_usage: BTreeMap<String, Vec<String>> = BTreeMap::new(); // dep_name -> vec of workspace members
//         let mut dep_dev_status: BTreeMap<String, bool> = BTreeMap::new(); // dep_name -> is any usage a dev dep

//         if workspace_packages.len() > 1 {
//             // Multi-crate workspace - collect from all members
//             for package in &workspace_packages {
//                 for dep in &package.dependencies {
//                     // Skip workspace-internal dependencies
//                     if dep.path.is_some() || self.workspace_packages.contains(&dep.name) {
//                         continue;
//                     }

//                     let is_dev_dep = matches!(dep.kind, DependencyKind::Development);
//                     dep_usage
//                         .entry(dep.name.clone())
//                         .or_default()
//                         .push(package.name.to_string());

//                     // Mark as dev_dep if ANY usage is dev (we could be more nuanced here)
//                     let current_dev_status =
//                         dep_dev_status.get(&dep.name).copied().unwrap_or(false);
//                     dep_dev_status.insert(dep.name.clone(), current_dev_status || is_dev_dep);
//                 }
//             }
//         } else {
//             // Single crate - use manifest dependencies
//             let single_crate_name = workspace_packages
//                 .first()
//                 .map(|p| p.name.to_string())
//                 .unwrap_or_default();
//             for (crate_names, dev_dep) in [
//                 (self.manifest.dependencies.keys(), false),
//                 (self.manifest.dev_dependencies.keys(), true),
//             ] {
//                 for crate_name in crate_names {
//                     dep_usage
//                         .entry(crate_name.clone())
//                         .or_default()
//                         .push(single_crate_name.clone());
//                     dep_dev_status.insert(crate_name.clone(), dev_dep);
//                 }
//             }
//         }

//         // Convert dependencies to CrateInfo with used_by tracking
//         for (dep_name, using_crates) in dep_usage {
//             let dev_dep = dep_dev_status.get(&dep_name).copied().unwrap_or(false);
//             let metadata = self
//                 .metadata
//                 .packages
//                 .iter()
//                 .find(|package| eq_ignoring_dash_underscore(&package.name, &dep_name));

//             crates.push(CrateInfo {
//                 crate_type: CrateProvenance::Library,
//                 version: metadata.map(|p| p.version.to_string()),
//                 description: metadata.and_then(|p| p.description.clone()),
//                 dev_dep,
//                 name: dep_name,
//                 default_crate: false,
//                 used_by: using_crates,
//             });
//         }

//         crates
//     }

//     /// Get available crate names and optional descriptions
//     pub(crate) fn available_crates(&self) -> impl Iterator<Item = CrateName<'_>> {
//         self.available_crates
//             .iter()
//             .filter_map(|x| CrateName::new(x))
//     }

//     pub fn project_root(&self) -> &Path {
//         self.manifest_path.parent().unwrap_or(&self.manifest_path)
//     }

//     pub(crate) fn default_crate_name(&self) -> Option<CrateName<'_>> {
//         if let Some(root) = self.metadata.root_package() {
//             CrateName::new(&root.name)
//         } else {
//             self.metadata
//                 .workspace_default_packages()
//                 .first()
//                 .and_then(|p| CrateName::new(p.name.as_str()))
//         }
//     }
//     /// Get crate info, optionally scoped to a specific workspace member
//     pub fn crate_info<'a>(
//         &'a self,
//         member_name: Option<&str>,
//     ) -> impl Iterator<Item = &'a CrateInfo> {
//         let filter_member = member_name.or_else(|| self.detect_subcrate_context());
//         let member_string = filter_member.map(|s| s.to_string());

//         self.crate_info.iter().filter(move |info| {
//             match &member_string {
//                 Some(member) => {
//                     // Include: workspace members + deps used by this member + standard library
//                     info.crate_type().is_workspace()
//                         || info.used_by().contains(member)
//                         || matches!(info.crate_type(), CrateProvenance::Rust)
//                 }
//                 None => true, // Include all for workspace view
//             }
//         })
//     }

//     /// Detect if we're in a subcrate context based on working directory
//     pub fn detect_subcrate_context(&self) -> Option<&str> {
//         let root_package = self.metadata.root_package()?;
//         let workspace_packages = self.metadata.workspace_packages();

//         // Check if we're in a subcrate context (working directory set to a specific workspace member)
//         if workspace_packages.len() > 1
//             && workspace_packages
//                 .iter()
//                 .any(|pkg| pkg.name == root_package.name)
//         {
//             Some(&root_package.name)
//         } else {
//             None
//         }
//     }

//     pub fn normalize_crate_name<'a>(&'a self, crate_name: &'a str) -> Option<CrateName<'a>> {
//         match crate_name {
//             "crate" => {
//                 // In workspace contexts (>1 package), don't allow "crate" alias
//                 if self.metadata.workspace_packages().len() > 1 {
//                     None
//                 } else {
//                     self.default_crate_name()
//                 }
//             }

//             // rustdoc placeholders
//             "alloc" | "alloc_crate" => Some(CrateName("alloc")),
//             "core" | "core_crate" => Some(CrateName("core")),
//             "proc_macro" | "proc_macro_crate" => Some(CrateName("proc_macro")),
//             "test" | "test_crate" => Some(CrateName("test")),
//             "std" | "std_crate" => Some(CrateName("std")),
//             "std_detect" | "rustc_literal_escaper" => None,

//             // future-proof: skip internal rustc crates
//             name if name.starts_with("rustc_") => None,
//             name => {
//                 // First try to find in available crates
//                 self.available_crates()
//                     .find(|correct_name| eq_ignoring_dash_underscore(correct_name, name))
//                     .or({
//                         // If not found in available crates, still return the name so
//                         // load_crate can attempt to fetch from docs.rs
//                         Some(CrateName(name))
//                     })
//             }
//         }
//     }
// }

// fn eq_ignoring_dash_underscore(a: &str, b: &str) -> bool {
//     let mut a = a.chars();
//     let mut b = b.chars();
//     loop {
//         match (a.next(), b.next()) {
//             (Some('_'), Some('-')) | (Some('-'), Some('_')) => {}
//             (Some(a), Some(b)) if a == b => {}
//             (None, None) => break true,
//             _ => break false,
//         }
//     }
// }
