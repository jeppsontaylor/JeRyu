# vgit Agent Workspace

Single-binary Rust CI/CD control plane, now part of a multi-crate workspace that includes the Proof-Scoped Control Plane tools.

## Proof Routing

Use `proof-lanes.toml` to map a change type to its required validation lanes. Check `[module_hints]` to identify the change type for a given file before running any tests.

Quick reference:

| Change Type | Lanes |
|---|---|
| `leaf-bugfix` | check, unit |
| `state-change` | check, unit, integration |
| `api-change` | check, unit, integration |
| `release-change` | check, unit, integration |
| `security-relevant` | check, unit, integration, security |
| `cross-module` | check, unit, integration, full |

## Proof-Scoped Control Plane

Five crates in `crates/` provide agent-efficiency tooling for this workspace. Run these as part of structural or API changes:

| Crate | Command | Purpose |
|---|---|---|
| `cargo-witness` | `cargo run -p cargo-witness -- build` | Build witness graph of pub API signatures; `diff` classifies changes; `diagnose` routes compile errors to owning module |
| `cargo-vrc` | `cargo run -p cargo-vrc -- map --output-dir .` | Generate `agent-map.json` + `test-map.json`; `plan <paths>` selects minimal test set by dep graph |
| `cargo-aer` | `cargo run -p cargo-aer -- scan --output aer-findings.json` | Audit for mega-files, structural exceptions; manage `aer-records/` |
| `witness-rt` | (library) | `agent_ensure!`, `agent_bail!`, `agent_expect!` macros + panic hook that emits structured repair packets |
| `arc-bench` | `cargo run -p arc-bench -- run psd-mechanics` | Benchmark ARC/VRC design tradeoffs |

**Default flow for any non-trivial change:**
1. `cargo run -p cargo-witness -- build` — refresh `.witness/witness-graph.json`
2. `cargo run -p cargo-vrc -- map --output-dir .` — refresh `agent-map.json` / `test-map.json`
3. `cargo run -p cargo-witness -- diff <old.json> <new.json>` — classify change scope before widening validation
4. `cargo run -p cargo-vrc -- plan <changed-paths> --output vrc-plan.json` — get minimal test selection
5. `cargo run -p cargo-witness -- diagnose` — after compile failures, get ARC-local routing

## Proof Commands

1. `cargo check --workspace --message-format=json`
2. `cargo nextest run -p vgit --lib --profile ci`
3. `cargo test -p vgit --test '*' -- --test-threads=1` (for state-dependent integration tests)
4. `just postgres-state-proof` (for Postgres-backed state changes; uses a disposable local container)

## Module Ownership

Each `src/*.rs` file has `//! Owner:`, `//! Proof:`, `//! Invariants:` at the top — read them before editing.

| Module | Owner Area | Change Type |
|---|---|---|
| `release.rs` | Release pipeline, canary, prod promotion | `release-change` |
| `state.rs` | Postgres-primary state, SQLite fallback, all DB types & queries | `state-change` |
| `engine.rs` | Webhook server, reconciliation loop | `api-change` |
| `dispatch.rs` | CLI routing hub (no business logic) | `api-change` |
| `test_intel/` | VTI smart test selection | `api-change` |
| `exec.rs` | Custom executor, sandbox isolation | `security-relevant` |
| `secrets.rs` | Vault lifecycle, rotation, handoff | `security-relevant` |
| `honeypot.rs` | Supply-chain detonation detection | `security-relevant` |
| `sandbox.rs` | Network-namespace workload isolation | `security-relevant` |
| `admission.rs` | Git hook admission control | `security-relevant` |
| `taint.rs` | Taint tracking (detonation lane) | `security-relevant` |
| `decision.rs` | Risk gates, supersedence, impact classification | `cross-module` |
| `policy.rs` | TrustTier, cache promotion gates | `cross-module` |
| `agent.rs` | Autonomous agent system | `api-change` |
| `pool.rs` | Runner fleet management | `api-change` |
| `cache.rs` | SmartCache, LRU GC, disk management | `leaf-bugfix` |
| `cache_brain.rs` | Cache decisions (trust + taint + epoch) | `cross-module` |
| `cache_proxy.rs` | sccache TCP proxy | `leaf-bugfix` |
| `epoch.rs` | Epoch-based cache invalidation | `state-change` |
| `buildkit.rs` | Per-namespace BuildKit config | `security-relevant` |
| `witness.rs` | Build witness cacheability classification | `leaf-bugfix` |
| `shadow.rs` | Remote mirror sync | `leaf-bugfix` |
| `impact.rs` | Change impact analysis | `leaf-bugfix` |
| `reclaim.rs` | Storage audit & GC | `leaf-bugfix` |
| `tui/` | Ratatui TUI dashboard | `leaf-bugfix` |

