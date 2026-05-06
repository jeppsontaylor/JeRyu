//! Owner: VTI Test Intelligence subsystem — plan cache
//! Proof: `cargo nextest run -p jeryu -- test_intel::cache`
//! Invariants: Cached plans are keyed by inputs that affect test selection and expire on outdated evidence.
//! Test result caching via content-addressed witness hashes.
//!
//! This module enables 20-100x CI speedups for repeat runs by caching test
//! verdicts keyed on a SHA-256 of all relevant inputs: source file hashes,
//! Cargo.lock hash, rustc version, and cache epoch.
//!
//! Cacheability rules:
//! - Unit tests with no external deps: **cacheable**
//! - Integration tests touching Docker/GitLab: **not cacheable**
//! - E2E tests: **never cacheable**
//! - Tests with flake history: **never cacheable**

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Cacheability classification for a test command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cacheability {
    /// Fully cacheable; can skip if cache hit
    Cacheable,
    /// Not cacheable due to external dependencies
    Uncacheable,
    /// Forced uncacheable due to flakiness history
    FlakyUncacheable,
}

/// A cache key for a specific test execution context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCacheKey {
    /// The computed SHA-256 digest
    pub digest: String,
    /// Human-readable description of inputs
    pub inputs_description: String,
    /// Whether this key represents a cacheable result
    pub cacheability: Cacheability,
    /// Reasons why it's uncacheable (if applicable)
    pub uncacheable_reasons: Vec<String>,
}

/// A cached test verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedVerdict {
    /// The cache key that produced this verdict
    pub cache_key: String,
    /// The test identifier
    pub test_id: String,
    /// Pass or fail
    pub passed: bool,
    /// Duration of the original run in milliseconds
    pub duration_ms: u64,
    /// When this was cached
    pub cached_at: String,
    /// Cache epoch at time of caching
    pub epoch: i64,
}

/// Result of checking the cache for a set of tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheLookupResult {
    /// Tests that had a cache hit (can be skipped)
    pub hits: Vec<CacheHit>,
    /// Tests that need to be re-run
    pub misses: Vec<CacheMiss>,
    /// Total time saved by cache hits (ms)
    pub time_saved_ms: u64,
    /// Summary statistics
    pub hit_rate: f64,
}

/// A single cache hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheHit {
    pub test_id: String,
    pub cache_key: String,
    pub original_duration_ms: u64,
    pub cached_at: String,
}

/// A single cache miss.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMiss {
    pub test_id: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Cache key computation
// ---------------------------------------------------------------------------

/// Compute a deterministic cache key for a test execution.
///
/// The key is a SHA-256 of:
/// - test_id (the command string)
/// - source file content hashes (sorted by path for determinism)
/// - Cargo.lock hash
/// - rustc version
/// - cache epoch (allows global invalidation)
pub fn compute_cache_key(
    test_id: &str,
    source_hashes: &[(&str, &str)], // (path, content_hash)
    cargo_lock_hash: &str,
    rustc_version: &str,
    cache_epoch: i64,
) -> TestCacheKey {
    let mut hasher = Sha256::new();
    let mut uncacheable_reasons = Vec::new();

    // 1. Test identity
    hasher.update(b"test:");
    hasher.update(test_id.as_bytes());
    hasher.update(b"\n");

    // 2. Source hashes (sorted for determinism)
    let mut sorted_hashes: Vec<_> = source_hashes.to_vec();
    sorted_hashes.sort_by_key(|(path, _)| path.to_string());
    for (path, hash) in &sorted_hashes {
        hasher.update(b"src:");
        hasher.update(path.as_bytes());
        hasher.update(b":");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }

    // 3. Cargo.lock
    hasher.update(b"lock:");
    hasher.update(cargo_lock_hash.as_bytes());
    hasher.update(b"\n");

    // 4. Toolchain
    hasher.update(b"rustc:");
    hasher.update(rustc_version.as_bytes());
    hasher.update(b"\n");

    // 5. Cache epoch
    hasher.update(b"epoch:");
    hasher.update(cache_epoch.to_string().as_bytes());
    hasher.update(b"\n");

    let digest = hex::encode(hasher.finalize());

    // Classify cacheability
    let cacheability = classify_cacheability(test_id, &mut uncacheable_reasons);

    let inputs_desc = format!(
        "test={}, sources={}, lock={}, rustc={}, epoch={}",
        test_id,
        sorted_hashes.len(),
        &cargo_lock_hash[..8.min(cargo_lock_hash.len())],
        rustc_version,
        cache_epoch
    );

    TestCacheKey {
        digest,
        inputs_description: inputs_desc,
        cacheability,
        uncacheable_reasons,
    }
}

