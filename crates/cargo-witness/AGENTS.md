# cargo-witness

Witness graph and repair routing for agent-native Rust workspaces.

## What This Crate Does

- `cargo witness build` — generates `.witness/witness-graph.json` with dual hashes per crate
- `cargo witness diff <old> <new>` — classifies changes as interface vs implementation
- `cargo witness diagnose` — routes `cargo check` errors to owning ARCs
- `cargo witness repair` — assembles minimal repair bundles from failure packets

## Invariants

- Interface hashes capture all `pub` item signatures via `syn` parsing
- Implementation hashes exclude pub signatures
- Compile diagnostics are always routed to an owning ARC
- Repair bundles contain minimal sufficient context

## Commands

```bash
cargo check -p cargo-witness
cargo test -p cargo-witness
cargo test -p cargo-witness --doc
cargo run -p cargo-witness -- build
cargo run -p cargo-witness -- diagnose
```
