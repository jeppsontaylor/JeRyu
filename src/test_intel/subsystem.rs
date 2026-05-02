//! Owner: VTI Test Intelligence subsystem — subsystem ownership graph
//! Proof: `cargo nextest run -p jeryu -- test_intel::subsystem`
//! Invariants: Subsystem mappings stay deterministic and reflect the shared VTI contract.
//! Subsystem rules: maps source file paths to named subsystems and test commands.
//!
//! Each subsystem owns a set of source paths (via simple glob patterns), a nextest
//! filter expression for unit tests, a list of integration test binaries, and
//! a set of paths that force a full test run if changed.
//!
//! Uses a lightweight glob matcher (no external crate) since our patterns are
//! simple: `foo/*`, `foo/**`, `dir/**/*.ext`, and `*.ext`.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A named subsystem with its owned paths and test commands.
#[derive(Debug, Clone)]
pub struct Subsystem {
    pub id: &'static str,
    pub description: &'static str,
    /// Glob patterns for source files owned by this subsystem.
    pub owned_paths: &'static [&'static str],
    /// Nextest filter expression for unit tests.
    pub unit_filter: &'static str,
    /// Integration test binary names (from `tests/` directory).
    pub integration_tests: &'static [&'static str],
    /// If any of these paths change, force full test run.
    pub force_full_paths: &'static [&'static str],
    /// Runner tags required for this subsystem's tests.
    pub runner_tags: &'static [&'static str],
    /// Whether this subsystem is cross-cutting (changes affect many others).
    pub cross_cutting: bool,
}

/// Serializable representation for JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsystemInfo {
    pub id: String,
    pub description: String,
    pub owned_paths: Vec<String>,
    pub unit_filter: String,
    pub integration_tests: Vec<String>,
    pub cross_cutting: bool,
}

impl From<&Subsystem> for SubsystemInfo {
    fn from(s: &Subsystem) -> Self {
        Self {
            id: s.id.to_string(),
            description: s.description.to_string(),
            owned_paths: s.owned_paths.iter().map(|p| p.to_string()).collect(),
            unit_filter: s.unit_filter.to_string(),
            integration_tests: s.integration_tests.iter().map(|p| p.to_string()).collect(),
            cross_cutting: s.cross_cutting,
        }
    }
}

/// Paths that always trigger a full test run regardless of subsystem.
pub const GLOBAL_INVALIDATORS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "rust-toolchain",
    ".cargo/*",
    ".gitlab-ci.yml",
    ".github/workflows/*",
    "build.rs",
    "src/admission.rs",
    "src/policy.rs",
];

