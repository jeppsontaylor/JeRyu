//! Owner: Interactive TUI subsystem - optional Jankurai snapshot loading
//! Proof: `cargo nextest run -p jeryu -- tui::jankurai`
//! Invariants: loading is read-only, non-fatal, and only surfaces installed-tool data.

mod model;
mod parse;
mod root;

use std::path::Path;

pub use model::{
    JankuraiDimension, JankuraiEntry, JankuraiEntryKind, JankuraiHistoryPoint, JankuraiScan,
    JankuraiSnapshot,
};
use parse::{parse_history, parse_report};
use root::{fs_read_optional, is_jankurai_installed, repo_root_from_runtime};

pub fn load_snapshot() -> JankuraiSnapshot {
    if !is_jankurai_installed() {
        return JankuraiSnapshot::default();
    }

    match repo_root_from_runtime() {
        Ok(root) => load_snapshot_from(&root),
        Err(error) => JankuraiSnapshot {
            installed: true,
            error: Some(error),
            ..Default::default()
        },
    }
}

pub(crate) fn load_snapshot_from(repo_root: &Path) -> JankuraiSnapshot {
    let mut snapshot = JankuraiSnapshot {
        installed: true,
        ..Default::default()
    };
    let mut errors = Vec::new();

    let report_path = repo_root.join("agent/repo-score.json");
    let history_path = repo_root.join("agent/score-history.jsonl");

    match fs_read_optional(&report_path) {
        Ok(Some(raw)) => match parse_report(&raw) {
            Ok(parsed) => {
                snapshot.dimensions = parsed.dimensions;
                snapshot.entries = parsed.entries;
                snapshot.last_scan = Some(parsed.scan);
            }
            Err(err) => errors.push(format!("{}: {}", report_path.display(), err)),
        },
        Ok(None) => {}
        Err(err) => errors.push(format!("{}: {}", report_path.display(), err)),
    }

    match fs_read_optional(&history_path) {
        Ok(Some(raw)) => {
            let (history, mut history_errors) = parse_history(&raw);
            snapshot.history = history;
            errors.append(&mut history_errors);
        }
        Ok(None) => {}
        Err(err) => errors.push(format!("{}: {}", history_path.display(), err)),
    }

    if !errors.is_empty() {
        snapshot.error = Some(errors.join("; "));
    }

    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn loads_snapshot_from_repo_root_without_panicking_on_missing_files() {
        let repo_dir = tempfile::tempdir().expect("repo dir");
        fs::write(
            repo_dir.path().join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\n",
        )
        .unwrap();

        let snapshot = load_snapshot_from(repo_dir.path());
        assert!(snapshot.installed);
        assert!(snapshot.history.is_empty());
        assert!(snapshot.error.is_none());
    }

    #[test]
    fn load_snapshot_from_reads_report_and_history() {
        let repo_dir = tempfile::tempdir().expect("repo dir");
        fs::write(
            repo_dir.path().join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::create_dir_all(repo_dir.path().join("agent")).unwrap();
        fs::write(
            repo_dir.path().join("agent/repo-score.json"),
            r#"{"generated_at":"1778038040","score":92,"raw_score":92,"decision":{"status":"advisory","minimum_score":85,"passed":true},"conformance_decision":"block","dimensions":[],"caps_applied":["cap-a"],"findings":[{"severity":"high","hardness":"hard","path":"src/lib.rs","problem":"one","agent_fix":"fix one","evidence":["a"],"rule_id":"rule-a","lane":"fast","owner":"tools"}]}"#,
        )
        .unwrap();
        fs::write(
            repo_dir.path().join("agent/score-history.jsonl"),
            r#"{"generated_at":"1778038030","score":88,"raw_score":89}
{"generated_at":"1778038040","score":92,"raw_score":92}"#,
        )
        .unwrap();

        let snapshot = load_snapshot_from(repo_dir.path());
        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(snapshot.history.len(), 2);
        assert!(snapshot.error.is_none());
    }
}
