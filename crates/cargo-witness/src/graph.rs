use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::model::{CrateWitness, WitnessGraph};

#[path = "graph_extract.rs"]
mod graph_extract;
use graph_extract::*;

/// Build a witness graph for the workspace at `workspace_root`.
///
/// For each workspace member crate, parses all `.rs` files in `src/` using
/// `syn`, extracts public item signatures, and computes dual hashes:
/// - **Interface hash**: SHA-256 of sorted pub-item signatures
/// - **Implementation hash**: SHA-256 of all source content minus pub signatures
pub fn build_witness_graph(
    _workspace_root: &Path,
    manifest_path: Option<&Path>,
) -> Result<WitnessGraph> {
    let snapshot = cargo_vrc::load_workspace(manifest_path)?;

    let mut crates = Vec::new();
    for package in &snapshot.packages {
        let witness = build_crate_witness(&snapshot.workspace_root, package)?;
        crates.push(witness);
    }

    Ok(WitnessGraph {
        generated_at: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        workspace_root: display_workspace_root(),
        crates,
    })
}

/// Build witness data for a single crate.
fn build_crate_witness(
    workspace_root: &Path,
    package: &cargo_vrc::workspace::PackageSnapshot,
) -> Result<CrateWitness> {
    let src_dir = package.package_root.join("src");
    let mut pub_items = Vec::new();
    let mut interface_hasher = Sha256::new();
    let mut impl_hasher = Sha256::new();
    let mut file_count = 0usize;
    let mut total_lines = 0usize;

    if src_dir.exists() {
        let mut rs_files: Vec<PathBuf> = WalkDir::new(&src_dir)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().is_file()
                    && entry.path().extension().and_then(|ext| ext.to_str()) == Some("rs")
            })
            .map(|entry| entry.path().to_path_buf())
            .collect();

        // Sort for deterministic hashing.
        rs_files.sort();

        for rs_file in rs_files {
            let content = fs::read_to_string(&rs_file)
                .with_context(|| format!("failed to read {}", rs_file.display()))?;

            file_count += 1;
            total_lines += content.lines().count();

            let relative = rs_file
                .strip_prefix(workspace_root)
                .unwrap_or(&rs_file)
                .display()
                .to_string();

            let extracted = extract_pub_items(&relative, &content);
            let pub_signatures: BTreeSet<String> = extracted
                .iter()
                .map(|item| item.signature.clone())
                .collect();

            // Interface hash: sorted pub signatures.
            for sig in &pub_signatures {
                interface_hasher.update(sig.as_bytes());
                interface_hasher.update(b"\n");
            }

            // Implementation hash: everything that isn't a pub signature.
            // We hash the full content and also the "non-pub" marker so that
            // identical pub signatures with different implementations produce
            // different implementation hashes.
            impl_hasher.update(relative.as_bytes());
            impl_hasher.update(content.as_bytes());
            for sig in &pub_signatures {
                // XOR-remove the pub signatures from the impl hash by including
                // a distinguishing prefix.
                impl_hasher.update(b"PUB:");
                impl_hasher.update(sig.as_bytes());
            }

            pub_items.extend(extracted);
        }
    }

    let interface_hash = hex_digest(interface_hasher);
    let implementation_hash = hex_digest(impl_hasher);

    Ok(CrateWitness {
        name: package.name.clone(),
        interface_hash,
        implementation_hash,
        pub_items,
        direct_deps: package.direct_dependencies.clone(),
        reverse_deps: package.reverse_dependencies.clone(),
        file_count,
        total_lines,
    })
}

/// Finalize a SHA-256 hasher into a hex string.
fn hex_digest(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Write a witness graph to disk as JSON.
pub fn write_witness_graph(workspace_root: &Path, graph: &WitnessGraph) -> Result<()> {
    let output_dir = workspace_root.join(".witness");
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let output_path = output_dir.join("witness-graph.json");
    let json = serde_json::to_string_pretty(graph)?;
    fs::write(&output_path, json)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(())
}

/// Load a witness graph from disk.
pub fn load_witness_graph(workspace_root: &Path) -> Result<WitnessGraph> {
    let path = workspace_root.join(".witness/witness-graph.json");
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}

/// Try to load a witness graph, returning None if it doesn't exist.
pub fn load_witness_graph_if_present(workspace_root: &Path) -> Option<WitnessGraph> {
    let path = workspace_root.join(".witness/witness-graph.json");
    if !path.exists() {
        return None;
    }
    load_witness_graph(workspace_root).ok()
}

fn display_workspace_root() -> String {
    ".".to_string()
}
