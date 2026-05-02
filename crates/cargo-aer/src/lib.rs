use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_vrc::{PackageAgentMetadata, WorkspaceSnapshot, load_workspace};
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub class_id: String,
    pub severity: String,
    pub confidence: f64,
    pub path: String,
    pub summary: String,
    pub suggested_fix: String,
    #[serde(default)]
    pub existing_exception: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub generated_at: String,
    pub workspace_root: String,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AerRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub class_id: String,
    #[serde(default)]
    pub rule: String,
    #[serde(default)]
    pub exception: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub risk: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub doc_links: Vec<String>,
    #[serde(default)]
    pub sunset_condition: String,
}

pub fn scan_workspace(manifest_path: Option<&Path>) -> Result<ScanReport> {
    let snapshot = load_workspace(manifest_path)?;
    let mut findings = Vec::new();
    for package in &snapshot.packages {
        findings.extend(scan_package(&snapshot, package)?);
    }
    for finding in &mut findings {
        if finding.existing_exception.is_some() {
            finding.severity = "note".to_string();
        }
    }
    findings.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.class_id.cmp(&right.class_id))
    });
    Ok(ScanReport {
        generated_at: Utc::now().format("%Y-%m-%d").to_string(),
        workspace_root: display_workspace_root(),
        findings,
    })
}

pub fn init_records(workspace_root: &Path) -> Result<Vec<PathBuf>> {
    let records_dir = workspace_root.join("aer-records");
    fs::create_dir_all(&records_dir)
        .with_context(|| format!("failed to create {}", records_dir.display()))?;
    let readme = records_dir.join("README.md");
    if !readme.exists() {
        fs::write(
            &readme,
            "# Agent Exception Records\n\nEach YAML file captures one explicit break from a strict default.\n",
        )
        .with_context(|| format!("failed to write {}", readme.display()))?;
    }
    let example = records_dir.join("EXAMPLE.yaml");
    if !example.exists() {
        fs::write(
            &example,
            r#"id: aer.example.mega-file-parser
class_id: mega-file
rule: "Files should stay below the house budget unless locality would be harmed."
exception: "Parser table remains in one file to preserve traceability."
reason: "Splitting the grammar table would make cross-rule debugging harder during a protocol migration."
risk: medium
owner: parser-team
doc_links:
  - https://doc.rust-lang.org/book/ch13-04-performance.html
sunset_condition: "Remove when the grammar format is stabilized and table generation lands."
"#,
        )
        .with_context(|| format!("failed to write {}", example.display()))?;
    }
    Ok(vec![readme, example])
}

pub fn stale_records(workspace_root: &Path) -> Result<Vec<Finding>> {
    let records_dir = workspace_root.join("aer-records");
    let mut findings = Vec::new();
    if !records_dir.exists() {
        return Ok(findings);
    }
    for entry in WalkDir::new(&records_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let path = entry.path();
        if !path.is_file()
            || !matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("yaml" | "yml")
            )
        {
            continue;
        }
        let record = parse_record(path)?;
        if record.owner.trim().is_empty() {
            findings.push(Finding {
                class_id: "stale-aer".to_string(),
                severity: "warning".to_string(),
                confidence: 0.95,
                path: display_relative(workspace_root, path),
                summary: "AER is missing an owner".to_string(),
                suggested_fix: "Set owner so the exception has clear stewardship.".to_string(),
                existing_exception: Some(record.id.clone()),
            });
        }
        if record.sunset_condition.trim().is_empty() {
            findings.push(Finding {
                class_id: "stale-aer".to_string(),
                severity: "warning".to_string(),
                confidence: 0.95,
                path: display_relative(workspace_root, path),
                summary: "AER is missing a sunset condition".to_string(),
                suggested_fix: "Add a concrete reevaluation condition or review date.".to_string(),
                existing_exception: Some(record.id.clone()),
            });
        }
    }
    Ok(findings)
}

pub fn markdown_report(report: &ScanReport) -> String {
    let mut output = String::new();
    output.push_str("# cargo-aer Findings\n\n");
    output.push_str(&format!("Generated: {}\n\n", report.generated_at));
    output.push_str("| Class | Severity | Confidence | Path | Summary | Suggested Fix |\n");
    output.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for finding in &report.findings {
        output.push_str(&format!(
            "| {} | {} | {:.2} | `{}` | {} | {} |\n",
            finding.class_id,
            finding.severity,
            finding.confidence,
            finding.path,
            finding.summary.replace('|', "\\|"),
            finding.suggested_fix.replace('|', "\\|")
        ));
    }
    output
}

