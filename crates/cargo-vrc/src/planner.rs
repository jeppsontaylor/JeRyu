use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::model::{
    AgentMap, AgentMember, SelectedArc, SelectedTest, TestEntry, TestMap, ValidationCommands,
    VerificationReport, VrcPlan,
};
use crate::workspace::{PackageSnapshot, WorkspaceSnapshot};

pub fn build_agent_map(snapshot: &WorkspaceSnapshot) -> AgentMap {
    let members = snapshot
        .packages
        .iter()
        .map(|package| AgentMember {
            name: package.name.clone(),
            manifest_path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            package_root: display_relative(&snapshot.workspace_root, &package.package_root),
            direct_dependencies: package.direct_dependencies.clone(),
            reverse_dependencies: package.reverse_dependencies.clone(),
            public_surfaces: public_surfaces(package),
            risk_tags: risk_tags(package),
            instruction_locations: instruction_locations(&snapshot.workspace_root, package),
            validation_commands: ValidationCommands {
                local: package.agent.local_validate.clone(),
                boundary: package.agent.boundary_validate.clone(),
            },
            api_surface_hash: api_surface_hash(package),
            proof_density: proof_density(package),
            context_roots: context_roots(&snapshot.workspace_root, package),
            exception_refs: package.agent.exceptions.clone(),
        })
        .collect();

    AgentMap {
        generated_at: generated_at(),
        workspace_root: display_workspace_root(),
        validation_order: snapshot.workspace_agent.validation_order.clone(),
        shared_contracts: snapshot.workspace_agent.shared_contracts.clone(),
        ci_profiles: snapshot.workspace_agent.ci_profiles.clone(),
        instruction_roots: snapshot.workspace_agent.instruction_roots.clone(),
        members,
    }
}

pub fn build_test_map(snapshot: &WorkspaceSnapshot) -> TestMap {
    let smoke_tests = collect_profile_commands(snapshot, "pull-request", "smoke");
    let e2e_gates = collect_profile_commands(snapshot, "scheduled-hardening", "e2e");
    let entries = snapshot
        .packages
        .iter()
        .map(|package| TestEntry {
            arc: package.name.clone(),
            source_roots: owned_path_display(package),
            unit_tests: package
                .agent
                .local_validate
                .iter()
                .filter(|command| !command.contains("--doc"))
                .cloned()
                .collect(),
            doctests: package
                .agent
                .local_validate
                .iter()
                .filter(|command| command.contains("--doc"))
                .cloned()
                .collect(),
            integration_harnesses: package.target_tests.clone(),
            reverse_dependency_tests: package.agent.boundary_validate.clone(),
            smoke_tests: smoke_tests.clone(),
            e2e_gates: e2e_gates.clone(),
            selection_reason: if package.agent.public_api {
                "public surface changes require reverse dependency and contract awareness"
                    .to_string()
            } else {
                "leaf ARC changes usually stop at local proof unless a manifest or boundary moves"
                    .to_string()
            },
            estimated_cost: estimated_cost(package),
            required_for_change_types: required_for_change_types(package),
        })
        .collect();

    TestMap {
        generated_at: generated_at(),
        workspace_root: display_workspace_root(),
        entries,
    }
}