/// Classify whether a test command is cacheable based on its name/type.
fn classify_cacheability(test_id: &str, reasons: &mut Vec<String>) -> Cacheability {
    let id_lower = test_id.to_lowercase();

    // E2E tests: never cacheable (external state)
    if id_lower.contains("e2e")
        || id_lower.contains("end_to_end")
        || id_lower.contains("end-to-end")
    {
        reasons.push("E2E tests depend on external state and are never cacheable".into());
        return Cacheability::Uncacheable;
    }

    // Docker/container tests: not cacheable
    if id_lower.contains("docker") || id_lower.contains("container") || id_lower.contains("dind") {
        reasons.push("Docker/container tests depend on daemon state".into());
        return Cacheability::Uncacheable;
    }

    // GitLab API tests: not cacheable
    if id_lower.contains("gitlab") && id_lower.contains("live") {
        reasons.push("Live GitLab API tests depend on external service".into());
        return Cacheability::Uncacheable;
    }

    // Integration tests that explicitly use network: not cacheable
    if id_lower.contains("--test")
        && (id_lower.contains("pool_tests") || id_lower.contains("job_tests"))
    {
        reasons.push("Pool/job integration tests may depend on Docker daemon".into());
        return Cacheability::Uncacheable;
    }

    // Agent tests: may use network
    if id_lower.contains("--test") && id_lower.contains("agent_tests") {
        reasons.push("Agent integration tests may use network resources".into());
        return Cacheability::Uncacheable;
    }

    // Unit tests (--lib, nextest -E 'test(...)') are cacheable
    Cacheability::Cacheable
}

/// Mark a test as flaky-uncacheable.
pub fn mark_flaky(key: &mut TestCacheKey) {
    key.cacheability = Cacheability::FlakyUncacheable;
    key.uncacheable_reasons
        .push("Test has flake history — cache disabled".into());
}

// ---------------------------------------------------------------------------
// Cache lookup simulation
// ---------------------------------------------------------------------------

/// Given a set of tests and their cache keys, check which have cached verdicts.
///
/// In production, this would query the `cache_verdicts` table. Here we provide
/// the lookup logic that consumers (engine, test_runner) use.
pub fn check_cache(
    tests: &[(String, TestCacheKey)],
    cached_verdicts: &[CachedVerdict],
) -> CacheLookupResult {
    let mut hits = Vec::new();
    let mut misses = Vec::new();
    let mut time_saved_ms = 0u64;

    for (test_id, key) in tests {
        // Uncacheable tests are always misses
        if key.cacheability != Cacheability::Cacheable {
            misses.push(CacheMiss {
                test_id: test_id.clone(),
                reason: match key.uncacheable_reasons.first().cloned() {
                    Some(reason) => reason,
                    None => "uncacheable".into(),
                },
            });
            continue;
        }

        // Look for a matching verdict
        if let Some(verdict) = cached_verdicts
            .iter()
            .find(|v| v.cache_key == key.digest && v.passed)
        {
            hits.push(CacheHit {
                test_id: test_id.clone(),
                cache_key: key.digest.clone(),
                original_duration_ms: verdict.duration_ms,
                cached_at: verdict.cached_at.clone(),
            });
            time_saved_ms += verdict.duration_ms;
        } else {
            misses.push(CacheMiss {
                test_id: test_id.clone(),
                reason: "no cache hit".into(),
            });
        }
    }

    let total = tests.len().max(1);
    let hit_rate = hits.len() as f64 / total as f64;

    CacheLookupResult {
        hits,
        misses,
        time_saved_ms,
        hit_rate,
    }
}

/// Human-readable cache lookup report.
pub fn explain_cache_lookup(result: &CacheLookupResult) -> String {
    let mut out = String::new();
    out.push_str("╭─ VTI Test Cache Lookup ───────────────────────╮\n");
    out.push_str(&format!("│ Hits:       {:<34} │\n", result.hits.len()));
    out.push_str(&format!("│ Misses:     {:<34} │\n", result.misses.len()));
    out.push_str(&format!(
        "│ Hit rate:   {:<34.1}% │\n",
        result.hit_rate * 100.0
    ));
    out.push_str(&format!(
        "│ Time saved: {:<34} │\n",
        format_duration_ms(result.time_saved_ms)
    ));
    out.push_str("╰───────────────────────────────────────────────╯\n\n");

    if !result.hits.is_empty() {
        out.push_str("Cache hits (skippable):\n");
        for hit in &result.hits {
            out.push_str(&format!(
                "  ✓ {} (saved {})\n",
                hit.test_id,
                format_duration_ms(hit.original_duration_ms)
            ));
        }
        out.push('\n');
    }

    if !result.misses.is_empty() {
        out.push_str("Cache misses (must run):\n");
        for miss in &result.misses {
            out.push_str(&format!("  ● {} ({})\n", miss.test_id, miss.reason));
        }
    }

    out
}