pub fn sarif_report(report: &ScanReport) -> serde_json::Value {
    let mut rules = BTreeMap::new();
    for finding in &report.findings {
        rules.entry(finding.class_id.clone()).or_insert_with(|| {
            serde_json::json!({
                "id": finding.class_id,
                "name": finding.class_id,
                "shortDescription": { "text": finding.summary },
                "help": { "text": finding.suggested_fix },
            })
        });
    }

    serde_json::json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "cargo-aer",
                    "rules": rules.into_values().collect::<Vec<_>>(),
                }
            },
            "results": report.findings.iter().map(|finding| {
                serde_json::json!({
                    "ruleId": finding.class_id,
                    "level": sarif_level(&finding.severity),
                    "message": { "text": finding.summary },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": finding.path }
                        }
                    }]
                })
            }).collect::<Vec<_>>()
        }]
    })
}

fn scan_package(
    snapshot: &WorkspaceSnapshot,
    package: &cargo_vrc::workspace::PackageSnapshot,
) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    let existing = |class_id: &str| existing_exception(&package.agent, class_id);

    if package.agent.purpose.trim().is_empty()
        || package.agent.local_validate.is_empty()
        || package.agent.invariants.is_empty()
    {
        findings.push(Finding {
            class_id: "missing-local-validation-metadata".to_string(),
            severity: "warning".to_string(),
            confidence: 0.98,
            path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            summary: format!(
                "{} is missing purpose, invariants, or local validation metadata",
                package.name
            ),
            suggested_fix: "Populate package.metadata.agent with purpose, invariants, and local_validate commands.".to_string(),
            existing_exception: existing("missing-local-validation-metadata"),
        });
    }

    if looks_like_junk_drawer(&package.name) {
        findings.push(Finding {
            class_id: "junk-drawer-shared-crate".to_string(),
            severity: "warning".to_string(),
            confidence: 0.85,
            path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            summary: format!("{} looks like a generic shared crate", package.name),
            suggested_fix:
                "Rename or split the crate by domain so ownership and blast radius stay clear."
                    .to_string(),
            existing_exception: existing("junk-drawer-shared-crate"),
        });
    }

    let manifest_value: toml::Value = toml::from_str(
        &fs::read_to_string(&package.manifest_path)
            .with_context(|| format!("failed to read {}", package.manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", package.manifest_path.display()))?;

    if let Some(feature_count) = feature_count(&manifest_value)
        && feature_count > 8
    {
        findings.push(Finding {
                class_id: "feature-explosion".to_string(),
                severity: if feature_count > 12 { "error" } else { "warning" }.to_string(),
                confidence: 0.9,
                path: display_relative(&snapshot.workspace_root, &package.manifest_path),
                summary: format!("{} declares {} Cargo features", package.name, feature_count),
                suggested_fix: "Prefer additive features and split incompatible modes into separate crates when possible.".to_string(),
                existing_exception: existing("feature-explosion"),
            });
    }

    if package.agent.public_api
        && !package
            .agent
            .local_validate
            .iter()
            .any(|command| command.contains("--doc"))
    {
        findings.push(Finding {
            class_id: "public-api-no-doctest-coverage".to_string(),
            severity: "warning".to_string(),
            confidence: 0.92,
            path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            summary: format!(
                "{} is public_api=true but local validation omits doctests",
                package.name
            ),
            suggested_fix:
                "Add `cargo test -p <crate> --doc` to local_validate or document an AER."
                    .to_string(),
            existing_exception: existing("public-api-no-doctest-coverage"),
        });
    }

    if package.agent.public_api
        && !workspace_has_semver_profile(
            &snapshot.workspace_agent.ci_profiles,
            &snapshot.workspace_agent.shared_contracts,
        )
    {
        findings.push(Finding {
            class_id: "public-api-no-semver-gate".to_string(),
            severity: "warning".to_string(),
            confidence: 0.7,
            path: display_relative(&snapshot.workspace_root, &package.manifest_path),
            summary: format!(
                "{} is public_api=true but the workspace does not advertise semver checks",
                package.name
            ),
            suggested_fix: "Add cargo-semver-checks to a scheduled hardening CI profile."
                .to_string(),
            existing_exception: existing("public-api-no-semver-gate"),
        });
    }

    let tests_dir = package.package_root.join("tests");
    if tests_dir.exists() {
        let integration_count = WalkDir::new(&tests_dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_file())
            .count();
        if integration_count > 5 {
            findings.push(Finding {
                class_id: "integration-test-forest".to_string(),
                severity: "warning".to_string(),
                confidence: 0.97,
                path: display_relative(&snapshot.workspace_root, &tests_dir),
                summary: format!("{} has {} top-level integration test files", package.name, integration_count),
                suggested_fix: "Consolidate related tests into a smaller set of harnesses with internal modules.".to_string(),
                existing_exception: existing("integration-test-forest"),
            });
        }
    }

    let pkg_root = &package.package_root;
    for entry in WalkDir::new(pkg_root)
        .into_iter()
        .filter_entry(|e| {
            // Skip the workspace `target/` directory (generated build artifacts).
            let name = e.file_name().to_string_lossy();
            !(e.depth() == 1 && name == "target")
        })
        .filter_map(|entry| entry.ok())
    {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let line_count = content.lines().count();
        if line_count > 500 {
            let relative_path = display_relative(&snapshot.workspace_root, path);
            findings.push(Finding {
                class_id: "mega-file".to_string(),
                severity: if line_count > 800 { "error" } else { "warning" }.to_string(),
                confidence: 0.99,
                path: relative_path.clone(),
                summary: format!("{} is {} lines long", relative_path, line_count),
                suggested_fix: "Extract a smaller ARC-facing module or record an AER explaining why locality wins.".to_string(),
                existing_exception: existing("mega-file"),
            });
        }

        findings.extend(scan_function_lengths(
            &snapshot.workspace_root,
            path,
            &content,
            &existing,
        )?);
        findings.extend(scan_unsafe_blocks(
            &snapshot.workspace_root,
            path,
            &content,
            &existing,
        ));

        if looks_like_core_layer(&package.agent, &package.name) && hidden_io_signal(&content) {
            findings.push(Finding {
                class_id: "hidden-side-effects".to_string(),
                severity: "warning".to_string(),
                confidence: 0.73,
                path: display_relative(&snapshot.workspace_root, path),
                summary: "File appears to perform I/O or env access inside a core or pure layer".to_string(),
                suggested_fix: "Move I/O and env access to an adapter boundary and inject the result into the core layer.".to_string(),
                existing_exception: existing("hidden-side-effects"),
            });
        }
    }

    Ok(findings)
}

fn scan_function_lengths(
    workspace_root: &Path,
    path: &Path,
    content: &str,
    existing: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    let signature = Regex::new(r"^\s*(pub\s+)?(async\s+)?fn\s+([A-Za-z0-9_]+)")?;
    let lines = content.lines().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if let Some(captures) = signature.captures(line) {
            let name = captures
                .get(3)
                .map(|value| value.as_str())
                .unwrap_or("unknown");
            let start = index;
            let mut seen_body = line.contains('{');
            let mut depth = line.matches('{').count() as i32 - line.matches('}').count() as i32;
            index += 1;
            while index < lines.len() {
                let current = lines[index];
                if current.contains('{') {
                    seen_body = true;
                }
                depth += current.matches('{').count() as i32;
                depth -= current.matches('}').count() as i32;
                index += 1;
                if seen_body && depth <= 0 {
                    break;
                }
            }
            let length = index.saturating_sub(start);
            if length > 500 {
                findings.push(Finding {
                    class_id: "mega-function".to_string(),
                    severity: if length > 600 { "error" } else { "warning" }.to_string(),
                    confidence: 0.78,
                    path: display_relative(workspace_root, path),
                    summary: format!("function `{}` spans {} lines", name, length),
                    suggested_fix: "Extract helpers, compress control flow, or capture the exception in an AER.".to_string(),
                    existing_exception: existing("mega-function"),
                });
            }
            continue;
        }
        index += 1;
    }
    Ok(findings)
}

fn scan_unsafe_blocks(
    workspace_root: &Path,
    path: &Path,
    content: &str,
    existing: &dyn Fn(&str) -> Option<String>,
) -> Vec<Finding> {
    let pattern = Regex::new(r"\bunsafe(\s+fn|\s*\{)").expect("valid unsafe regex");
    let lines = content.lines().collect::<Vec<_>>();
    let mut findings = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with('*') || !pattern.is_match(trimmed) {
            continue;
        }
        let start = index.saturating_sub(2);
        let has_safety_comment = lines[start..=index]
            .iter()
            .any(|candidate| candidate.contains("SAFETY:"));
        if !has_safety_comment {
            findings.push(Finding {
                class_id: "undocumented-unsafe".to_string(),
                severity: "error".to_string(),
                confidence: 0.96,
                path: display_relative(workspace_root, path),
                summary: "unsafe block or function is missing a nearby SAFETY comment".to_string(),
                suggested_fix:
                    "Document the safety preconditions immediately above the unsafe site."
                        .to_string(),
                existing_exception: existing("undocumented-unsafe"),
            });
        }
    }
    findings
}

