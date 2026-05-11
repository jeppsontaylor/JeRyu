//! Owner: Agent Surface
//! Proof: `cargo check -p jeryu && cargo test -p jeryu agent_surface`
//! Invariants: Generated routing index is derived from repo truth; audit fails on missing hard surfaces and outdated generated output.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const REQUIRED_ROOT_SECTIONS: &[&str] = &[
    "Proof Routing",
    "Proof Commands",
    "Module Ownership",
    "Cross-Repo Contract",
    "Guardrails",
    "Diagnostics",
    "Token Optimization",
];

#[path = "agent_surface_index.rs"]
mod agent_surface_index;
use agent_surface_index::*;

#[derive(Debug, Clone, Deserialize)]
struct ProofLanesFile {
    #[serde(default)]
    lane: BTreeMap<String, ProofLane>,
    #[serde(default)]
    change_type: BTreeMap<String, ChangeType>,
    #[serde(default)]
    module_hints: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProofLane {
    command: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ChangeType {
    #[serde(default)]
    lanes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AgentIndex {
    generated_at: String,
    repo_root: String,
    token_budget_path: String,
    entries: Vec<AgentIndexEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct AgentIndexEntry {
    id: String,
    path: String,
    owner: String,
    proof: String,
    invariants: String,
    default_change_type: String,
    proof_lanes: Vec<String>,
    proof_commands: Vec<String>,
    widening_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AgentSurfaceAudit {
    ok: bool,
    token_budget_present: bool,
    root_agents_ok: bool,
    rtk_doc_present: bool,
    index_current: bool,
    modules_checked: usize,
    issues: Vec<AuditIssue>,
    warnings: Vec<AuditIssue>,
}

#[derive(Debug, Clone, Serialize)]
struct AuditIssue {
    scope: String,
    path: String,
    detail: String,
}

pub fn render_agent_index(check: bool) -> Result<()> {
    let root = repo_root()?;
    let index = build_index(&root)?;
    let json_text = serde_json::to_string_pretty(&index)?;
    let markdown_text = render_markdown(&index);
    let json_path = root.join("agent-index.json");
    let markdown_path = root.join("agent-index.md");

    if check {
        if !generated_index_is_current(&json_path, &json_text, &markdown_path, &markdown_text) {
            bail!(
                "agent index drift detected; run `cargo run -p jeryu -- repo render-agent-index`"
            );
        }
        return Ok(());
    }

    fs::write(&json_path, json_text).with_context(|| format!("write {}", json_path.display()))?;
    fs::write(&markdown_path, markdown_text)
        .with_context(|| format!("write {}", markdown_path.display()))?;
    println!("{}", json_path.display());
    println!("{}", markdown_path.display());
    Ok(())
}

pub fn audit_agent_surface(as_json: bool) -> Result<()> {
    let root = repo_root()?;
    let report = build_audit_report(&root)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Agent surface audit");
        println!(
            "  token budget: {}",
            if report.token_budget_present {
                "ok"
            } else {
                "missing"
            }
        );
        println!(
            "  root AGENTS:  {}",
            if report.root_agents_ok {
                "ok"
            } else {
                "needs work"
            }
        );
        println!(
            "  RTK docs:     {}",
            if report.rtk_doc_present {
                "ok"
            } else {
                "missing"
            }
        );
        println!(
            "  index fresh:  {}",
            if report.index_current {
                "ok"
            } else {
                "outdated"
            }
        );
        println!("  modules:      {}", report.modules_checked);
        if !report.issues.is_empty() {
            println!("\nIssues:");
            for issue in &report.issues {
                println!("  - {} [{}]: {}", issue.scope, issue.path, issue.detail);
            }
        }
        if !report.warnings.is_empty() {
            println!("\nWarnings:");
            for warning in &report.warnings {
                println!(
                    "  - {} [{}]: {}",
                    warning.scope, warning.path, warning.detail
                );
            }
        }
    }
    if !report.ok {
        bail!("agent surface audit failed");
    }
    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("current dir")?;
    Ok(cwd)
}

fn build_audit_report(root: &Path) -> Result<AgentSurfaceAudit> {
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let token_budget_present = root.join("token-budget.toml").is_file();
    if !token_budget_present {
        issues.push(AuditIssue {
            scope: "root".to_string(),
            path: "token-budget.toml".to_string(),
            detail: "missing token budget configuration".to_string(),
        });
    }

    let root_agents = root.join("AGENTS.md");
    let root_agents_ok = check_sections(&root_agents, REQUIRED_ROOT_SECTIONS, &mut issues)?;

    let rtk_doc = root.join("docs/RTK.md");
    let rtk_doc_present = rtk_doc.is_file();
    if !rtk_doc_present {
        issues.push(AuditIssue {
            scope: "root".to_string(),
            path: "docs/RTK.md".to_string(),
            detail: "missing RTK usage guidance".to_string(),
        });
    }

    let entries = module_entries(root)?;
    for entry in &entries {
        if entry.owner.trim().is_empty() {
            warnings.push(AuditIssue {
                scope: "module".to_string(),
                path: entry.path.clone(),
                detail: "missing `//! Owner:` header".to_string(),
            });
        }
        if entry.proof.trim().is_empty() {
            warnings.push(AuditIssue {
                scope: "module".to_string(),
                path: entry.path.clone(),
                detail: "missing `//! Proof:` header".to_string(),
            });
        }
        if entry.invariants.trim().is_empty() {
            warnings.push(AuditIssue {
                scope: "module".to_string(),
                path: entry.path.clone(),
                detail: "missing `//! Invariants:` header".to_string(),
            });
        }
    }

    let index = build_index(root)?;
    let expected_json = serde_json::to_string_pretty(&index)?;
    let expected_markdown = render_markdown(&index);
    let index_current = generated_index_is_current(
        &root.join("agent-index.json"),
        &expected_json,
        &root.join("agent-index.md"),
        &expected_markdown,
    );
    if !index_current {
        issues.push(AuditIssue {
            scope: "root".to_string(),
            path: "agent-index.{json,md}".to_string(),
            detail: "generated index is missing or outdated".to_string(),
        });
    }

    Ok(AgentSurfaceAudit {
        ok: issues.is_empty(),
        token_budget_present,
        root_agents_ok,
        rtk_doc_present,
        index_current,
        modules_checked: entries.len(),
        issues,
        warnings,
    })
}

#[cfg(test)]
#[path = "agent_surface_tests.rs"]
mod tests;
