use super::*;
use std::collections::BTreeSet;
use std::path::Path;

const DOUX_MAIN_JOBS: &[&str] = &[
    "lint-cargo-fmt",
    "lint-cargo-clippy",
    "lint-shell",
    "lint-ci-schema",
    "vrc-plan",
    "compile-workspace",
    "build-release-artifacts",
    "build-bootstrap-musl",
    "build-enclave-server",
    "test-rust-cargo-vrc",
    "test-rust-veox-testctl",
    "test-rust-veox-deploy",
    "test-rust-nextest-1",
    "test-rust-nextest-2",
    "test-rust-nextest-3",
    "test-rust-nextest-4",
    "test-rust-contracts-core",
    "test-rust-contracts-persistence",
    "test-rust-contracts-warp",
    "test-rust-contracts-nht",
    "test-rust-contracts-retirement",
    "test-rust-nht-crypto",
    "test-frontend-warp",
    "test-frontend-nht",
    "test-public-deps",
    "test-security-hardening",
    "test-governance-retirement",
    "test-shell-container-parity",
    "test-security-ip-leak",
    "test-security-ip-guard",
    "test-security-ip-exfiltration",
    "test-smoke-warp-unified",
    "test-smoke-nht-datasets",
    "test-live-public-surface",
    "test-local-built",
    "publish-rc-dry-run",
    "test-local-rc",
    "audit-aer-scan",
    "audit-remaining-structural-findings",
    "audit-final-mile",
];

fn job_dependencies(job: &str) -> &'static [&'static str] {
    match job {
        "test-rust-nextest-1"
        | "test-rust-nextest-2"
        | "test-rust-nextest-3"
        | "test-rust-nextest-4" => &["compile-workspace"],
        "vrc-plan" => &["plan-tests"],
        "build-enclave-server" => &["build-release-artifacts", "build-bootstrap-musl"],
        "test-live-public-surface" | "test-local-built" | "publish-rc-dry-run" => {
            &["build-enclave-server"]
        }
        "test-local-rc" => &["publish-rc-dry-run"],
        _ => &[],
    }
}

fn add_job_with_dependencies(job: &str, selected: &mut BTreeSet<String>) {
    for dependency in job_dependencies(job) {
        if *dependency != "plan-tests" {
            add_job_with_dependencies(dependency, selected);
        }
    }
    selected.insert(job.to_string());
}

fn materialized_jobs(plan: &ExternalTestPlan) -> Vec<String> {
    let mut selected = BTreeSet::new();
    match plan.mode {
        ExternalPlanMode::Full => {
            for job in DOUX_MAIN_JOBS {
                add_job_with_dependencies(job, &mut selected);
            }
        }
        ExternalPlanMode::Selected => {
            for job in &plan.selected_jobs {
                add_job_with_dependencies(job, &mut selected);
            }
        }
        ExternalPlanMode::DocsOnly => {}
    }
    selected.into_iter().collect()
}

fn extract_top_level_yaml_block(content: &str, block_name: &str) -> Option<String> {
    let header = format!("{block_name}:");
    let mut started = false;
    let mut out = Vec::new();

    for line in content.lines() {
        let is_top_level = !line.starts_with(' ') && !line.starts_with('\t');
        if !started {
            if line.trim_start().starts_with(&header) {
                started = true;
                out.push(line.to_string());
            }
            continue;
        }

        if is_top_level && !line.trim().is_empty() && line.split_once(':').is_some() {
            break;
        }
        out.push(line.to_string());
    }

    if out.is_empty() {
        None
    } else {
        Some(format!("{}\n", out.join("\n")))
    }
}

fn top_level_block_names(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            if line.starts_with(' ') || line.starts_with('\t') {
                return None;
            }
            let (name, _) = line.split_once(':')?;
            if name.is_empty() {
                return None;
            }
            Some(name.to_string())
        })
        .collect()
}

fn strip_job_rules(block: &str) -> String {
    let mut out = Vec::new();
    let mut skipping_rules = false;

    for line in block.lines() {
        let indent = line.chars().take_while(|ch| *ch == ' ').count();
        if indent == 2 && line.trim() == "rules:" {
            skipping_rules = true;
            continue;
        }
        if skipping_rules {
            if !line.trim().is_empty() && indent <= 2 {
                skipping_rules = false;
            } else {
                continue;
            }
        }
        out.push(line.to_string());
    }

    format!("{}\n", out.join("\n"))
}

fn collect_ci_blocks(
    workspace: &Path,
) -> (Vec<String>, std::collections::BTreeMap<String, String>) {
    let mut hidden = Vec::new();
    let mut jobs = std::collections::BTreeMap::new();
    let ci_dir = workspace.join("ci/gitlab");
    let Ok(entries) = std::fs::read_dir(ci_dir) else {
        return (hidden, jobs);
    };

    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("yml") {
            paths.push(path);
        }
    }
    paths.sort();

    for path in paths {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for name in top_level_block_names(&content) {
            if let Some(block) = extract_top_level_yaml_block(&content, &name) {
                if name.starts_with('.') {
                    hidden.push(block);
                } else {
                    jobs.entry(name).or_insert(block);
                }
            }
        }
    }

    (hidden, jobs)
}