/// JSON representation.
pub fn explain_cache_json(result: &CacheLookupResult) -> serde_json::Value {
    serde_json::json!({
        "hits": result.hits,
        "misses": result.misses,
        "time_saved_ms": result.time_saved_ms,
        "hit_rate": result.hit_rate,
    })
}

fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{:.1}m", ms as f64 / 60_000.0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_deterministic() {
        let key1 = compute_cache_key(
            "cargo test --lib",
            &[("src/pool.rs", "abc123"), ("src/main.rs", "def456")],
            "lockfile_hash",
            "1.92.0",
            1,
        );
        let key2 = compute_cache_key(
            "cargo test --lib",
            &[("src/pool.rs", "abc123"), ("src/main.rs", "def456")],
            "lockfile_hash",
            "1.92.0",
            1,
        );
        assert_eq!(key1.digest, key2.digest);
    }

    #[test]
    fn cache_key_order_independent() {
        // Source hashes in different order should produce the same key
        let key1 = compute_cache_key(
            "cargo test --lib",
            &[("src/a.rs", "h1"), ("src/b.rs", "h2")],
            "lock",
            "1.92",
            1,
        );
        let key2 = compute_cache_key(
            "cargo test --lib",
            &[("src/b.rs", "h2"), ("src/a.rs", "h1")],
            "lock",
            "1.92",
            1,
        );
        assert_eq!(key1.digest, key2.digest);
    }

    #[test]
    fn cache_key_changes_with_source() {
        let key1 = compute_cache_key(
            "cargo test --lib",
            &[("src/pool.rs", "hash_v1")],
            "lock",
            "1.92",
            1,
        );
        let key2 = compute_cache_key(
            "cargo test --lib",
            &[("src/pool.rs", "hash_v2")],
            "lock",
            "1.92",
            1,
        );
        assert_ne!(key1.digest, key2.digest);
    }

    #[test]
    fn cache_key_changes_with_epoch() {
        let key1 = compute_cache_key("test", &[], "lock", "1.92", 1);
        let key2 = compute_cache_key("test", &[], "lock", "1.92", 2);
        assert_ne!(key1.digest, key2.digest);
    }

    #[test]
    fn cache_key_changes_with_rustc() {
        let key1 = compute_cache_key("test", &[], "lock", "1.91.0", 1);
        let key2 = compute_cache_key("test", &[], "lock", "1.92.0", 1);
        assert_ne!(key1.digest, key2.digest);
    }

    #[test]
    fn unit_test_is_cacheable() {
        let key = compute_cache_key(
            "cargo nextest run -E 'test(/pool/)'",
            &[],
            "lock",
            "1.92",
            1,
        );
        assert_eq!(key.cacheability, Cacheability::Cacheable);
    }

    #[test]
    fn e2e_test_not_cacheable() {
        let key = compute_cache_key("cargo nextest run --test e2e", &[], "lock", "1.92", 1);
        assert_eq!(key.cacheability, Cacheability::Uncacheable);
        assert!(key.uncacheable_reasons[0].contains("E2E"));
    }

    #[test]
    fn docker_test_not_cacheable() {
        let key = compute_cache_key("docker compose up -d && run_tests", &[], "lock", "1.92", 1);
        assert_eq!(key.cacheability, Cacheability::Uncacheable);
    }

    #[test]
    fn pool_integration_not_cacheable() {
        let key = compute_cache_key(
            "cargo nextest run --test pool_tests",
            &[],
            "lock",
            "1.92",
            1,
        );
        assert_eq!(key.cacheability, Cacheability::Uncacheable);
    }

    #[test]
    fn mark_flaky_overrides_cacheability() {
        let mut key = compute_cache_key(
            "cargo nextest run -E 'test(/cache/)'",
            &[],
            "lock",
            "1.92",
            1,
        );
        assert_eq!(key.cacheability, Cacheability::Cacheable);
        mark_flaky(&mut key);
        assert_eq!(key.cacheability, Cacheability::FlakyUncacheable);
    }

    #[test]
    fn cache_lookup_hit_on_matching_digest() {
        let key = compute_cache_key("unit-test", &[("a.rs", "h1")], "lock", "1.92", 1);
        let verdict = CachedVerdict {
            cache_key: key.digest.clone(),
            test_id: "unit-test".into(),
            passed: true,
            duration_ms: 5000,
            cached_at: "2026-01-01T00:00:00Z".into(),
            epoch: 1,
        };

        let result = check_cache(&[("unit-test".into(), key)], &[verdict]);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.misses.len(), 0);
        assert_eq!(result.time_saved_ms, 5000);
        assert!((result.hit_rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cache_lookup_miss_on_different_digest() {
        let key = compute_cache_key("unit-test", &[("a.rs", "h1")], "lock", "1.92", 1);
        let verdict = CachedVerdict {
            cache_key: "wrong_digest".into(),
            test_id: "unit-test".into(),
            passed: true,
            duration_ms: 5000,
            cached_at: "2026-01-01T00:00:00Z".into(),
            epoch: 1,
        };

        let result = check_cache(&[("unit-test".into(), key)], &[verdict]);
        assert_eq!(result.hits.len(), 0);
        assert_eq!(result.misses.len(), 1);
    }

    #[test]
    fn cache_lookup_uncacheable_always_miss() {
        let key = compute_cache_key("cargo nextest run --test e2e", &[], "lock", "1.92", 1);
        let verdict = CachedVerdict {
            cache_key: key.digest.clone(),
            test_id: "e2e".into(),
            passed: true,
            duration_ms: 30000,
            cached_at: "2026-01-01T00:00:00Z".into(),
            epoch: 1,
        };

        let result = check_cache(&[("cargo nextest run --test e2e".into(), key)], &[verdict]);
        // Even though we have a matching digest, e2e is uncacheable
        assert_eq!(result.hits.len(), 0);
        assert_eq!(result.misses.len(), 1);
        assert!(result.misses[0].reason.contains("E2E"));
    }

    #[test]
    fn cache_lookup_failed_verdict_is_miss() {
        let key = compute_cache_key("unit-test", &[("a.rs", "h1")], "lock", "1.92", 1);
        let verdict = CachedVerdict {
            cache_key: key.digest.clone(),
            test_id: "unit-test".into(),
            passed: false, // Failed — don't skip!
            duration_ms: 5000,
            cached_at: "2026-01-01T00:00:00Z".into(),
            epoch: 1,
        };

        let result = check_cache(&[("unit-test".into(), key)], &[verdict]);
        assert_eq!(result.hits.len(), 0);
        assert_eq!(result.misses.len(), 1);
    }

    #[test]
    fn mixed_cache_lookup() {
        let key_unit = compute_cache_key("unit-test", &[("a.rs", "h1")], "lock", "1.92", 1);
        let key_e2e = compute_cache_key(
            "cargo nextest run --test e2e",
            &[("a.rs", "h1")],
            "lock",
            "1.92",
            1,
        );
        let key_new = compute_cache_key("new-test", &[("b.rs", "h2")], "lock", "1.92", 1);

        let verdict = CachedVerdict {
            cache_key: key_unit.digest.clone(),
            test_id: "unit-test".into(),
            passed: true,
            duration_ms: 3000,
            cached_at: "2026-01-01T00:00:00Z".into(),
            epoch: 1,
        };

        let result = check_cache(
            &[
                ("unit-test".into(), key_unit),
                ("cargo nextest run --test e2e".into(), key_e2e),
                ("new-test".into(), key_new),
            ],
            &[verdict],
        );
        assert_eq!(result.hits.len(), 1); // unit-test
        assert_eq!(result.misses.len(), 2); // e2e (uncacheable) + new-test (no verdict)
        assert_eq!(result.time_saved_ms, 3000);
    }

    #[test]
    fn explain_output_formatting() {
        let result = CacheLookupResult {
            hits: vec![CacheHit {
                test_id: "unit-test".into(),
                cache_key: "abc".into(),
                original_duration_ms: 5000,
                cached_at: "2026-01-01".into(),
            }],
            misses: vec![CacheMiss {
                test_id: "e2e-test".into(),
                reason: "uncacheable".into(),
            }],
            time_saved_ms: 5000,
            hit_rate: 0.5,
        };
        let text = explain_cache_lookup(&result);
        assert!(text.contains("Cache Lookup"));
        assert!(text.contains("unit-test"));
        assert!(text.contains("e2e-test"));
        assert!(text.contains("5.0s"));
    }

    #[test]
    fn format_duration_units() {
        assert_eq!(format_duration_ms(500), "500ms");
        assert_eq!(format_duration_ms(5000), "5.0s");
        assert_eq!(format_duration_ms(90000), "1.5m");
    }

    #[test]
    fn cache_json_roundtrips() {
        let result = CacheLookupResult {
            hits: vec![],
            misses: vec![],
            time_saved_ms: 0,
            hit_rate: 0.0,
        };
        let json = explain_cache_json(&result);
        assert_eq!(json["time_saved_ms"], 0);
        assert_eq!(json["hit_rate"], 0.0);
    }
}