pub fn build_vrc_plan(snapshot: &WorkspaceSnapshot, changed_paths: &[PathBuf]) -> Result<VrcPlan> {
    let normalized_paths = normalize_changed_paths(&snapshot.workspace_root, changed_paths);
    let mut selected_arcs = Vec::new();
    let mut selected_tests = Vec::new();
    let mut rationale = Vec::new();
    let mut boundary_required = false;

    for package in &snapshot.packages {
        let hits = matched_paths(snapshot, package, &normalized_paths);
        if hits.is_empty() {
            continue;
        }
        let requires_boundary = hits.iter().any(|path| boundary_trigger(path, package));
        boundary_required |= requires_boundary;
        let reason = if requires_boundary {
            format!(
                "Matched {} and crossed a public or manifest boundary",
                hits.join(", ")
            )
        } else {
            format!("Matched owned paths {}", hits.join(", "))
        };
        rationale.push(format!("Selected {} because {}", package.name, reason));
        selected_arcs.push(SelectedArc {
            name: package.name.clone(),
            reason: reason.clone(),
            local_validate: package.agent.local_validate.clone(),
            boundary_validate: if requires_boundary {
                package.agent.boundary_validate.clone()
            } else {
                Vec::new()
            },
            public_api: package.agent.public_api,
        });

        for command in &package.agent.local_validate {
            selected_tests.push(SelectedTest {
                arc: package.name.clone(),
                command: command.clone(),
                ring: if command.contains("--doc") {
                    "doctest".to_string()
                } else {
                    "local".to_string()
                },
                selection_reason: "local proof is always required for matched ARCs".to_string(),
            });
        }
        if requires_boundary {
            for command in &package.agent.boundary_validate {
                selected_tests.push(SelectedTest {
                    arc: package.name.clone(),
                    command: command.clone(),
                    ring: "boundary".to_string(),
                    selection_reason:
                        "public API or manifest changes widened the validation radius".to_string(),
                });
            }
        }
    }

    let stop_condition = if selected_arcs.is_empty() {
        rationale.push("No ARC matched the changed paths; this is likely a documentation or root-policy change.".to_string());
        "no-arc-match".to_string()
    } else if boundary_required {
        "stop after mapped reverse dependency and contract rings".to_string()
    } else {
        "stop after local compile, tests, and doctests".to_string()
    };

    let skipped_rings = if boundary_required {
        vec!["full-e2e".to_string()]
    } else {
        vec![
            "reverse-dependency".to_string(),
            "contract".to_string(),
            "smoke".to_string(),
            "full-e2e".to_string(),
        ]
    };

    Ok(VrcPlan {
        generated_at: generated_at(),
        changed_paths: normalized_paths,
        selected_arcs,
        selected_tests,
        stop_condition,
        skipped_rings,
        rationale,
    })
}

pub fn explain_subject(snapshot: &WorkspaceSnapshot, subject: &str) -> Result<serde_json::Value> {
    let subject_path = Path::new(subject);
    if subject_path.exists() || subject.contains('/') || subject.contains('\\') {
        let plan = build_vrc_plan(snapshot, &[subject_path.to_path_buf()])?;
        return serde_json::to_value(plan).context("failed to serialize explanation plan");
    }

    let matches = snapshot
        .packages
        .iter()
        .filter(|package| {
            package.name == subject
                || package
                    .agent
                    .entrypoints
                    .iter()
                    .any(|entry| entry == subject)
        })
        .map(|package| {
            serde_json::json!({
                "arc": package.name,
                "purpose": package.agent.purpose,
                "entrypoints": package.agent.entrypoints,
                "invariants": package.agent.invariants,
                "local_validate": package.agent.local_validate,
                "boundary_validate": package.agent.boundary_validate,
                "reverse_dependencies": package.reverse_dependencies,
            })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "subject": subject,
        "matches": matches,
    }))
}

pub fn verify_workspace(snapshot: &WorkspaceSnapshot) -> VerificationReport {
    let mut report = VerificationReport::default();

    if snapshot.workspace_agent.validation_order.is_empty() {
        report
            .warnings
            .push("workspace.metadata.agent.validation_order is empty".to_string());
    }
    if snapshot.workspace_agent.instruction_roots.is_empty() {
        report
            .warnings
            .push("workspace.metadata.agent.instruction_roots is empty".to_string());
    }

    for package in &snapshot.packages {
        if package.agent.purpose.trim().is_empty() {
            report.errors.push(format!(
                "{} is missing package.metadata.agent.purpose",
                package.name
            ));
        }
        if package.agent.invariants.is_empty() {
            report
                .warnings
                .push(format!("{} is missing explicit invariants", package.name));
        }
        if package.agent.local_validate.is_empty() {
            report.errors.push(format!(
                "{} is missing package.metadata.agent.local_validate",
                package.name
            ));
        }
        if package.agent.owned_paths.is_empty() {
            report.warnings.push(format!(
                "{} is missing package.metadata.agent.owned_paths; path matching will fall back to package roots",
                package.name
            ));
        }
        if package.agent.public_api && package.agent.boundary_validate.is_empty() {
            report.errors.push(format!(
                "{} is marked public_api=true but has no boundary_validate commands",
                package.name
            ));
        }
        let local_agents = package.package_root.join("AGENTS.md");
        if !local_agents.exists() {
            report.warnings.push(format!(
                "{} has no local AGENTS.md; consider adding crate-specific guidance",
                package.name
            ));
        }
    }

    report
}

