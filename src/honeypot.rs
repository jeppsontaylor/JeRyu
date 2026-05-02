//! Owner: Supply-Chain Detonation / Honey Token Detection
//! Proof: `cargo test -p vgit -- honeypot`
//! Invariants: Honey tokens are seeded before untrusted workload starts; Tripwire kills the target process on any touch; never remove paths from the watch list without an AER

use notify::{EventKind, RecursiveMode, Watcher};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info};

/// Seeds the sandbox with honey tokens.
/// Returns the list of files to watch.
pub fn seed_sandbox(sandbox_dir: &str) -> Vec<PathBuf> {
    let mut tokens = Vec::new();
    let base = Path::new(sandbox_dir);

    // 1. Decoy AWS Credentials
    let aws_dir = base.join(".aws");
    if fs::create_dir_all(&aws_dir).is_ok() {
        let creds = aws_dir.join("credentials");
        let fake_aws = "[default]\naws_access_key_id = AKIAIOSFODNN7VGITXYZ\naws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYVGITKEY";
        if fs::write(&creds, fake_aws).is_ok() {
            tokens.push(creds);
        }
    }

    // 2. Decoy NPM registry token
    let npmrc = base.join(".npmrc");
    let fake_npm = "//registry.npmjs.org/:_authToken=npm_vgit_decoy_trap_token_x1y2z3";
    if fs::write(&npmrc, fake_npm).is_ok() {
        tokens.push(npmrc);
    }

    info!(
        "Seeded {} honey tokens in sandbox {}",
        tokens.len(),
        sandbox_dir
    );
    tokens
}

/// Retrieves the predefined honey token paths to monitor.
pub fn get_tokens(sandbox_dir: &str) -> Vec<PathBuf> {
    let base = Path::new(sandbox_dir);
    vec![base.join(".aws").join("credentials"), base.join(".npmrc")]
}

/// A wrapper struct to keep the notify watcher alive
pub struct Tripwire {
    _watcher: notify::RecommendedWatcher,
}

/// Starts watching the honey tokens and kills the target process if touched.
pub fn start_tripwire(
    pid: u32,
    tokens: Vec<PathBuf>,
    sandbox_dir: String,
) -> anyhow::Result<Tripwire> {
    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            // Check for any access or modify events
            if matches!(event.kind, EventKind::Access(_) | EventKind::Modify(_)) {
                // Ignore events on paths we don't care about, though we only subscribed to tokens
                tx.send(event.paths).ok();
            }
        }
    })?;

    for token in &tokens {
        if let Err(e) = watcher.watch(token, RecursiveMode::NonRecursive) {
            tracing::warn!("Failed to watch honey token {:?}: {}", token, e);
        }
    }

    // Spawn the active killer thread
    std::thread::spawn(move || {
        if let Ok(tripped_paths) = rx.recv() {
            error!(
                "🚨 DETONATION LANE TRIGGERED 🚨 Malicious action detected on honey tokens: {:?}",
                tripped_paths
            );

            // Force kill the pipeline process
            let _ = std::process::Command::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .status();

            // Drop a quarantine marker so `cleanup` knows to skip sandbox destruction
            let marker = Path::new(&sandbox_dir).join(".vgit_quarantine");
            let _ = fs::write(
                &marker,
                format!("Quarantined due to touching: {:?}", tripped_paths),
            );
        }
    });

    Ok(Tripwire { _watcher: watcher })
}
