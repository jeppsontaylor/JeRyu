//! Owner: SSH remote install integration test
//! Proof: `cargo test --test ssh_install_test -- --ignored --test-threads=1`
//! Invariants: Requires Docker; generates evidence in target/ci-evidence/ssh-install.
//!
//! This test is gated behind `#[ignore]` because it requires a running Docker
//! daemon.  CI runs it explicitly with `-- --ignored`.  Local developers can
//! run it with `bash ops/ci/ssh_install_integration.sh` directly or through
//! `cargo test --test ssh_install_test -- --ignored`.

use std::path::Path;
use std::process::Command;

/// Locate the repository root relative to this test file.
fn repo_root() -> String {
    std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo")
}

/// Check whether Docker is available.
fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
#[ignore = "requires Docker — run with `cargo test --test ssh_install_test -- --ignored`"]
fn ssh_install_integration_full() {
    if !docker_available() {
        eprintln!("SKIP: Docker is not available; skipping SSH install integration test");
        return;
    }

    let root = repo_root();
    let script = format!("{}/ops/ci/ssh_install_integration.sh", root);

    assert!(
        Path::new(&script).exists(),
        "SSH integration script not found at {script}"
    );

    let evidence_dir = format!("{}/target/ci-evidence/ssh-install", root);
    let output = Command::new("bash")
        .arg(&script)
        .env("EVIDENCE_DIR", &evidence_dir)
        .current_dir(&root)
        .output()
        .expect("failed to execute SSH integration script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("=== STDOUT ===\n{stdout}");
    if !stderr.is_empty() {
        eprintln!("=== STDERR ===\n{stderr}");
    }

    assert!(
        output.status.success(),
        "SSH integration test failed with exit code {:?}\nstdout: {stdout}\nstderr: {stderr}",
        output.status.code()
    );

    // Verify evidence files were created.
    let summary_path = format!("{evidence_dir}/summary.json");
    assert!(
        Path::new(&summary_path).exists(),
        "Evidence summary not found at {summary_path}"
    );

    let summary = std::fs::read_to_string(&summary_path).expect("reading evidence summary");
    assert!(
        summary.contains(r#""result": "pass""#),
        "Evidence summary does not contain passing result"
    );
}

#[test]
fn remote_install_dryrun_does_not_require_network() {
    let root = repo_root();

    // Build the jeryu binary path — use the cargo-provided path.
    let jeryu = match std::env::var("CARGO_BIN_EXE_jeryu") {
        Ok(path) => path,
        Err(_) => format!("{root}/target/debug/jeryu"),
    };

    if !Path::new(&jeryu).exists() {
        eprintln!("SKIP: jeryu binary not found at {jeryu}; build first");
        return;
    }

    let output = Command::new(&jeryu)
        .args([
            "remote",
            "install",
            "testuser@10.0.0.99",
            "--dry-run",
            "--yes",
            "--json",
            "--service-mode",
            "manual",
        ])
        .output()
        .expect("failed to run jeryu remote install --dry-run");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "dry-run should succeed without network: {:?}\nstdout: {stdout}",
        output.status.code()
    );

    assert!(
        stdout.contains("\"action\""),
        "dry-run JSON should contain action field"
    );
    assert!(
        stdout.contains("\"steps\""),
        "dry-run JSON should contain steps field"
    );
    assert!(
        stdout.contains("\"service_mode\""),
        "dry-run JSON should contain service_mode field"
    );
}