fn normalize_changed_paths(workspace_root: &Path, changed_paths: &[PathBuf]) -> Vec<String> {
    let mut normalized = changed_paths
        .iter()
        .map(|path| {
            let absolute = if path.is_absolute() {
                path.clone()
            } else {
                workspace_root.join(path)
            };
            display_relative(workspace_root, &absolute)
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn matched_paths(
    snapshot: &WorkspaceSnapshot,
    package: &PackageSnapshot,
    changed_paths: &[String],
) -> Vec<String> {
    let matcher = build_globset(&package.agent.owned_paths).ok();
    let package_root = display_relative(&snapshot.workspace_root, &package.package_root);
    let mut hits = BTreeSet::new();
    for changed in changed_paths {
        if changed == &package_root || changed.starts_with(&(package_root.clone() + "/")) {
            hits.insert(changed.clone());
            continue;
        }
        if let Some(matcher) = &matcher {
            let changed_path = Path::new(changed);
            if package_root == "." && matcher.is_match(changed_path) {
                hits.insert(changed.clone());
                continue;
            }
            if let Ok(stripped) = changed_path.strip_prefix(&package_root)
                && matcher.is_match(stripped)
            {
                hits.insert(changed.clone());
            }
        }
    }
    hits.into_iter().collect()
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern)?);
    }
    builder
        .build()
        .context("failed to compile owned_paths globset")
}

