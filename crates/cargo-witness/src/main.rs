use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cargo-witness", bin_name = "cargo")]
#[command(about = "Witness graph and repair routing for agent-native Rust workspaces")]
struct Cli {
    #[command(subcommand)]
    command: TopLevelCommand,
}

#[derive(Subcommand)]
enum TopLevelCommand {
    /// Compatibility wrapper for `cargo witness <subcommand>`.
    Witness {
        #[command(subcommand)]
        command: Command,
    },
    /// Build the witness graph (`.witness/witness-graph.json`).
    Build {
        /// Path to Cargo.toml (defaults to workspace root).
        #[arg(long)]
        manifest_path: Option<PathBuf>,
    },
    /// Diff two witness graphs and classify changes.
    Diff {
        /// Path to the prior witness graph JSON.
        prior: PathBuf,
        /// Path to the current witness graph JSON.
        new: PathBuf,
    },
    /// Route compile diagnostics to owning ARCs.
    Diagnose {
        /// Path to Cargo.toml (defaults to workspace root).
        #[arg(long)]
        manifest_path: Option<PathBuf>,
    },
    /// Assemble a minimal repair bundle from the latest failure.
    Repair {
        /// Path to Cargo.toml (defaults to workspace root).
        #[arg(long)]
        manifest_path: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Command {
    /// Build the witness graph (`.witness/witness-graph.json`).
    ///
    /// Parses all pub items from workspace crates via `syn` and computes
    /// dual hashes: interface (pub signatures) and implementation (everything else).
    Build {
        /// Path to Cargo.toml (defaults to workspace root).
        #[arg(long)]
        manifest_path: Option<PathBuf>,
    },

    /// Diff two witness graphs and classify changes.
    ///
    /// For each crate, outputs: interface-changed, implementation-only,
    /// added, removed, or unchanged.
    Diff {
        /// Path to the prior witness graph JSON.
        prior: PathBuf,
        /// Path to the current witness graph JSON.
        new: PathBuf,
    },

    /// Route compile diagnostics to owning ARCs.
    ///
    /// Runs `cargo check --message-format=json` and maps each error/warning
    /// to its owning ARC with enriched context (purpose, invariants, commands).
    Diagnose {
        /// Path to Cargo.toml (defaults to workspace root).
        #[arg(long)]
        manifest_path: Option<PathBuf>,
    },

    /// Assemble a minimal repair bundle from the latest failure.
    ///
    /// Reads `target/agent/compile-packets.json` or `target/agent/last-failure.json`
    /// and merges with the witness graph to produce the smallest context an agent needs.
    Repair {
        /// Path to Cargo.toml (defaults to workspace root).
        #[arg(long)]
        manifest_path: Option<PathBuf>,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("cargo-witness error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        TopLevelCommand::Witness { command } => dispatch(command),
        TopLevelCommand::Build { manifest_path } => dispatch(Command::Build { manifest_path }),
        TopLevelCommand::Diff { prior, new } => dispatch(Command::Diff { prior, new }),
        TopLevelCommand::Diagnose { manifest_path } => {
            dispatch(Command::Diagnose { manifest_path })
        }
        TopLevelCommand::Repair { manifest_path } => dispatch(Command::Repair { manifest_path }),
    }
}

fn dispatch(command: Command) -> Result<()> {
    match command {
        Command::Build { manifest_path } => {
            let workspace_root = resolve_workspace_root(manifest_path.as_deref())?;
            let graph = cargo_witness::graph::build_witness_graph(
                &workspace_root,
                manifest_path.as_deref(),
            )?;
            cargo_witness::graph::write_witness_graph(&workspace_root, &graph)?;
            let crate_count = graph.crates.len();
            let pub_items: usize = graph.crates.iter().map(|c| c.pub_items.len()).sum();
            println!(
                "witness-graph.json written: {crate_count} crates, {pub_items} pub items indexed"
            );
            Ok(())
        }

        Command::Diff { prior, new } => {
            let prior_content = std::fs::read_to_string(&prior)
                .with_context(|| format!("failed to read {}", prior.display()))?;
            let prior_graph: cargo_witness::model::WitnessGraph =
                serde_json::from_str(&prior_content)
                    .with_context(|| format!("failed to parse {}", prior.display()))?;

            let new_content = std::fs::read_to_string(&new)
                .with_context(|| format!("failed to read {}", new.display()))?;
            let new_graph: cargo_witness::model::WitnessGraph = serde_json::from_str(&new_content)
                .with_context(|| format!("failed to parse {}", new.display()))?;

            let diff = cargo_witness::diff::diff_witness_graphs(&prior_graph, &new_graph);
            println!("{}", serde_json::to_string_pretty(&diff)?);
            Ok(())
        }

        Command::Diagnose { manifest_path } => {
            let workspace_root = resolve_workspace_root(manifest_path.as_deref())?;
            let packets = cargo_witness::diagnose::diagnose_workspace(
                &workspace_root,
                manifest_path.as_deref(),
            )?;
            cargo_witness::diagnose::write_compile_packets(&workspace_root, &packets)?;
            let errors = packets.summary.total_errors;
            let warnings = packets.summary.total_warnings;
            let arcs = packets.summary.arcs_affected;
            if errors > 0 {
                println!(
                    "compile-packets.json written: {errors} errors, {warnings} warnings across {arcs} ARCs"
                );
            } else {
                println!("workspace compiles cleanly — no diagnostics to route");
            }
            Ok(())
        }

        Command::Repair { manifest_path } => {
            let workspace_root = resolve_workspace_root(manifest_path.as_deref())?;
            let bundle = cargo_witness::repair::build_repair_bundle(&workspace_root)?;
            cargo_witness::repair::write_repair_bundle(&workspace_root, &bundle)?;
            println!("{}", serde_json::to_string_pretty(&bundle)?);
            Ok(())
        }
    }
}

/// Resolve the workspace root from a manifest path or the current directory.
fn resolve_workspace_root(manifest_path: Option<&std::path::Path>) -> Result<PathBuf> {
    if let Some(path) = manifest_path {
        let path = path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {}", path.display()))?;
        Ok(path
            .parent()
            .context("manifest path has no parent")?
            .to_path_buf())
    } else {
        let metadata_query = cargo_metadata::MetadataCommand::new();
        #[rustfmt::skip]
        let metadata = metadata_query.exec().context("failed to read cargo metadata via the workspace allowlist")?;
        Ok(metadata.workspace_root.as_std_path().to_path_buf())
    }
}