fn parse_record(path: &Path) -> Result<AerRecord> {
    let payload =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&payload).with_context(|| format!("failed to parse {}", path.display()))
}

fn feature_count(value: &toml::Value) -> Option<usize> {
    value
        .get("features")
        .and_then(|features| features.as_table())
        .map(|table| table.len())
}

fn workspace_has_semver_profile(
    ci_profiles: &[cargo_vrc::model::CiProfile],
    shared_contracts: &[String],
) -> bool {
    if ci_profiles
        .iter()
        .find(|profile| profile.name.eq_ignore_ascii_case("scheduled-hardening"))
        .is_some_and(|profile| {
            profile
                .commands
                .iter()
                .any(|command| command_has_semver_signal(command))
        })
    {
        return true;
    }

    if ci_profiles.iter().any(|profile| {
        profile
            .commands
            .iter()
            .any(|command| command_has_semver_signal(command))
    }) {
        return true;
    }

    shared_contracts
        .iter()
        .any(|contract| contract.to_ascii_lowercase().contains("semver"))
}

fn command_has_semver_signal(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    command.contains("semver-checks") || command.contains("semver")
}

fn looks_like_junk_drawer(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["common", "utils", "helpers", "shared"]
        .iter()
        .any(|needle| {
            name == *needle
                || name.starts_with(&format!("{needle}-"))
                || name.ends_with(&format!("-{needle}"))
        })
}