fn boundary_trigger(path: &str, package: &PackageSnapshot) -> bool {
    let path = Path::new(path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    file_name == "Cargo.toml"
        || (package.agent.public_api && (file_name == "lib.rs" || file_name == "mod.rs"))
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}

fn public_surfaces(package: &PackageSnapshot) -> Vec<String> {
    if !package.agent.entrypoints.is_empty() {
        return package.agent.entrypoints.clone();
    }
    package.target_names.clone()
}

fn risk_tags(package: &PackageSnapshot) -> Vec<String> {
    let mut tags = vec![if package.agent.risk.is_empty() {
        "risk:unspecified".to_string()
    } else {
        format!("risk:{}", package.agent.risk)
    }];
    if package.agent.public_api {
        tags.push("public-api".to_string());
    }
    if !package.agent.exceptions.is_empty() {
        tags.push("has-aer".to_string());
    }
    tags
}

fn instruction_locations(workspace_root: &Path, package: &PackageSnapshot) -> Vec<String> {
    let mut locations = Vec::new();
    for path in [
        workspace_root.join("AGENTS.md"),
        workspace_root.join("CLAUDE.md"),
        workspace_root.join(".github/copilot-instructions.md"),
        package.package_root.join("AGENTS.md"),
    ] {
        if path.exists() {
            locations.push(display_relative(workspace_root, &path));
        }
    }
    locations
}

fn context_roots(workspace_root: &Path, package: &PackageSnapshot) -> Vec<String> {
    let mut roots = vec![display_relative(workspace_root, &package.package_root)];
    for suffix in ["src", "tests", "examples"] {
        let candidate = package.package_root.join(suffix);
        if candidate.exists() {
            roots.push(display_relative(workspace_root, &candidate));
        }
    }
    for location in instruction_locations(workspace_root, package) {
        roots.push(location);
    }
    roots.sort();
    roots.dedup();
    roots
}

fn owned_path_display(package: &PackageSnapshot) -> Vec<String> {
    if package.agent.owned_paths.is_empty() {
        return vec!["<package-root>".to_string()];
    }
    package.agent.owned_paths.clone()
}

fn proof_density(package: &PackageSnapshot) -> f64 {
    let proof_points = package.agent.invariants.len()
        + package.agent.local_validate.len()
        + package.agent.boundary_validate.len()
        + package.agent.exceptions.len();
    let entrypoints = package.agent.entrypoints.len().max(1);
    let density = proof_points as f64 / entrypoints as f64;
    (density * 100.0).round() / 100.0
}

fn api_surface_hash(package: &PackageSnapshot) -> String {
    let mut hasher = Sha256::new();
    hasher.update(package.name.as_bytes());
    hasher.update(if package.agent.public_api {
        &b"public"[..]
    } else {
        &b"private"[..]
    });
    for item in public_surfaces(package) {
        hasher.update(item.as_bytes());
    }
    for feature in &package.features {
        hasher.update(feature.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn collect_profile_commands(
    snapshot: &WorkspaceSnapshot,
    profile_name: &str,
    needle: &str,
) -> Vec<String> {
    snapshot
        .workspace_agent
        .ci_profiles
        .iter()
        .find(|profile| profile.name == profile_name)
        .map(|profile| {
            profile
                .commands
                .iter()
                .filter(|command| command.contains(needle))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn estimated_cost(package: &PackageSnapshot) -> String {
    let local = package.agent.local_validate.len();
    let boundary = package.agent.boundary_validate.len();
    let harnesses = package.target_tests.len();
    match local + boundary + harnesses {
        0..=2 => "low".to_string(),
        3..=5 => "medium".to_string(),
        _ => "high".to_string(),
    }
}

fn required_for_change_types(package: &PackageSnapshot) -> Vec<String> {
    let mut change_types = vec!["leaf-bugfix".to_string(), "invariant-change".to_string()];
    if package.agent.public_api {
        change_types.push("public-api-change".to_string());
    }
    if !package.features.is_empty() {
        change_types.push("feature-change".to_string());
    }
    change_types.push("manifest-change".to_string());
    change_types
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

fn display_workspace_root() -> String {
    ".".to_string()
}

fn generated_at() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

pub fn context_metrics(workspace_root: &Path, package_root: &Path) -> Result<(usize, u64)> {
    let mut file_count = 0usize;
    let mut bytes = 0u64;
    for entry in WalkDir::new(package_root)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();
        if !matches!(extension, "rs" | "toml" | "md" | "json" | "yaml" | "yml") {
            continue;
        }
        file_count += 1;
        bytes += fs::metadata(path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?
            .len();
    }
    let root_display = display_relative(workspace_root, package_root);
    if file_count == 0 {
        return Ok((0, 0));
    }
    let _ = root_display;
    Ok((file_count, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_workspace;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root")
            .to_path_buf()
    }

    fn arcified_manifest() -> PathBuf {
        workspace_root().join("labs/repo-shape-bench/arcified/Cargo.toml")
    }

    fn current_manifest() -> PathBuf {
        workspace_root().join("Cargo.toml")
    }

    #[test]
    fn leaf_bugfix_plan_stays_local() {
        let snapshot = load_workspace(Some(&arcified_manifest())).expect("load fixture workspace");
        let plan = build_vrc_plan(&snapshot, &[PathBuf::from("crates/orders-core/src/lib.rs")])
            .expect("build vrc plan");

        assert_eq!(plan.selected_arcs.len(), 1);
        assert_eq!(plan.selected_arcs[0].name, "orders-core");
        assert_eq!(plan.selected_tests.len(), 3);
        assert_eq!(
            plan.stop_condition,
            "stop after local compile, tests, and doctests"
        );
        assert!(
            plan.skipped_rings
                .contains(&"reverse-dependency".to_string())
        );
    }

    #[test]
    fn explain_subject_returns_entrypoint_match() {
        let snapshot = load_workspace(Some(&arcified_manifest())).expect("load fixture workspace");
        let explanation = explain_subject(&snapshot, "render_quote").expect("explain subject");

        let matches = explanation["matches"].as_array().expect("matches array");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["arc"], "http-api");
    }

    #[test]
    fn current_workspace_maps_use_stable_relative_paths() {
        let snapshot = load_workspace(Some(&current_manifest())).expect("load current workspace");
        let agent_map = build_agent_map(&snapshot);
        let test_map = build_test_map(&snapshot);

        assert_eq!(agent_map.workspace_root, ".");
        assert_eq!(test_map.workspace_root, ".");
        assert!(
            agent_map
                .members
                .iter()
                .all(|member| !member.manifest_path.starts_with('/'))
        );
        assert!(
            agent_map
                .members
                .iter()
                .all(|member| !member.package_root.starts_with('/'))
        );

        let harnesses = test_map
            .entries
            .iter()
            .flat_map(|entry| entry.integration_harnesses.iter())
            .collect::<Vec<_>>();
        assert!(!harnesses.is_empty());
        assert!(harnesses.iter().all(|path| !path.starts_with('/')));
        assert!(harnesses.iter().all(|path| !path.contains("/../")));
    }

    #[test]
    fn current_workspace_root_paths_select_vgit_only() {
        let snapshot = load_workspace(Some(&current_manifest())).expect("load current workspace");

        let plan = build_vrc_plan(&snapshot, &[PathBuf::from("src/admission.rs")])
            .expect("root source plan");
        assert!(
            plan.selected_arcs.iter().any(|arc| arc.name == "vgit"),
            "root src changes should select vgit"
        );
        assert!(
            !plan.selected_arcs.iter().any(|arc| arc.name == "cargo-vrc"),
            "package-local src/** globs must not match root src changes"
        );
    }
}
