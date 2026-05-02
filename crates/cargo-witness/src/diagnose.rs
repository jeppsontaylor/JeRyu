use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;

use crate::model::{CompilePacket, CompilePackets, CompileSummary};

/// Run `cargo check --message-format=json` and route each diagnostic
/// to its owning ARC.
///
/// Enriches each diagnostic with cell purpose, invariants, and local
/// test commands from the workspace snapshot.
pub fn diagnose_workspace(
    workspace_root: &Path,
    manifest_path: Option<&Path>,
) -> Result<CompilePackets> {
    let manifest_path = manifest_path
        .map(|path| {
            path.canonicalize()
                .with_context(|| format!("failed to canonicalize {}", path.display()))
        })
        .transpose()?;
    let snapshot = cargo_vrc::load_workspace(manifest_path.as_deref())?;

    // Run cargo check and capture JSON diagnostics.
    let mut command = Command::new("cargo");
    command
        .arg("check")
        .arg("--workspace")
        .arg("--message-format=json")
        .current_dir(workspace_root);
    if let Some(path) = manifest_path.as_deref() {
        command.arg("--manifest-path").arg(path);
    }

    let output = command
        .output()
        .context("failed to run cargo check --message-format=json")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut packets = Vec::new();
    let mut arcs_affected = BTreeSet::new();
    let mut total_errors = 0usize;
    let mut total_warnings = 0usize;

    for line in stdout.lines() {
        let Ok(msg) = serde_json::from_str::<CargoMessage>(line) else {
            continue;
        };

        if msg.reason != "compiler-message" {
            continue;
        }

        let Some(diagnostic) = msg.message else {
            continue;
        };

        // Only route errors and warnings, not notes/help.
        if !matches!(diagnostic.level.as_str(), "error" | "warning") {
            continue;
        }

        // Find the primary span.
        let primary_span = diagnostic
            .spans
            .iter()
            .find(|span| span.is_primary)
            .or_else(|| diagnostic.spans.first());

        let (file, line_num, column) = if let Some(span) = primary_span {
            (span.file_name.clone(), span.line_start, span.column_start)
        } else {
            ("<unknown>".to_string(), 0, 0)
        };

        // Route to owning ARC.
        let owning_arc = snapshot
            .packages
            .iter()
            .find(|package| {
                let pkg_root = package
                    .package_root
                    .strip_prefix(&snapshot.workspace_root)
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                file.starts_with(&pkg_root)
            })
            .map(|package| package.name.clone())
            .unwrap_or_else(|| "<unmatched>".to_string());

        let cell_purpose = snapshot
            .packages
            .iter()
            .find(|p| p.name == owning_arc)
            .map(|p| p.agent.purpose.clone())
            .filter(|purpose| !purpose.is_empty());

        let invariants = snapshot
            .packages
            .iter()
            .find(|p| p.name == owning_arc)
            .map(|p| p.agent.invariants.clone())
            .unwrap_or_default();

        let local_commands = snapshot
            .packages
            .iter()
            .find(|p| p.name == owning_arc)
            .map(|p| p.agent.local_validate.clone())
            .unwrap_or_default();

        // Extract compiler suggestion if present.
        let compiler_suggestion = diagnostic
            .children
            .iter()
            .find(|child| child.level == "help")
            .map(|child| child.message.clone());

        match diagnostic.level.as_str() {
            "error" => total_errors += 1,
            "warning" => total_warnings += 1,
            _ => {}
        }
        arcs_affected.insert(owning_arc.clone());

        packets.push(CompilePacket {
            level: diagnostic.level,
            code: diagnostic.code.as_ref().map(|c| c.code.clone()),
            message: diagnostic.message,
            file,
            line: line_num,
            column,
            owning_arc,
            cell_purpose,
            invariants,
            local_commands,
            compiler_suggestion,
        });
    }

    Ok(CompilePackets {
        generated_at: Utc::now().format("%Y-%m-%d").to_string(),
        packets,
        summary: CompileSummary {
            total_errors,
            total_warnings,
            arcs_affected: arcs_affected.len(),
        },
    })
}

/// Write compile packets to disk.
pub fn write_compile_packets(workspace_root: &Path, packets: &CompilePackets) -> Result<()> {
    let output_dir = workspace_root.join("target/agent");
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let output_path = output_dir.join("compile-packets.json");
    let json = serde_json::to_string_pretty(packets)?;
    fs::write(&output_path, json)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(())
}

// ── Cargo JSON diagnostic types ────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CargoMessage {
    reason: String,
    #[serde(default)]
    message: Option<Diagnostic>,
}

#[derive(Debug, Deserialize)]
struct Diagnostic {
    level: String,
    message: String,
    #[serde(default)]
    code: Option<DiagnosticCode>,
    #[serde(default)]
    spans: Vec<DiagnosticSpan>,
    #[serde(default)]
    children: Vec<DiagnosticChild>,
}

#[derive(Debug, Deserialize)]
struct DiagnosticCode {
    code: String,
}

#[derive(Debug, Deserialize)]
struct DiagnosticSpan {
    file_name: String,
    line_start: u32,
    column_start: u32,
    #[serde(default)]
    is_primary: bool,
}

#[derive(Debug, Deserialize)]
struct DiagnosticChild {
    level: String,
    message: String,
}
