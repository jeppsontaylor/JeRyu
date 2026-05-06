use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::model::{
    AgentMap, AgentMember, SelectedArc, SelectedTest, TestEntry, TestMap, ValidationCommands,
    VerificationReport, VrcPlan,
};
use crate::workspace::WorkspaceSnapshot;

#[path = "planner_support.rs"]
mod planner_support;

pub use planner_support::context_metrics;
use self::planner_support::{
    api_surface_hash, boundary_trigger, collect_profile_commands, context_roots, display_relative,
    display_workspace_root, estimated_cost, generated_at, instruction_locations, matched_paths,
    normalize_changed_paths, owned_path_display, proof_density, public_surfaces,
    required_for_change_types, risk_tags, verify_workspace_fields,
};

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
    verify_workspace_fields(snapshot)
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

    fn current_manifest() -> PathBuf {
        workspace_root().join("Cargo.toml")
    }

    #[test]
    fn crate_local_change_stays_local() {
        let snapshot = load_workspace(Some(&current_manifest())).expect("load current workspace");
        let plan = build_vrc_plan(&snapshot, &[PathBuf::from("crates/cargo-vrc/src/model.rs")])
            .expect("build vrc plan");

        assert_eq!(plan.selected_arcs.len(), 1);
        assert_eq!(plan.selected_arcs[0].name, "cargo-vrc");
        assert_eq!(
            plan.stop_condition,
            "stop after local compile, tests, and doctests"
        );
        assert!(!plan.selected_tests.is_empty());
    }

    #[test]
    fn explain_subject_returns_package_match() {
        let snapshot = load_workspace(Some(&current_manifest())).expect("load current workspace");
        let explanation = explain_subject(&snapshot, "cargo-vrc").expect("explain subject");

        let matches = explanation["matches"].as_array().expect("matches array");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["arc"], "cargo-vrc");
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
    fn current_workspace_root_paths_select_jeryu_only() {
        let snapshot = load_workspace(Some(&current_manifest())).expect("load current workspace");

        let plan = build_vrc_plan(&snapshot, &[PathBuf::from("src/admission.rs")])
            .expect("root source plan");
        assert!(
            plan.selected_arcs.iter().any(|arc| arc.name == "jeryu"),
            "root src changes should select jeryu"
        );
        assert!(
            !plan.selected_arcs.iter().any(|arc| arc.name == "cargo-vrc"),
            "package-local src/** globs must not match root src changes"
        );
    }
}
