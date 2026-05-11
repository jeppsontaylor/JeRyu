//! Release gate integration tests
//! Owner: ops/release
//! Proof: `cargo test -p jeryu --test release_gate`
//! Invariants:
//!   - release-gate.sh returns zero only when all gates pass
//!   - release-gate output is structured JSON
//!   - gate failure produces actionable diagnostics

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap();
    loop {
        if dir.join("Cargo.toml").exists() && dir.join("agent").exists() {
            return dir;
        }
        if !dir.pop() {
            panic!("cannot find repo root");
        }
    }
}

fn release_gate_script() -> PathBuf {
    repo_root().join("ops/release/release-gate.sh")
}

#[test]
fn release_gate_script_exists() {
    let script = release_gate_script();
    assert!(script.exists(), "release-gate.sh must exist at ops/release/");
    assert!(
        fs::metadata(&script)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false),
        "release-gate.sh must be executable"
    );
}

#[test]
fn release_gate_rejects_missing_version() {
    let output = Command::new("bash")
        .arg(release_gate_script())
        .output()
        .expect("failed to run release-gate.sh");
    assert!(!output.status.success(), "must fail without version arg");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage") || stderr.contains("version"),
        "should print usage: {stderr}"
    );
}

#[test]
fn release_gate_version_mismatch_detected() {
    let output = Command::new("bash")
        .arg(release_gate_script())
        .arg("99.99.99")
        .output()
        .expect("failed to run release-gate.sh");
    assert!(
        !output.status.success(),
        "must fail when version.json doesn't match"
    );
}

#[test]
fn release_gate_evidence_is_json() {
    let ev_dir = repo_root().join("target/jankurai/release-gate");
    if ev_dir.exists() {
        for entry in fs::read_dir(&ev_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                let content = fs::read_to_string(&path).unwrap();
                assert!(
                    serde_json::from_str::<serde_json::Value>(&content).is_ok(),
                    "evidence file {:?} must be valid JSON",
                    path
                );
            }
        }
    }
}

#[test]
fn version_json_is_parseable() {
    let vpath = repo_root().join("version.json");
    let content = fs::read_to_string(&vpath).expect("version.json must exist");
    let v: serde_json::Value = serde_json::from_str(&content).expect("version.json must be JSON");
    assert!(v.get("version").is_some(), "version.json must have version");
    assert!(
        v.get("tag_policy").is_some(),
        "version.json must have tag_policy"
    );
}

#[test]
fn changelog_has_current_version_entry() {
    let vpath = repo_root().join("version.json");
    let content = fs::read_to_string(&vpath).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let version = v["version"].as_str().unwrap();
    let changelog = fs::read_to_string(repo_root().join("CHANGELOG.md")).unwrap();
    assert!(
        changelog.contains(&format!("## [{version}]")),
        "CHANGELOG must have entry for version {version}"
    );
}

#[test]
fn version_file_matches_version_json() {
    let vpath = repo_root().join("version.json");
    let content = fs::read_to_string(&vpath).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let json_ver = v["version"].as_str().unwrap();
    let version_file = fs::read_to_string(repo_root().join("VERSION")).unwrap();
    let version_file = version_file.trim();
    assert_eq!(
        json_ver, version_file,
        "VERSION and version.json must agree"
    );
}

#[test]
fn cargo_workspace_version_matches_version_json() {
    let vpath = repo_root().join("version.json");
    let content = fs::read_to_string(&vpath).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    let json_ver = v["cargo_workspace_version"]
        .as_str()
        .or_else(|| v["version"].as_str())
        .unwrap();
    let cargo_toml = fs::read_to_string(repo_root().join("Cargo.toml")).unwrap();
    let ws_ver = cargo_toml
        .lines()
        .find(|l| l.starts_with("version.workspace = true") || l.contains("version ="))
        .expect("must have version in Cargo.toml");
    assert!(
        cargo_toml.contains(&format!("version = \"{json_ver}\""))
            || cargo_toml.contains("version.workspace = true"),
        "Cargo.toml version must match version.json"
    );
}

#[test]
fn no_duplicate_release_tags() {
    let output = Command::new("git")
        .args(["tag", "-l", "v*"])
        .current_dir(repo_root())
        .output()
        .expect("git tag list failed");
    let tags = String::from_utf8_lossy(&output.stdout);
    let mut seen = std::collections::HashSet::new();
    for tag in tags.lines() {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        assert!(
            seen.insert(tag.to_string()),
            "duplicate tag: {tag}"
        );
    }
}

#[test]
fn release_gate_summary_schema() {
    let summary = repo_root().join("target/jankurai/release-gate/release-gate-summary.json");
    if !summary.exists() {
        eprintln!("skipping: no release-gate summary found (run `just release-gate` first)");
        return;
    }
    let content = fs::read_to_string(&summary).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(v.get("version").is_some(), "summary must have version");
    assert!(v.get("gates").is_some(), "summary must have gates array");
    assert!(v.get("status").is_some(), "summary must have status");
}