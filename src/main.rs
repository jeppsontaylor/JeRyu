//! Owner: CLI dispatcher — no business logic
//! Proof: `cargo check -p vgit --message-format=json`
//! Invariants: Main only initializes runtime state and delegates behavior to dispatch/modules.
//! vgit — Headless GitLab with Rust god-mode control.
//!
//! A single binary to bootstrap, operate, and extend a local GitLab
//! instance with full programmatic control over runners, CI/CD,
//! issues, and autonomous agents.
//!
//! Structure:
//!   cli.rs      — clap struct/enum definitions (pure data)
//!   dispatch.rs — command→module wiring (no business logic)
//!   lib.rs      — domain modules

#![allow(dead_code)]

mod cli;
mod dispatch;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let is_tui = matches!(cli.command, cli::Commands::Tui { .. });

    if !is_tui {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .with_target(false)
            .init();
    }

    witness_rt::register_cells(vec![
        witness_rt::CellRegistration {
            id: "cache".into(),
            purpose: "LRU manager cache GC and eviction".into(),
            owned_paths: vec!["src/cache.rs".into(), "src/cache_brain.rs".into(), "src/cache_proxy.rs".into()],
            invariants: vec!["active manager caches only evicted when keep_active_managers=false".into()],
            local_commands: vec!["cargo nextest run -p vgit --lib -E 'test(/cache/)'".into()],
            escalate_commands: vec![],
            hints: vec![],
        },
        witness_rt::CellRegistration {
            id: "engine".into(),
            purpose: "Health loop and GC orchestration".into(),
            owned_paths: vec!["src/engine.rs".into()],
            invariants: vec!["emergency tier triggers above 93% disk usage, pass limit is 20 with stall detection".into()],
            local_commands: vec!["cargo nextest run -p vgit --lib -E 'test(/engine/)'".into()],
            escalate_commands: vec![],
            hints: vec![],
        },
        witness_rt::CellRegistration {
            id: "reclaim".into(),
            purpose: "Artifact and Docker volume cleanup".into(),
            owned_paths: vec!["src/reclaim.rs".into()],
            invariants: vec!["is_emergency forces 30m artifact age threshold".into()],
            local_commands: vec!["cargo nextest run -p vgit --lib -E 'test(/reclaim/)'".into()],
            escalate_commands: vec![],
            hints: vec![],
        },
        witness_rt::CellRegistration {
            id: "state".into(),
            purpose: "Postgres-primary state schema and accessor boundary".into(),
            owned_paths: vec!["src/state.rs".into()],
            invariants: vec!["all mutations go through state::Db methods or backend-neutral state helpers".into()],
            local_commands: vec!["cargo nextest run -p vgit --lib -E 'test(/state/)'".into()],
            escalate_commands: vec![],
            hints: vec![],
        },
        witness_rt::CellRegistration {
            id: "release".into(),
            purpose: "Tag negotiation, artifact publication, and rollback".into(),
            owned_paths: vec!["src/release.rs".into()],
            invariants: vec!["release pipeline is a transaction — partial completion triggers rollback".into()],
            local_commands: vec!["cargo nextest run -p vgit --lib -E 'test(/release/)'".into()],
            escalate_commands: vec![],
            hints: vec![],
        },
        witness_rt::CellRegistration {
            id: "test-intel".into(),
            purpose: "Smart test selection (VTI)".into(),
            owned_paths: vec!["src/test_intel/".into()],
            invariants: vec!["TestPlan selected_tests with kind=unit_filter contain full nextest -E command strings".into()],
            local_commands: vec!["cargo nextest run -p vgit --lib -E 'test(/test_intel/)'".into()],
            escalate_commands: vec![],
            hints: vec![],
        },
    ]);
    witness_rt::install_panic_hook(witness_rt::HookConfig::new("."));

    // Load ~/.vgit/settings.json (creates with defaults on first run).
    vgit::settings::init()?;

    dispatch::run(cli).await
}
