//! Owner: Test fixture infrastructure
//! Proof: `cargo test --test '*' -- fixtures`
//! Invariants: Fixture data must stay deterministic and avoid network dependencies.
//!
//! This module provides shared test fixture utilities for integration tests.
//! The `fixture_project/` subdirectory contains a self-contained Rust project
//! (`signal-router`) used for realistic TUI screenshots and CI validation.

use std::path::{Path, PathBuf};

/// Returns the path to the fixture project directory.
pub fn fixture_project_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR must be set by cargo");
    Path::new(&manifest)
        .join("tests")
        .join("fixtures")
        .join("fixture_project")
}

/// Verify the fixture project directory exists and has expected structure.
pub fn assert_fixture_project_valid() {
    let dir = fixture_project_dir();
    assert!(dir.join("Cargo.toml").exists(), "fixture Cargo.toml missing");
    assert!(dir.join("src/lib.rs").exists(), "fixture src/lib.rs missing");
    assert!(dir.join("src/main.rs").exists(), "fixture src/main.rs missing");
    assert!(
        dir.join("tests/integration.rs").exists(),
        "fixture integration test missing"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_project_has_expected_structure() {
        assert_fixture_project_valid();
    }

    #[test]
    fn fixture_project_cargo_toml_is_valid() {
        let toml_path = fixture_project_dir().join("Cargo.toml");
        let contents = std::fs::read_to_string(toml_path).expect("reading fixture Cargo.toml");
        assert!(contents.contains("signal-router"));
        assert!(contents.contains("[package]"));
    }
}
