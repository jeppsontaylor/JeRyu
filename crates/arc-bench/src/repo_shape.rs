use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result};
use cargo_vrc::planner::context_metrics;
use cargo_vrc::{build_vrc_plan, load_workspace};
use chrono::Utc;

use crate::model::{BenchVariantResult, ScenarioReport};

pub fn run(output: &Path) -> Result<ScenarioReport> {
    let root = workspace_root();
    let fixtures = [
        Fixture {
            variant: "monolith",
            manifest_path: root.join("proof/labs/repo-shape-bench/monolith/Cargo.toml"),
            changed: vec![PathBuf::from("src/orders.rs")],
        },
        Fixture {
            variant: "arcified",
            manifest_path: root.join("proof/labs/repo-shape-bench/arcified/Cargo.toml"),
            changed: vec![PathBuf::from("crates/orders-core/src/lib.rs")],
        },
    ];

    let mut results = Vec::new();
    for fixture in fixtures {
        let snapshot = load_workspace(Some(&fixture.manifest_path))?;
        let plan = build_vrc_plan(&snapshot, &fixture.changed)?;
        let start = Instant::now();
        for test in &plan.selected_tests {
            run_shell(&snapshot.workspace_root, &test.command)?;
        }
        let wall_time_ms = start.elapsed().as_millis() as u64;
        let mut context_files = 0usize;
        let mut context_bytes = 0u64;
        for selected in &plan.selected_arcs {
            if let Some(package) = snapshot
                .packages
                .iter()
                .find(|package| package.name == selected.name)
            {
                let (files, bytes) =
                    context_metrics(&snapshot.workspace_root, &package.package_root)?;
                context_files += files;
                context_bytes += bytes;
            }
        }
        results.push(BenchVariantResult {
            scenario: "repo-shape".to_string(),
            variant: fixture.variant.to_string(),
            wall_time_ms,
            peak_rss_kb: None,
            thread_count_max: None,
            throughput: None,
            latency_p50_ms: None,
            latency_p95_ms: None,
            context_files: Some(context_files),
            context_bytes: Some(context_bytes),
            selected_tests: Some(plan.selected_tests.len()),
            selected_arcs: Some(plan.selected_arcs.len()),
            notes: vec![
                format!("Changed paths: {}", plan.changed_paths.join(", ")),
                format!("Stop condition: {}", plan.stop_condition),
            ],
        });
    }

    let report = ScenarioReport {
        scenario: "repo-shape".to_string(),
        generated_at: Utc::now().format("%Y-%m-%d").to_string(),
        results,
        cases: Vec::new(),
        notes: vec![
            "Both fixtures implement the same order-quoting flow, but one lives in a monolith and one is split into ARCs.".to_string(),
            "The changed path models a local business-rule change rather than a public API break.".to_string(),
        ],
    };
    std::fs::write(output, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(report)
}

struct Fixture<'a> {
    variant: &'a str,
    manifest_path: PathBuf,
    changed: Vec<PathBuf>,
}

fn run_shell(cwd: &Path, command: &str) -> Result<()> {
    let status = Command::new("/bin/zsh")
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to run `{command}`"))?;
    if !status.success() {
        anyhow::bail!("command failed: {command}");
    }
    Ok(())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}
