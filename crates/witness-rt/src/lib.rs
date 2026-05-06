//! # witness-rt
//!
//! Runtime repair packets for agent-native Rust.
//!
//! This crate turns panics and assertion failures into structured
//! [`RepairPacket`] JSON that agents can consume directly instead of
//! parsing stack traces.
//!
//! ## Quick Start
//!
//! ```no_run
//! use witness_rt::{CellRegistration, HookConfig, install_panic_hook, register_cells};
//!
//! // 1. Register your cells at startup.
//! register_cells(vec![
//!     CellRegistration {
//!         id: "pricing".into(),
//!         purpose: "Quote pricing logic".into(),
//!         owned_paths: vec!["crates/pricing/src/".into()],
//!         invariants: vec!["totals are non-negative".into()],
//!         local_commands: vec!["cargo test -p pricing".into()],
//!         escalate_commands: vec![],
//!         hints: vec![],
//!     },
//! ]);
//!
//! // 2. Install the panic hook.
//! install_panic_hook(HookConfig::new("."));
//!
//! // 3. Use agent macros instead of bare assert!/unwrap/expect.
//! // witness_rt::agent_ensure!(total >= 0, "PRICE-NEG", "bad total", "check discounts", []);
//! ```
//!
//! ## Design
//!
//! - **Zero external dependencies** beyond `serde` / `serde_json`
//! - **`#[track_caller]`** propagation for accurate source locations
//! - **Panic-safe hook** that never panics itself
//! - **Cell registry** via `OnceLock` — set once, read concurrently

pub mod hook;
pub mod macros;
pub mod packet;

pub use hook::{current_timestamp, emit_repair_packet_direct, install_panic_hook, register_cells};
pub use packet::{CellRegistration, HookConfig, RepairPacket, emit_and_panic};