fn looks_like_core_layer(agent: &PackageAgentMetadata, name: &str) -> bool {
    let purpose = agent.purpose.to_ascii_lowercase();
    name.contains("core") || purpose.contains("pure") || purpose.contains("domain")
}

fn hidden_io_signal(content: &str) -> bool {
    [
        "std::fs::",
        "tokio::fs::",
        "std::env::",
        "reqwest::",
        "ureq::",
        "std::net::",
    ]
    .iter()
    .any(|needle| content.contains(needle))
}

fn existing_exception(agent: &PackageAgentMetadata, class_id: &str) -> Option<String> {
    let normalized = class_id.replace('-', "_");
    agent
        .exceptions
        .iter()
        .find(|exception| exception.contains(class_id) || exception.contains(&normalized))
        .cloned()
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

fn sarif_level(severity: &str) -> &'static str {
    match severity {
        "error" => "error",
        "warning" => "warning",
        _ => "note",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cargo_vrc::model::CiProfile;

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
    fn workspace_has_semver_profile_accepts_scheduled_hardening_commands() {
        let ci_profiles = vec![CiProfile {
            name: "scheduled-hardening".to_string(),
            commands: vec!["cargo semver-checks check-release".to_string()],
        }];

        assert!(workspace_has_semver_profile(&ci_profiles, &[]));
    }

    #[test]
    fn workspace_has_semver_profile_falls_back_to_shared_contracts() {
        let shared_contracts =
            vec!["Public APIs must run semver validation before release.".to_string()];

        assert!(workspace_has_semver_profile(&[], &shared_contracts));
    }

    #[test]
    fn workspace_has_semver_profile_rejects_missing_signal() {
        let ci_profiles = vec![CiProfile {
            name: "pull-request".to_string(),
            commands: vec!["cargo run -p cargo-aer -- scan --output aer-findings.json".to_string()],
        }];

        assert!(!workspace_has_semver_profile(&ci_profiles, &[]));
    }

    #[test]
    fn current_workspace_scan_uses_relative_paths_and_semver_gate() {
        let report = scan_workspace(Some(&current_manifest())).expect("scan current workspace");

        assert_eq!(report.workspace_root, ".");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| !finding.path.starts_with('/'))
        );
        assert!(
            report
                .findings
                .iter()
                .all(|finding| !finding.path.contains("/private/tmp/")
                    && !finding.path.contains("/Users/"))
        );
        assert!(
            !report
                .findings
                .iter()
                .any(|finding| finding.class_id == "public-api-no-semver-gate")
        );
    }

    #[test]
    fn scan_workspace_detects_hidden_io_in_core_fixture() {
        let manifest = workspace_root().join("labs/exception-zoo/cases/hidden-io-core/Cargo.toml");
        let report = scan_workspace(Some(&manifest)).expect("scan fixture workspace");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.class_id == "hidden-side-effects")
        );
    }

    #[test]
    fn scan_workspace_detects_missing_doctest_for_public_api_fixture() {
        let manifest = workspace_root().join("labs/exception-zoo/cases/semver-break/Cargo.toml");
        let report = scan_workspace(Some(&manifest)).expect("scan fixture workspace");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.class_id == "public-api-no-doctest-coverage")
        );
    }
}
