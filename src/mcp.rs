//! Owner: MCP adapter for external coding agents
//! Proof: `cargo check -p vgit` and `cargo test -p vgit mcp`
//! Invariants: MCP is a transport adapter over the existing capability policy;
//!             it must not bypass grant checks, evidence handling, or merge/release gates.

mod core;
mod http;
mod tools;

#[cfg(test)]
mod tests;

pub use core::start_mcp_stdio;
pub use http::start_mcp_http;
pub use tools::tool_manifest;

pub(crate) const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
pub(crate) const TOOL_PREFIX: &str = "vgit.";