/// File patterns that indicate a docs-only change.
pub const DOCS_PATTERNS: &[&str] = &["*.md", "docs/*", "LICENSE", ".gitignore", ".editorconfig"];

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// The complete set of subsystem rules for the JeRyu jeryu codebase.
pub const SUBSYSTEMS: &[Subsystem] = &[
    Subsystem {
        id: "pool",
        description: "Runner pool management and Docker container lifecycle",
        owned_paths: &["src/pool.rs", "src/docker.rs"],
        unit_filter: "test(/pool|docker|runner/)",
        integration_tests: &["pool_tests", "job_tests"],
        force_full_paths: &[],
        runner_tags: &["build", "docker-build"],
        cross_cutting: false,
    },
    Subsystem {
        id: "cache",
        description: "SmartCache, gateway, taint, epoch, and witness subsystems",
        owned_paths: &[
            "src/cache.rs",
            "src/cache_brain.rs",
            "src/cache_proxy.rs",
            "src/gateway/**",
            "src/epoch.rs",
            "src/taint.rs",
            "src/witness.rs",
            "src/sccache_mgr.rs",
        ],
        unit_filter: "test(/cache|singleflight|gateway|taint|epoch|witness|sccache/)",
        integration_tests: &["cache_integration_test"],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "agent",
        description: "Autonomous agent flow and capability RPC",
        owned_paths: &["src/agent.rs", "src/capability.rs"],
        unit_filter: "test(/agent|capability|risk_gate/)",
        integration_tests: &["agent_tests"],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "engine",
        description: "Webhook receiver, reconciliation, push/pipeline/job handling",
        owned_paths: &["src/engine.rs"],
        unit_filter: "test(/webhook|pipeline|supersedence|reconcil/)",
        integration_tests: &["job_tests"],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "release",
        description: "Release promotion, canary, and secrets management",
        owned_paths: &["src/release.rs", "src/secrets.rs"],
        unit_filter: "test(/release|canary|secret|vault|promote/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "decision",
        description: "Failure classification, retry logic, risk gates, trust tiers",
        owned_paths: &["src/decision.rs", "src/capsule.rs"],
        unit_filter: "test(/decision|risk_gate|retry|classif|capsule/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "tui",
        description: "Terminal user interface",
        owned_paths: &["src/tui/**"],
        unit_filter: "test(/tui|snapshot|render|widget/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "state",
        description: "Postgres-primary state database, SQLite fallback, migrations, CRUD operations",
        owned_paths: &["src/state.rs"],
        unit_filter: "test(/state|sqlite|db|migrat/)",
        integration_tests: &[
            "pool_tests",
            "job_tests",
            "agent_tests",
            "cache_integration_test",
        ],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: true,
    },
    Subsystem {
        id: "config",
        description: "Configuration, templates, bootstrap",
        owned_paths: &["src/config.rs", "src/bootstrap.rs"],
        unit_filter: "test(/config|template|bootstrap/)",
        integration_tests: &["pool_tests"],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "impact",
        description: "Impact analysis and test runner",
        owned_paths: &["src/impact.rs", "src/test_runner.rs", "src/test_intel/**"],
        unit_filter: "test(/impact|test_run|test_intel|plan_from/)",
        integration_tests: &[],
        // Changes to the selector itself should trigger full testing
        // until we have nightly audit confirming correctness.
        force_full_paths: &["src/test_intel/**"],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "exec",
        description: "Custom executor, sandbox, honeypot",
        owned_paths: &["src/exec.rs", "src/sandbox.rs", "src/honeypot.rs"],
        unit_filter: "test(/exec|sandbox|honeypot|custom_exec/)",
        integration_tests: &["e2e"],
        force_full_paths: &[],
        runner_tags: &["build", "docker-build"],
        cross_cutting: false,
    },
    Subsystem {
        id: "gitlab_client",
        description: "GitLab REST API client",
        owned_paths: &["src/gitlab_client.rs"],
        unit_filter: "test(/gitlab|client|api|endpoint/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "shadow",
        description: "Shadow sync and telemetry",
        owned_paths: &["src/shadow.rs", "src/telemetry.rs", "src/logs.rs"],
        unit_filter: "test(/shadow|sync|telemetry|log/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "explain_mod",
        description: "Pipeline explain and buildkit",
        owned_paths: &["src/explain.rs", "src/buildkit.rs"],
        unit_filter: "test(/explain|buildkit/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
    Subsystem {
        id: "reclaim",
        description: "Disk reclaim and garbage collection",
        owned_paths: &["src/reclaim.rs"],
        unit_filter: "test(/reclaim|gc|garbage/)",
        integration_tests: &[],
        force_full_paths: &[],
        runner_tags: &["default", "rust", "test"],
        cross_cutting: false,
    },
];

// ---------------------------------------------------------------------------
// Lightweight glob matching (no external crate)
// ---------------------------------------------------------------------------

/// Check if a file path matches a simple glob pattern.
///
/// Supported patterns:
/// - `foo/bar` — exact match
/// - `foo/*` — matches any file under `foo/` (single level)
/// - `foo/**` — matches any file under `foo/` (recursive)
/// - `*.ext` — matches any file ending with `.ext`
/// - `Foo*` — matches any file starting with `Foo`
/// - `**/foo` — matches `foo` anywhere in the path
/// - `dir/**/*.ext` — matches `*.ext` recursively under `dir/`
pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern == path {
        return true;
    }

    // Pattern: `dir/*` → match anything directly under `dir/`
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return path.starts_with(prefix)
            && path.len() > prefix.len() + 1
            && !path[prefix.len() + 1..].contains('/');
    }

    // Pattern: `dir/**` → match anything recursively under `dir/`
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path.starts_with(prefix) && path.len() > prefix.len() + 1;
    }

    // Pattern: `dir/**/*.ext` → match `*.ext` recursively under `dir/`
    if pattern.contains("/**/") {
        let parts: Vec<&str> = pattern.splitn(2, "/**/").collect();
        if parts.len() == 2 {
            let dir_prefix = parts[0];
            let tail = parts[1];
            if !path.starts_with(dir_prefix) || path.len() <= dir_prefix.len() + 1 {
                return false;
            }
            let remainder = &path[dir_prefix.len() + 1..];
            // Tail is typically `*.ext` or a literal
            return glob_match(tail, remainder)
                || remainder
                    .rsplit('/')
                    .next()
                    .map(|basename| glob_match(tail, basename))
                    .unwrap_or(false);
        }
    }

    // Pattern: `*.ext` — match any file ending with `.ext`
    // Pattern: `Foo*` — match any file starting with `Foo`
    if let Some(pos) = pattern.find('*') {
        let prefix = &pattern[..pos];
        let suffix = &pattern[pos + 1..];
        // For basename-level patterns (no `/` in pattern), match against the
        // full path or the basename.
        if !pattern.contains('/') {
            let basename = path.rsplit('/').next().unwrap_or(path);
            return basename.starts_with(prefix) && basename.ends_with(suffix);
        }
        return path.starts_with(prefix) && path.ends_with(suffix);
    }

    // Pattern: `**/foo` → match `foo` as the last component
    if let Some(suffix) = pattern.strip_prefix("**/") {
        return path == suffix || path.ends_with(&format!("/{}", suffix));
    }

    false
}

