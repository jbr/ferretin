#![allow(dead_code)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod color_scheme;
mod format;
mod format_context;
mod indent;
mod markdown;
mod renderer;
mod request;
mod styled_string;
mod traits;
mod verbosity;

/// A human-friendly CLI for browsing Rust documentation
#[derive(Parser, Debug)]
#[command(name = "ferretin")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to Cargo.toml (defaults to current directory)
    #[arg(short, long, global = true)]
    manifest_path: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show documentation for an item
    Get {
        /// Path to the item (e.g., "std::vec::Vec" or "serde::Serialize")
        path: String,

        /// Show source code
        #[arg(short, long)]
        source: bool,

        /// Recursively show nested items
        #[arg(short, long)]
        recursive: bool,
    },

    /// Search for items by name or documentation
    Search {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// List available crates
    List,
}

use request::Request;
use rustdoc_core::{RustdocProject, search::indexer::SearchIndex};
use std::rc::Rc;

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    // Default to current directory - RustdocProject::load will walk up to find workspace root
    let path = cli
        .manifest_path
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Load the project (accepts directory or Cargo.toml path)
    let project = Rc::new(RustdocProject::load(path)?);
    let request = Request::new(Rc::clone(&project));

    match cli.command {
        Commands::Get {
            path,
            source,
            recursive,
        } => {
            let context = format_context::FormatContext::new(source, recursive);
            let mut suggestions = vec![];

            match request.resolve_path(&path, &mut suggestions) {
                Some(item) => {
                    let doc_nodes = request.format_item(item, &context);
                    let output = context.render(doc_nodes);
                    println!("{}", output);
                    Ok(())
                }
                None => {
                    eprintln!("Could not find '{}'", path);
                    if !suggestions.is_empty() {
                        eprintln!("\nDid you mean:");
                        for suggestion in suggestions.iter().take(5) {
                            eprintln!("  {}", suggestion.path());
                        }
                    }
                    std::process::exit(1);
                }
            }
        }
        Commands::Search { query, limit } => {
            // Collect search results from all crates
            let mut all_results = vec![];

            for crate_info in project.crate_info(None) {
                let crate_name = crate_info.name();

                // Try to load/build the search index for this crate
                match SearchIndex::load_or_build(&request, crate_name) {
                    Ok(index) => {
                        // Search and collect results with crate name
                        let results = index.search(&query);
                        for (id_path, score) in results {
                            all_results.push((crate_name.to_string(), id_path.to_vec(), score));
                        }
                    }
                    Err(_) => {
                        // Silently skip crates that can't be indexed (e.g., not found)
                        continue;
                    }
                }
            }

            // Sort all results by score (descending)
            all_results.sort_by(|(_, _, score_a), (_, _, score_b)| {
                score_b.partial_cmp(score_a).unwrap_or(std::cmp::Ordering::Equal)
            });

            if all_results.is_empty() {
                eprintln!("No results found for '{}'", query);
                std::process::exit(1);
            }

            // Calculate total score for normalization
            let total_score: f32 = all_results.iter().map(|(_, _, score)| score).sum();
            let top_score = all_results.first().map(|(_, _, score)| *score).unwrap_or(0.0);

            println!("Search results for '{}':\n", query);

            // Display results with early stopping based on score thresholds
            let min_results = 1;
            let mut cumulative_score = 0.0;
            let mut prev_score = top_score;

            for (i, (crate_name, id_path, score)) in all_results.into_iter().enumerate() {
                // Early stopping: stop if we've shown enough results and scores are dropping significantly
                if i >= min_results && i >= limit {
                    break;
                }

                if i >= min_results
                    && (score / top_score < 0.05
                        || score / prev_score < 0.5
                        || cumulative_score / total_score > 0.3)
                {
                    break;
                }

                if let Some((item, path_segments)) = request.get_item_from_id_path(&crate_name, &id_path) {
                    cumulative_score += score;
                    prev_score = score;

                    let path = path_segments.join("::");
                    let normalized_score = 100.0 * score / total_score;

                    println!(
                        "• {} ({:?}) - score: {:.0}",
                        path,
                        item.kind(),
                        normalized_score
                    );

                    // Show first few lines of docs if available
                    if let Some(docs) = &item.docs {
                        let doc_preview: Vec<_> = docs.lines().take(2).collect();
                        if !doc_preview.is_empty() {
                            for line in doc_preview {
                                if !line.trim().is_empty() {
                                    println!("    {}", line);
                                }
                            }
                        }
                    }
                    println!();
                }
            }

            Ok(())
        }
        Commands::List => {
            println!("Available crates:\n");

            for crate_info in project.crate_info(None) {
                let crate_name = crate_info.name();

                let note = if crate_info.is_default_crate() {
                    " (workspace-local, aliased as \"crate\")".to_string()
                } else if crate_info.crate_type().is_workspace() {
                    " (workspace-local)".to_string()
                } else if let Some(version) = crate_info.version() {
                    let dev_dep_note = if crate_info.is_dev_dep() {
                        " (dev-dep)"
                    } else {
                        ""
                    };

                    // Show which workspace members use this dependency
                    let usage_info = if !crate_info.used_by().is_empty() {
                        let members: Vec<String> = crate_info
                            .used_by()
                            .iter()
                            .map(|member| {
                                if crate_info.is_dev_dep() {
                                    format!("{} dev", member)
                                } else {
                                    member.clone()
                                }
                            })
                            .collect();
                        format!(" ({})", members.join(", "))
                    } else {
                        String::new()
                    };

                    format!(" {}{}{}", version, dev_dep_note, usage_info)
                } else {
                    String::new()
                };

                println!("• {}{}", crate_name, note);

                if let Some(description) = crate_info.description() {
                    let description = description.replace('\n', " ");
                    println!("    {}", description);
                }
            }

            Ok(())
        }
    }
}
