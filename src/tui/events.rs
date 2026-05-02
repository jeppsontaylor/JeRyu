//! Owner: Interactive TUI subsystem — event handling stubs
//! Proof: `cargo nextest run -p jeryu -- tui::events`
//! Invariants: Input handling maps keys to registered actions without bypassing capability policy.
// TUI event handling is embedded directly in mod.rs run_loop via crossterm event polling.
// This module is reserved for future expansion (e.g., compound key bindings, modal dialogs).