fn emit_child_plan_context(plan: &ExternalTestPlan) -> String {
    let changed_paths = plan.changed_paths.join("\n");
    let selected_jobs = materialized_jobs(plan);
    let selected_json = match serde_json::to_string(&selected_jobs) {
        Ok(s) => s,
        Err(_) => "[]".to_string(),
    };
    let plan_json = format!(
        "{{\"mode\":\"{}\",\"selected_jobs\":{selected_json}}}",
        match plan.mode {
            ExternalPlanMode::Full => "full",
            ExternalPlanMode::Selected => "selected",
            ExternalPlanMode::DocsOnly => "docs_only",
        }
    );
    let skipped_json = match serde_json::to_string(&explain_external_skipped_json(plan)) {
        Ok(value) => value,
        Err(_) => "{}".to_string(),
    };
    let heredoc_body = |value: &str| {
        value
            .lines()
            .map(|line| format!("         \x20     {line}\n"))
            .collect::<String>()
    };
    format!(
        "plan-tests:\n\
         \x20 stage: lint\n\
         \x20 image: alpine:3.20\n\
         \x20 tags: [default]\n\
         \x20 script:\n\
         \x20   - mkdir -p target/jeryu\n\
         \x20   - |\n\
         \x20     cat > target/jeryu/changed-files.txt <<'VTI_CHANGED_FILES'\n\
{changed_paths}         \x20     VTI_CHANGED_FILES\n\
         \x20   - |\n\
         \x20     cat > target/jeryu/vti-plan.json <<'VTI_PLAN_JSON'\n\
{plan_json}         \x20     VTI_PLAN_JSON\n\
         \x20   - |\n\
         \x20     cat > target/jeryu/vti-skipped.json <<'VTI_SKIPPED_JSON'\n\
{skipped_json}         \x20     VTI_SKIPPED_JSON\n\
         \x20 artifacts:\n\
         \x20   when: always\n\
         \x20   expire_in: 7 days\n\
         \x20   paths:\n\
         \x20     - target/jeryu/changed-files.txt\n\
         \x20     - target/jeryu/vti-plan.json\n\
         \x20     - target/jeryu/vti-skipped.json\n\n",
        changed_paths = heredoc_body(&changed_paths),
        plan_json = heredoc_body(&plan_json),
        skipped_json = heredoc_body(&skipped_json),
    )
}

/// Generate GitLab child pipeline YAML from an external test plan.
pub fn emit_external_gitlab_yaml(plan: &ExternalTestPlan, workspace: Option<&Path>) -> String {
    match &plan.mode {
        ExternalPlanMode::DocsOnly => {
            let comment = match &plan.mode {
                ExternalPlanMode::DocsOnly => "# VTI: docs-only — no tests required",
                _ => panic!("docs-only branch selected for non-docs-only mode"),
            };
            format!(
                "{comment}\n\
                 stages:\n  - noop\n\n\
                 vti-noop:\n\
                 \x20 stage: noop\n\
                 \x20 script: [\"echo 'VTI: {mode}'\"]\n",
                mode = match &plan.mode {
                    ExternalPlanMode::DocsOnly => "docs-only",
                    _ => panic!("docs-only branch selected for non-docs-only mode"),
                }
            )
        }
        ExternalPlanMode::Full | ExternalPlanMode::Selected => {
            let mut yaml = String::new();
            yaml.push_str("# Auto-generated by jeryu Test Intelligence.\n");
            yaml.push_str("# This child pipeline materializes only the VTI-selected graph.\n");
            yaml.push_str("# Do not run this file directly.\n\n");
            yaml.push_str("variables:\n");
            yaml.push_str("  CI_PIPELINE_PRODUCT: \"main-candidate\"\n");
            yaml.push_str("  VTI_FORCE_SELECTED_GRAPH: \"1\"\n");
            yaml.push_str("  VTI_STATIC_MAIN: \"1\"\n");
            yaml.push_str(&format!(
                "  VTI_SELECTED_JOBS: \",{},\"\n\n",
                materialized_jobs(plan).join(",")
            ));
            yaml.push_str(
                "stages:\n  - lint\n  - compile\n  - package\n  - test-rust\n  - test-tools\n  - test-shell\n  - test-security\n  - test-e2e\n  - audit\n  - audit-seed-data\n  - deploy\n  - report\n\n",
            );
            yaml.push_str(&emit_child_plan_context(plan));

            let Some(workspace) = workspace else {
                for job in materialized_jobs(plan) {
                    yaml.push_str(&format!(
                        "{job}:\n  stage: test-rust\n  image: rust:1.92.0\n  tags: [build]\n  script:\n    - cargo run -p veox-testctl -- ci-job {job}\n\n"
                    ));
                }
                return yaml;
            };

            let (hidden_blocks, job_blocks) = collect_ci_blocks(workspace);
            for block in hidden_blocks {
                yaml.push_str(&block);
                yaml.push('\n');
            }

            for job in materialized_jobs(plan) {
                if job == "plan-tests" {
                    continue;
                }
                if let Some(block) = job_blocks.get(&job) {
                    yaml.push_str(&strip_job_rules(block));
                    yaml.push('\n');
                } else {
                    yaml.push_str(&format!(
                        "{job}:\n  stage: test-rust\n  image: rust:1.92.0\n  tags: [build]\n  script:\n    - cargo run -p veox-testctl -- ci-job {job}\n\n"
                    ));
                }
            }

            yaml
        }
    }
}