/// Check if a path matches any of a set of patterns.
pub fn matches_any(path: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| glob_match(p, path))
}

/// Find all subsystems affected by a set of changed paths.
pub fn affected_subsystems(changed_paths: &[String]) -> Vec<&'static Subsystem> {
    let mut affected = Vec::new();
    for subsystem in SUBSYSTEMS {
        let owns_changed = changed_paths
            .iter()
            .any(|p| matches_any(p, subsystem.owned_paths));
        if owns_changed {
            affected.push(subsystem);
        }
    }
    affected
}

/// Check if any changed path is a global invalidator.
pub fn has_global_invalidator(changed_paths: &[String]) -> Option<String> {
    for path in changed_paths {
        if matches_any(path, GLOBAL_INVALIDATORS) {
            return Some(path.clone());
        }
    }
    None
}

/// Check if all changed paths are documentation-only.
pub fn is_docs_only(changed_paths: &[String]) -> bool {
    !changed_paths.is_empty() && changed_paths.iter().all(|p| matches_any(p, DOCS_PATTERNS))
}

/// Check if any affected subsystem has force_full_paths that match the changes.
pub fn has_subsystem_force_full(
    changed_paths: &[String],
    affected: &[&Subsystem],
) -> Option<String> {
    for subsystem in affected {
        if subsystem.force_full_paths.is_empty() {
            continue;
        }
        for path in changed_paths {
            if matches_any(path, subsystem.force_full_paths) {
                return Some(format!(
                    "subsystem '{}' force-full trigger: {}",
                    subsystem.id, path
                ));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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
        // Should not match the directory itself
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
        let reason =
            has_subsystem_force_full(&["src/test_intel/planner.rs".to_string()], &affected);
        assert!(reason.is_some());
    }

    #[test]
    fn glob_dir_star_rejects_deep_paths() {
        // dir/* should only match single-level children
        assert!(glob_match("src/gateway/*", "src/gateway/handler.rs"));
        assert!(!glob_match("src/gateway/*", "src/gateway/sub/deep.rs"));
    }

    #[test]
    fn glob_dir_doublestar_matches_deep_paths() {
        // dir/** should match recursively
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
        // Foo* should match files starting with Foo
        assert!(glob_match("Dockerfile*", "Dockerfile"));
        assert!(glob_match("Dockerfile*", "Dockerfile.enclave"));
        assert!(glob_match("Dockerfile*", "Dockerfile.prod"));
        assert!(!glob_match("Dockerfile*", "not-a-dockerfile"));
        // requirements*.txt
        assert!(glob_match("requirements*.txt", "requirements.txt"));
        assert!(glob_match("requirements*.txt", "requirements-dev.txt"));
        assert!(!glob_match("requirements*.txt", "setup.py"));
    }

    #[test]
    fn glob_compound_doublestar_ext() {
        // dir/**/*.ext should match *.ext recursively under dir/
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
        // *.md should match .md files at any depth
        assert!(glob_match("*.md", "README.md"));
        assert!(glob_match("*.md", "docs/ARCHITECTURE.md"));
        assert!(!glob_match("*.md", "src/main.rs"));
    }
}
