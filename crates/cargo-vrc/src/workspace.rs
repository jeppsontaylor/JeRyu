use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_metadata::{Metadata, MetadataCommand, Package};

use crate::model::{PackageAgentMetadata, WorkspaceAgentMetadata};

#[derive(Debug, Clone)]
pub struct PackageSnapshot {
    pub name: String,
    pub manifest_path: PathBuf,
    pub package_root: PathBuf,
    pub agent: PackageAgentMetadata,
    pub direct_dependencies: Vec<String>,
    pub reverse_dependencies: Vec<String>,
    pub target_names: Vec<String>,
    pub target_tests: Vec<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceSnapshot {
    pub metadata: Metadata,
    pub workspace_root: PathBuf,
    pub workspace_agent: WorkspaceAgentMetadata,
    pub packages: Vec<PackageSnapshot>,
}

pub fn load_workspace(manifest_path: Option<&Path>) -> Result<WorkspaceSnapshot> {
    let mut command = MetadataCommand::new();
    if let Some(path) = manifest_path {
        command.manifest_path(path);
    }
    let metadata = command.exec().context("failed to read cargo metadata")?;
    let workspace_root = normalize_existing_path(metadata.workspace_root.as_std_path());
    let workspace_agent = parse_workspace_agent(&metadata.workspace_metadata)?;
    let member_ids: HashSet<_> = metadata.workspace_members.iter().cloned().collect();
    let package_by_id: HashMap<_, _> = metadata
        .packages
        .iter()
        .map(|package| (package.id.clone(), package))
        .collect();

    let mut direct: HashMap<String, Vec<String>> = HashMap::new();
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(resolve) = &metadata.resolve {
        for node in &resolve.nodes {
            if !member_ids.contains(&node.id) {
                continue;
            }
            let Some(package) = package_by_id.get(&node.id) else {
                continue;
            };
            for dep in &node.deps {
                if !member_ids.contains(&dep.pkg) {
                    continue;
                }
                if let Some(dep_package) = package_by_id.get(&dep.pkg) {
                    direct
                        .entry(package.name.to_string())
                        .or_default()
                        .push(dep_package.name.to_string());
                    reverse
                        .entry(dep_package.name.to_string())
                        .or_default()
                        .push(package.name.to_string());
                }
            }
        }
    }

    let packages = metadata
        .packages
        .iter()
        .filter(|package| member_ids.contains(&package.id))
        .map(|package| package_snapshot(package, &workspace_root, &direct, &reverse))
        .collect::<Result<Vec<_>>>()?;

    Ok(WorkspaceSnapshot {
        metadata,
        workspace_root,
        workspace_agent,
        packages,
    })
}

fn parse_workspace_agent(value: &serde_json::Value) -> Result<WorkspaceAgentMetadata> {
    let agent = value
        .get("agent")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if agent.is_null() {
        return Ok(WorkspaceAgentMetadata::default());
    }
    serde_json::from_value(agent).context("failed to parse workspace.metadata.agent")
}

fn parse_package_agent(package: &Package) -> Result<PackageAgentMetadata> {
    let agent = package
        .metadata
        .get("agent")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if agent.is_null() {
        return Ok(PackageAgentMetadata::default());
    }
    serde_json::from_value(agent).with_context(|| {
        format!(
            "failed to parse package.metadata.agent for {}",
            package.name
        )
    })
}

fn package_snapshot(
    package: &Package,
    workspace_root: &Path,
    direct: &HashMap<String, Vec<String>>,
    reverse: &HashMap<String, Vec<String>>,
) -> Result<PackageSnapshot> {
    let manifest_path = normalize_existing_path(package.manifest_path.as_std_path());
    let package_root = manifest_path
        .parent()
        .context("package manifest unexpectedly missing parent directory")?
        .to_path_buf();
    let agent = parse_package_agent(package)?;
    let mut target_names = package
        .targets
        .iter()
        .map(|target| target.name.clone())
        .collect::<Vec<_>>();
    target_names.sort();
    let mut target_tests = package
        .targets
        .iter()
        .filter(|target| {
            target
                .kind
                .iter()
                .any(|kind| matches!(kind, cargo_metadata::TargetKind::Test))
        })
        .map(|target| {
            let normalized = normalize_existing_path(target.src_path.as_std_path());
            display_relative(workspace_root, &normalized)
        })
        .collect::<Vec<_>>();
    target_tests.sort();
    let mut features = package.features.keys().cloned().collect::<Vec<_>>();
    features.sort();
    Ok(PackageSnapshot {
        name: package.name.to_string(),
        manifest_path,
        package_root,
        agent,
        direct_dependencies: sorted_lookup(direct, &package.name),
        reverse_dependencies: sorted_lookup(reverse, &package.name),
        target_names,
        target_tests,
        features,
    })
}

fn sorted_lookup(map: &HashMap<String, Vec<String>>, key: &str) -> Vec<String> {
    let mut values = map.get(key).cloned().unwrap_or_default();
    values.sort();
    values.dedup();
    values
}

fn normalize_existing_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|| {
            if path == root {
                ".".to_string()
            } else {
                path.display().to_string()
            }
        })
}
