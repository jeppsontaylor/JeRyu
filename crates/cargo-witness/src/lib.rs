//! # cargo-witness
//!
//! Witness graph and repair routing for agent-native Rust workspaces.
//!
//! This crate provides four capabilities:
//!
//! 1. **Build** — generates a witness graph with dual hashes (interface vs implementation)
//! 2. **Diff** — classifies changes between two witness graphs
//! 3. **Diagnose** — routes compile errors to owning ARCs with enriched context
//! 4. **Repair** — assembles minimal repair bundles from failure packets

pub mod diagnose;
pub mod diff;
pub mod graph;
pub mod model;
pub mod repair;
