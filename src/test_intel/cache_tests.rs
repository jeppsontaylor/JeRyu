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
        passed: false,
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
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.misses.len(), 2);
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
