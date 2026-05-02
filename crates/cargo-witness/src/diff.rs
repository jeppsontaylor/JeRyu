use std::collections::HashMap;

use chrono::Utc;

use crate::model::{ChangeClassification, CrateChange, WitnessDiff, WitnessGraph};

/// Diff two witness graphs and classify changes per crate.
///
/// For each crate present in either graph, classify the change as:
/// - `InterfaceChanged` — pub signatures changed, must escalate
/// - `ImplementationOnly` — only internals changed, stay local
/// - `Unchanged` — no change
/// - `Added` — new crate in `new` not in `old`
/// - `Removed` — crate in `old` missing from `new`
pub fn diff_witness_graphs(old: &WitnessGraph, new: &WitnessGraph) -> WitnessDiff {
    let old_map: HashMap<&str, &crate::model::CrateWitness> =
        old.crates.iter().map(|c| (c.name.as_str(), c)).collect();
    let new_map: HashMap<&str, &crate::model::CrateWitness> =
        new.crates.iter().map(|c| (c.name.as_str(), c)).collect();

    let mut all_names: Vec<&str> = old_map
        .keys()
        .chain(new_map.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    all_names.sort();

    let mut changes = Vec::new();

    for name in all_names {
        match (old_map.get(name), new_map.get(name)) {
            (None, Some(_new_crate)) => {
                changes.push(CrateChange {
                    name: name.to_string(),
                    classification: ChangeClassification::Added,
                    interface_changed: true,
                    implementation_changed: true,
                    local_commands: vec![
                        format!("cargo check -p {name}"),
                        format!("cargo test -p {name}"),
                    ],
                    escalation_commands: vec![format!(
                        "cargo nextest run -E 'rdeps({name})' --ignore-default-filter"
                    )],
                    reason: "new crate added to workspace".to_string(),
                });
            }
            (Some(_), None) => {
                changes.push(CrateChange {
                    name: name.to_string(),
                    classification: ChangeClassification::Removed,
                    interface_changed: true,
                    implementation_changed: true,
                    local_commands: vec![],
                    escalation_commands: vec!["cargo check --workspace".to_string()],
                    reason: "crate removed from workspace — validate all former consumers"
                        .to_string(),
                });
            }
            (Some(old_crate), Some(new_crate)) => {
                let interface_changed = old_crate.interface_hash != new_crate.interface_hash;
                let implementation_changed =
                    old_crate.implementation_hash != new_crate.implementation_hash;

                if !interface_changed && !implementation_changed {
                    continue; // Skip unchanged crates entirely.
                }

                let classification = if interface_changed {
                    ChangeClassification::InterfaceChanged
                } else {
                    ChangeClassification::ImplementationOnly
                };

                let local_commands = vec![
                    format!("cargo check -p {name}"),
                    format!("cargo test -p {name}"),
                ];

                let escalation_commands = if interface_changed {
                    vec![format!(
                        "cargo nextest run -E 'rdeps({name})' --ignore-default-filter"
                    )]
                } else {
                    vec![]
                };

                let reason = if interface_changed {
                    format!(
                        "interface hash changed ({} → {}); pub API shift requires rdep validation",
                        &old_crate.interface_hash[..12],
                        &new_crate.interface_hash[..12]
                    )
                } else {
                    format!(
                        "implementation hash changed ({} → {}); interface stable — local-only validation",
                        &old_crate.implementation_hash[..12],
                        &new_crate.implementation_hash[..12]
                    )
                };

                changes.push(CrateChange {
                    name: name.to_string(),
                    classification,
                    interface_changed,
                    implementation_changed,
                    local_commands,
                    escalation_commands,
                    reason,
                });
            }
            (None, None) => unreachable!(),
        }
    }

    let total_crates_changed = changes.len();
    let escalation_required = changes
        .iter()
        .any(|change| change.classification == ChangeClassification::InterfaceChanged);
    let estimated_test_commands: usize = changes
        .iter()
        .map(|change| change.local_commands.len() + change.escalation_commands.len())
        .sum();

    WitnessDiff {
        generated_at: Utc::now().format("%Y-%m-%d").to_string(),
        changes,
        total_crates_changed,
        escalation_required,
        estimated_test_commands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CrateWitness, WitnessGraph};

    fn make_crate(name: &str, iface: &str, imp: &str) -> CrateWitness {
        CrateWitness {
            name: name.to_string(),
            interface_hash: iface.to_string(),
            implementation_hash: imp.to_string(),
            pub_items: vec![],
            direct_deps: vec![],
            reverse_deps: vec![],
            file_count: 1,
            total_lines: 100,
        }
    }

    fn make_graph(crates: Vec<CrateWitness>) -> WitnessGraph {
        WitnessGraph {
            generated_at: "2026-03-31".into(),
            workspace_root: ".".into(),
            crates,
        }
    }

    #[test]
    fn unchanged_crates_are_skipped() {
        let old = make_graph(vec![make_crate("a", "hash1", "hash2")]);
        let new = make_graph(vec![make_crate("a", "hash1", "hash2")]);
        let diff = diff_witness_graphs(&old, &new);
        assert_eq!(diff.total_crates_changed, 0);
        assert!(!diff.escalation_required);
    }

    #[test]
    fn implementation_only_change_stays_local() {
        let old = make_graph(vec![make_crate("a", "same-iface", "impl-old-xxxxx")]);
        let new = make_graph(vec![make_crate("a", "same-iface", "impl-new-xxxxx")]);
        let diff = diff_witness_graphs(&old, &new);
        assert_eq!(diff.total_crates_changed, 1);
        assert!(!diff.escalation_required);
        assert_eq!(
            diff.changes[0].classification,
            ChangeClassification::ImplementationOnly
        );
        assert!(diff.changes[0].escalation_commands.is_empty());
    }

    #[test]
    fn interface_change_triggers_escalation() {
        let old = make_graph(vec![make_crate("a", "old-iface-xxx", "impl-hash-xxx")]);
        let new = make_graph(vec![make_crate("a", "new-iface-xxx", "impl-hash-xxx")]);
        let diff = diff_witness_graphs(&old, &new);
        assert_eq!(diff.total_crates_changed, 1);
        assert!(diff.escalation_required);
        assert_eq!(
            diff.changes[0].classification,
            ChangeClassification::InterfaceChanged
        );
        assert!(!diff.changes[0].escalation_commands.is_empty());
    }

    #[test]
    fn added_crate_detected() {
        let old = make_graph(vec![]);
        let new = make_graph(vec![make_crate("b", "hash1", "hash2")]);
        let diff = diff_witness_graphs(&old, &new);
        assert_eq!(diff.total_crates_changed, 1);
        assert_eq!(diff.changes[0].classification, ChangeClassification::Added);
    }

    #[test]
    fn removed_crate_detected() {
        let old = make_graph(vec![make_crate("c", "hash1", "hash2")]);
        let new = make_graph(vec![]);
        let diff = diff_witness_graphs(&old, &new);
        assert_eq!(diff.total_crates_changed, 1);
        assert_eq!(
            diff.changes[0].classification,
            ChangeClassification::Removed
        );
    }
}
