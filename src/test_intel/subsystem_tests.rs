use super::*;

#[test]
fn glob_exact_match() {
    assert!(glob_match("src/pool.rs", "src/pool.rs"));
    assert!(!glob_match("src/pool.rs", "src/docker.rs"));
}

#[test]
fn glob_dir_star() {
    assert!(glob_match("src/gateway/*", "src/gateway/handler.rs"));
    assert!(glob_match("src/tui/*", "src/tui/ui.rs"));
    assert!(!glob_match("src/gateway/*", "src/pool.rs"));
    assert!(!glob_match("src/gateway/*", "src/gateway"));
}

#[test]
fn glob_extension() {
    assert!(glob_match("*.md", "README.md"));
    assert!(glob_match("*.md", "docs/ARCHITECTURE.md"));
    assert!(!glob_match("*.md", "src/main.rs"));
}

#[test]
fn pool_rs_matches_pool_subsystem() {
    let affected = affected_subsystems(&["src/pool.rs".to_string()]);
    assert_eq!(affected.len(), 1);
    assert_eq!(affected[0].id, "pool");
}

#[test]
fn state_rs_is_cross_cutting() {
    let affected = affected_subsystems(&["src/state.rs".to_string()]);
    assert_eq!(affected.len(), 1);
    assert!(affected[0].cross_cutting);
    assert!(affected[0].integration_tests.len() > 1);
}

#[test]
fn tui_matches_tui_subsystem() {
    let affected = affected_subsystems(&["src/tui/ui.rs".to_string()]);
    assert_eq!(affected.len(), 1);
    assert_eq!(affected[0].id, "tui");
}

#[test]
fn nested_tui_file_matches_tui_subsystem() {
    let affected = affected_subsystems(&["src/tui/flow/widget.rs".to_string()]);
    assert_eq!(affected.len(), 1);
    assert_eq!(affected[0].id, "tui");
}

#[test]
fn cargo_toml_is_global_invalidator() {
    assert!(has_global_invalidator(&["Cargo.toml".to_string()]).is_some());
}

#[test]
fn readme_is_docs_only() {
    assert!(is_docs_only(&["README.md".to_string()]));
}

#[test]
fn readme_plus_code_is_not_docs_only() {
    assert!(!is_docs_only(&[
        "README.md".to_string(),
        "src/pool.rs".to_string(),
    ]));
}

#[test]
fn multiple_subsystems_affected() {
    let affected =
        affected_subsystems(&["src/pool.rs".to_string(), "src/decision.rs".to_string()]);
    let ids: Vec<_> = affected.iter().map(|s| s.id).collect();
    assert!(ids.contains(&"pool"));
    assert!(ids.contains(&"decision"));
}

#[test]
fn unknown_file_matches_no_subsystem() {
    let affected = affected_subsystems(&["some/random/file.txt".to_string()]);
    assert!(affected.is_empty());
}

#[test]
fn gateway_glob_matches_cache_subsystem() {
    let affected = affected_subsystems(&["src/gateway/handler.rs".to_string()]);
    assert_eq!(affected.len(), 1);
    assert_eq!(affected[0].id, "cache");
}

#[test]
fn nested_gateway_file_matches_cache_subsystem() {
    let affected = affected_subsystems(&["src/gateway/npm/lockfile.rs".to_string()]);
    assert_eq!(affected.len(), 1);
    assert_eq!(affected[0].id, "cache");
}

#[test]
fn test_intel_force_full() {
    let affected = affected_subsystems(&["src/test_intel/planner.rs".to_string()]);
    assert!(!affected.is_empty());
    let reason = has_subsystem_force_full(&["src/test_intel/planner.rs".to_string()], &affected);
    assert!(reason.is_some());
}

#[test]
fn glob_dir_star_rejects_deep_paths() {
    assert!(glob_match("src/gateway/*", "src/gateway/handler.rs"));
    assert!(!glob_match("src/gateway/*", "src/gateway/sub/deep.rs"));
}

#[test]
fn glob_dir_doublestar_matches_deep_paths() {
    assert!(glob_match(
        "apps/veox-bootstrap/**",
        "apps/veox-bootstrap/src/lib.rs"
    ));
    assert!(glob_match(
        "apps/veox-bootstrap/**",
        "apps/veox-bootstrap/src/deep/nested/file.rs"
    ));
    assert!(!glob_match(
        "apps/veox-bootstrap/**",
        "apps/veox-deploy/src/lib.rs"
    ));
}

#[test]
fn glob_suffix_star_works() {
    assert!(glob_match("Dockerfile*", "Dockerfile"));
    assert!(glob_match("Dockerfile*", "Dockerfile.enclave"));
    assert!(glob_match("Dockerfile*", "Dockerfile.prod"));
    assert!(!glob_match("Dockerfile*", "not-a-dockerfile"));
    assert!(glob_match("requirements*.txt", "requirements.txt"));
    assert!(glob_match("requirements*.txt", "requirements-dev.txt"));
    assert!(!glob_match("requirements*.txt", "setup.py"));
}

#[test]
fn glob_compound_doublestar_ext() {
    assert!(glob_match(
        "ops/agent-workflows/**/*.md",
        "ops/agent-workflows/ci/release.md"
    ));
    assert!(glob_match(
        "ops/agent-workflows/**/*.md",
        "ops/agent-workflows/FLOW.md"
    ));
    assert!(!glob_match(
        "ops/agent-workflows/**/*.md",
        "ops/agent-workflows/ci/script.sh"
    ));
    assert!(!glob_match(
        "ops/agent-workflows/**/*.md",
        "ops/scripts/readme.md"
    ));
}

#[test]
fn glob_star_extension_matches_in_subdirs() {
    assert!(glob_match("*.md", "README.md"));
    assert!(glob_match("*.md", "docs/ARCHITECTURE.md"));
    assert!(!glob_match("*.md", "src/main.rs"));
}
