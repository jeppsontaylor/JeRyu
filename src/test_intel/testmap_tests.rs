use super::*;

fn test_map() -> TestMap {
    toml::from_str(
        r#"
[policy]
full_on_unknown = true
min_confidence = 0.85

[global_invalidators]
paths = ["Cargo.lock", "Cargo.toml", ".gitlab-ci.yml"]

[docs]
paths = ["*.md", "docs/**"]

[[subsystem]]
id = "app-core"
description = "Core application"
paths = ["apps/core/**"]
ci_jobs = ["compile", "test-core"]
cross_cutting = false

[[subsystem]]
id = "app-deploy"
description = "Deploy tooling"
paths = ["apps/deploy/**"]
ci_jobs = ["compile", "test-deploy"]
cross_cutting = false

[[subsystem]]
id = "shared-lib"
description = "Shared library"
paths = ["crates/shared/**"]
ci_jobs = ["compile", "test-core", "test-deploy"]
cross_cutting = true
"#,
    )
    .unwrap()
}

#[test]
fn empty_diff_is_full() {
    let m = test_map();
    let plan = plan_from_testmap(&m, &[]);
    assert_eq!(plan.mode, ExternalPlanMode::Full);
}

#[test]
fn global_invalidator_triggers_full() {
    let m = test_map();
    let plan = plan_from_testmap(&m, &["Cargo.lock".into()]);
    assert_eq!(plan.mode, ExternalPlanMode::Full);
    assert!(plan.rationale[0].contains("global invalidator"));
}

#[test]
fn docs_only_change() {
    let m = test_map();
    let plan = plan_from_testmap(&m, &["README.md".into(), "docs/setup.md".into()]);
    assert_eq!(plan.mode, ExternalPlanMode::DocsOnly);
}

#[test]
fn single_subsystem_selects_jobs() {
    let m = test_map();
    let plan = plan_from_testmap(&m, &["apps/core/main.rs".into()]);
    assert_eq!(plan.mode, ExternalPlanMode::Selected);
    assert!(plan.selected_jobs.contains(&"test-core".into()));
    assert!(!plan.selected_jobs.contains(&"test-deploy".into()));
    assert_eq!(plan.affected_subsystems, vec!["app-core"]);
}

#[test]
fn multiple_subsystems_union_jobs() {
    let m = test_map();
    let plan = plan_from_testmap(
        &m,
        &["apps/core/main.rs".into(), "apps/deploy/run.rs".into()],
    );
    assert_eq!(plan.mode, ExternalPlanMode::Selected);
    assert!(plan.selected_jobs.contains(&"test-core".into()));
    assert!(plan.selected_jobs.contains(&"test-deploy".into()));
}

#[test]
fn deploy_core_changes_select_release_artifact_chain() {
    let m: TestMap = toml::from_str(
        r#"
[policy]
full_on_unknown = true
min_confidence = 0.85

[global_invalidators]
paths = [".jeryu/**", "ci/gitlab/**"]

[[subsystem]]
id = "deploy-crates"
paths = ["crates/deploy-core/**", "crates/deploy-manifest/**", "crates/deploy-doctor/**"]
ci_jobs = [
    "compile-workspace",
    "test-rust-veox-deploy",
    "build-release-artifacts",
    "build-bootstrap-musl",
    "build-enclave-server",
    "test-live-public-surface",
    "test-local-built",
    "publish-rc-dry-run",
    "test-local-rc",
]
"#,
    )
    .unwrap();
    let plan = plan_from_testmap(&m, &["crates/deploy-core/src/docker.rs".into()]);
    assert_eq!(plan.mode, ExternalPlanMode::Selected);
    for job in [
        "build-release-artifacts",
        "build-bootstrap-musl",
        "build-enclave-server",
        "test-live-public-surface",
        "test-local-built",
        "publish-rc-dry-run",
        "test-local-rc",
    ] {
        assert!(
            plan.selected_jobs.contains(&job.to_string()),
            "missing selected job {job}"
        );
    }
}

#[test]
fn cross_cutting_lowers_confidence() {
    let m = test_map();
    let plan = plan_from_testmap(&m, &["crates/shared/lib.rs".into()]);
    assert_eq!(plan.mode, ExternalPlanMode::Selected);
    assert!(plan.confidence < 1.0, "should be < 1.0 for cross-cutting");
    assert_eq!(plan.confidence, 0.90);
}

#[test]
fn unknown_file_triggers_full() {
    let m = test_map();
    let plan = plan_from_testmap(&m, &["some/unknown/path.rs".into()]);
    assert_eq!(plan.mode, ExternalPlanMode::Full);
    assert!(plan.rationale.iter().any(|r| r.contains("unmatched")));
}

#[test]
fn docs_mixed_with_code_not_docs_only() {
    let m = test_map();
    let plan = plan_from_testmap(&m, &["README.md".into(), "apps/core/main.rs".into()]);
    assert_eq!(plan.mode, ExternalPlanMode::Selected);
    assert!(plan.selected_jobs.contains(&"test-core".into()));
}

#[test]
fn yaml_generation_selected() {
    let m = test_map();
    let plan = plan_from_testmap(&m, &["apps/core/main.rs".into()]);
    let yaml = emit_external_gitlab_yaml(&plan, None);
    assert!(yaml.contains("compile:"));
    assert!(yaml.contains("test-core:"));
    assert!(yaml.contains("VTI_FORCE_SELECTED_GRAPH"));
    assert!(yaml.contains("ci-job test-core"));
}

#[test]
fn yaml_generation_docs() {
    let m = test_map();
    let plan = plan_from_testmap(&m, &["README.md".into()]);
    let yaml = emit_external_gitlab_yaml(&plan, None);
    assert!(yaml.contains("vti-noop:"));
    assert!(yaml.contains("docs-only"));
}

#[test]
fn explain_json_roundtrips() {
    let m = test_map();
    let plan = plan_from_testmap(&m, &["apps/core/main.rs".into()]);
    let json = explain_external_json(&plan);
    assert_eq!(json["mode"], "selected");
    assert!(!json["selected_jobs"].as_array().unwrap().is_empty());
}

#[test]
fn unknown_not_full_when_policy_allows() {
    let m: TestMap = toml::from_str(
        r#"
[policy]
full_on_unknown = false
min_confidence = 0.50

[global_invalidators]
paths = ["Cargo.lock"]

[[subsystem]]
id = "app"
paths = ["src/**"]
ci_jobs = ["test"]
"#,
    )
    .unwrap();
    // With full_on_unknown = false, unmatched paths don't trigger full
    let plan = plan_from_testmap(&m, &["unknown/path.rs".into()]);
    // No subsystem matched, and no full-on-unknown, so selected with 0 jobs
    // But with unmatched penalty it might dip below confidence...
    // Actually with no matched subsystems and some unmatched, confidence = 1.0 - 0.15 = 0.85
    // min_confidence = 0.50, so this stays at Selected with 0 jobs
    assert_ne!(plan.mode, ExternalPlanMode::Full);
}
