# witness-rt

Runtime repair packet library for agent-native Rust.

## What This Crate Does

- Installs a panic hook that emits structured `RepairPacket` JSON
- Provides `agent_ensure!`, `agent_bail!`, `agent_expect!`, `agent_ok!` macros
- Maps panic locations to owning cells via `#[track_caller]`
- Zero external dependencies beyond `serde` / `serde_json`

## Invariants

- Repair packets always include file, line, column
- The panic hook must never panic itself
- Cell matching uses path-prefix comparison against registered `owned_paths`

## Commands

```bash
cargo check -p witness-rt
cargo test -p witness-rt
cargo test -p witness-rt --doc
```
