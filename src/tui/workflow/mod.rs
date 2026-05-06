//! Owner: Interactive TUI subsystem — workflow DAG module
//! Proof: `cargo nextest run -p jeryu -- tui::workflow`
//! Invariants: Workflow subsystem is a self-contained plan-driven test execution DAG.

pub mod builder;
pub mod collector;
pub mod model;
pub mod nav;
pub mod widget;