/// Human-readable explanation of an external plan.
pub fn explain_external_plan(plan: &ExternalTestPlan) -> String {
    let mut out = String::new();
    let mode_label = match &plan.mode {
        ExternalPlanMode::Full => "FULL (all jobs)",
        ExternalPlanMode::Selected => "SELECTED (targeted jobs)",
        ExternalPlanMode::DocsOnly => "DOCS-ONLY (no tests)",
    };
    out.push_str("╭─ jeryu Test Intelligence Plan (external) ─────╮\n");
    out.push_str(&format!("│ Mode: {:<40} │\n", mode_label));
    out.push_str(&format!("│ Confidence: {:<34.2} │\n", plan.confidence));
    out.push_str("╰───────────────────────────────────────────────╯\n\n");

    if !plan.changed_paths.is_empty() {
        out.push_str("Changed:\n");
        for p in &plan.changed_paths {
            out.push_str(&format!("  ● {}\n", p));
        }
        out.push('\n');
    }

    if !plan.selected_jobs.is_empty() {
        out.push_str("Selected jobs:\n");
        for job in &plan.selected_jobs {
            out.push_str(&format!("  ✓ {}\n", job));
        }
        out.push('\n');
    }

    if !plan.skipped_jobs.is_empty() {
        out.push_str("Skipped jobs:\n");
        for job in &plan.skipped_jobs {
            out.push_str(&format!("  ○ {}\n", job));
        }
        out.push('\n');
    }

    if !plan.rationale.is_empty() {
        out.push_str("Rationale:\n");
        for reason in &plan.rationale {
            out.push_str(&format!("  → {}\n", reason));
        }
        out.push('\n');
    }

    if let Some(reason) = &plan.repair_reason {
        out.push_str(&format!("Recovery: {}\n", reason));
    }

    out.push_str(&format!(
        "Summary: {} jobs selected, {} jobs skipped, {} subsystems affected\n",
        plan.selected_jobs.len(),
        plan.skipped_jobs.len(),
        plan.affected_subsystems.len()
    ));

    out
}

/// JSON representation of an external plan.
pub fn explain_external_json(plan: &ExternalTestPlan) -> serde_json::Value {
    serde_json::json!({
        "mode": match &plan.mode {
            ExternalPlanMode::Full => "full",
            ExternalPlanMode::Selected => "selected",
            ExternalPlanMode::DocsOnly => "docs_only",
        },
        "confidence": plan.confidence,
        "selected_jobs": plan.selected_jobs,
        "skipped_jobs": plan.skipped_jobs,
        "affected_subsystems": plan.affected_subsystems,
        "rationale": plan.rationale,
        "changed_paths": plan.changed_paths,
        "repair_reason": plan.repair_reason,
    })
}

/// JSON metadata for jobs that VTI intentionally omitted from the graph.
pub fn explain_external_skipped_json(plan: &ExternalTestPlan) -> serde_json::Value {
    let materialized: BTreeSet<String> = materialized_jobs(plan).into_iter().collect();
    let skipped_jobs: Vec<String> = match plan.mode {
        ExternalPlanMode::Full => Vec::new(),
        ExternalPlanMode::Selected | ExternalPlanMode::DocsOnly => plan
            .skipped_jobs
            .iter()
            .filter(|job| !materialized.contains(*job))
            .cloned()
            .collect(),
    };

    serde_json::json!({
        "mode": match &plan.mode {
            ExternalPlanMode::Full => "full",
            ExternalPlanMode::Selected => "selected",
            ExternalPlanMode::DocsOnly => "docs_only",
        },
        "status": "vti-skipped",
        "skipped_jobs": skipped_jobs,
        "materialized_jobs": materialized.into_iter().collect::<Vec<_>>(),
        "reason": plan.repair_reason,
        "affected_subsystems": plan.affected_subsystems,
    })
}