**Note**: `docs/ARCHITECTURE.md` contains the full system architecture. Navigate by section heading for the module you need. See also `docs/API.md` (complete API surface) and `docs/VTI.md` (smart test selection).

## Cross-Repo Contract

- `dougx` defines CI job meaning and lane semantics (`apps/veox-testctl/src/ci.rs`)
- `JeRyu` consumes that meaning for scheduling, reconciliation, and release execution
- `dougx/.vgit/testmap.toml` is the shared VTI subsystem map; JeRyu reads but never writes it

## Guardrails

- All errors currently use `anyhow`. Typed errors are a future migration.
- `main.rs` is the CLI dispatcher — push logic into modules.
- State changes go through `state::Db` methods, never raw SQL in callers.
- Use `--message-format=json` with `cargo check` / `cargo build` for structured diagnostics.
- Prefer module-local edits over cross-module edits.

## Security

Security-relevant modules: `secrets.rs`, `exec.rs`, `honeypot.rs`, `admission.rs`, `sandbox.rs`, `taint.rs`, `buildkit.rs`. All require the `security-relevant` proof lane (see `proof-lanes.toml`).

- Never log plaintext secrets; `secrets.rs` enforces current/previous rotation pairs only.
- `exec.rs` quarantines on tripwire — do not weaken sandbox isolation without an AER.
- `honeypot.rs` is supply-chain detection infrastructure — detonation logic changes require an AER.
- `sandbox.rs`: strict network isolation uses unshare/bwrap — do not bypass.
- `buildkit.rs`: never share builder state across trust namespaces.
- `taint.rs`: taint graph is append-only; purges are recorded events.

## Diagnostics

- After compile failures, parse `reason: "compiler-message"` lines from `--message-format=json` output for structured spans.
- Module-level doc comments in `src/*.rs` include `//! Proof:` and `//! Invariants:` lines — read them before editing.
- For state bugs, check `state::Db` for the relevant query method before looking elsewhere.
- Run `cargo run -p vgit -- repo render-agent-index` after changing proof lanes, root routing docs, or module ownership comments.
- Run `cargo run -p vgit -- repo audit-agent-surface --json` to verify token budget, generated indexes, and routing surfaces.

## Build Speed

- Linker: mold configured via `.cargo/config.toml` for `x86_64-unknown-linux-gnu` — do not override for `*-musl` targets.
- CI: set `CARGO_INCREMENTAL=0` (deterministic, avoids stale incremental cache bugs); dev default is 1 for debug builds.
- Nextest: use `--profile ci` in CI (fail-fast=true, 120s timeout, retries=1). Use `--profile default` locally for full output.

## Token Optimization

- Use `cargo check --message-format=json` for structured compiler output.
- Use `cargo test -- --format=json --report-time -Z unstable-options` when available.
- Prefer targeted `cargo test -p vgit -- <test_name>` over full suite when the change is local.
- See `tips/flow/` and `tips/smart_test/` for additional speed patterns.

@docs/VTI.md
@docs/RTK.md
@agent-index.md
