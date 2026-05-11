use super::*;

#[test]
fn empty_diff_produces_full_plan() {
    let plan = plan_tests(&[]);
    assert_eq!(plan.mode, TestPlanMode::Full);
    assert!(plan.repair_reason.is_some());
}

#[test]
fn cargo_toml_produces_full_plan() {
    let plan = plan_tests(&["Cargo.toml".to_string()]);
    assert_eq!(plan.mode, TestPlanMode::Full);
    assert!(
        plan.repair_reason
            .as_ref()
            .unwrap()
            .contains("global invalidator")
    );
}

#[test]
fn readme_produces_docs_plan() {
    let plan = plan_tests(&["README.md".to_string()]);
    assert_eq!(plan.mode, TestPlanMode::DocsOnly);
    assert!(plan.selected_tests.is_empty());
}

#[test]
fn pool_change_selects_pool_tests_only() {
    let plan = plan_tests(&["src/pool.rs".to_string()]);
    assert_eq!(plan.mode, TestPlanMode::Selected);
    assert!(plan.affected_subsystems.contains(&"pool".to_string()));
    assert!(plan.skipped_subsystems.contains(&"cache".to_string()));
    assert!(plan.skipped_subsystems.contains(&"tui".to_string()));
    assert!(plan.skipped_subsystems.contains(&"release".to_string()));

    let has_unit = plan
        .selected_tests
        .iter()
        .any(|t| t.kind == "unit_filter" && t.subsystem == "pool");
    let has_integration = plan
        .selected_tests
        .iter()
        .any(|t| t.kind == "integration" && t.command.contains("pool_tests"));
    assert!(has_unit, "should have pool unit filter");
    assert!(has_integration, "should have pool_tests integration");
}

#[test]
fn tui_change_skips_pool_and_e2e() {
    let plan = plan_tests(&["src/tui/ui.rs".to_string()]);
    assert_eq!(plan.mode, TestPlanMode::Selected);
    assert!(plan.affected_subsystems.contains(&"tui".to_string()));
    assert!(plan.skipped_subsystems.contains(&"pool".to_string()));
    assert!(plan.skipped_subsystems.contains(&"exec".to_string()));
    assert!(plan.selected_tests.iter().all(|t| t.kind != "integration"));
}

#[test]
fn state_change_includes_cross_cutting_integration() {
    let plan = plan_tests(&["src/state.rs".to_string()]);
    assert_eq!(plan.mode, TestPlanMode::Selected);
    assert!(plan.affected_subsystems.contains(&"state".to_string()));
    let integration_count = plan
        .selected_tests
        .iter()
        .filter(|t| t.kind == "integration")
        .count();
    assert!(
        integration_count >= 3,
        "state should include at least 3 integration suites, got {}",
        integration_count
    );
}

#[test]
fn changed_test_file_always_included() {
    let plan = plan_tests(&["tests/pool_tests.rs".to_string()]);
    assert_eq!(
        plan.mode,
        TestPlanMode::Selected,
        "pure test file change should produce Selected, not Full"
    );
    let has_test = plan
        .selected_tests
        .iter()
        .any(|t| t.command.contains("pool_tests"));
    assert!(has_test, "changed test file should always be included");
}

#[test]
fn multiple_subsystem_change_has_lower_confidence() {
    let single = plan_tests(&["src/pool.rs".to_string()]);
    let multi = plan_tests(&[
        "src/pool.rs".to_string(),
        "src/cache.rs".to_string(),
        "src/release.rs".to_string(),
        "src/agent.rs".to_string(),
    ]);
    assert!(
        multi.confidence <= single.confidence,
        "multi-subsystem confidence {} should be <= single {}",
        multi.confidence,
        single.confidence
    );
}

#[test]
fn unknown_file_triggers_conservative_repair() {
    let plan = plan_tests(&["unknown/file.txt".to_string()]);
    assert_eq!(plan.mode, TestPlanMode::Full);
    assert!(plan.repair_reason.unwrap().contains("no subsystem"));
}

#[test]
fn test_intel_change_triggers_force_full() {
    let plan = plan_tests(&["src/test_intel/planner.rs".to_string()]);
    assert_eq!(plan.mode, TestPlanMode::Full);
}

#[test]
fn docs_plus_code_not_docs_only() {
    let plan = plan_tests(&["README.md".to_string(), "src/pool.rs".to_string()]);
    assert_ne!(plan.mode, TestPlanMode::DocsOnly);
    assert_eq!(plan.mode, TestPlanMode::Selected);
}

#[test]
fn confidence_high_for_single_subsystem() {
    let plan = plan_tests(&["src/decision.rs".to_string()]);
    assert!(
        plan.confidence >= 0.90,
        "single well-matched subsystem should have high confidence, got {}",
        plan.confidence
    );
}

#[test]
fn vti_receipt_binds_selected_plan_to_head_sha() {
    let plan = plan_tests(&["src/pool.rs".to_string()]);
    let receipt = plan.receipt(Some("base"), Some("head"));
    assert_eq!(receipt.policy_version, "vti-receipt-v3.01");
    assert_eq!(receipt.head_sha.as_deref(), Some("head"));
    assert!(receipt.skipped_tests_explained);
    assert!(receipt.receipt_id.starts_with("vti-"));
}

#[test]
fn vti_receipt_marks_conservative_repair() {
    let plan = plan_tests(&["unknown/file.txt".to_string()]);
    let receipt = plan.receipt(None, Some("head"));
    assert_eq!(receipt.mode, TestPlanMode::Full);
    assert!(receipt.conservative_repair);
    assert!(receipt.repair_reason.is_some());
}
