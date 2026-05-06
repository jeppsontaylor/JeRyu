# jeryu Agent Workspace

Read first: `@/Users/bentaylor/.codex/RTK.md`, `agent/JANKURAI_STANDARD.md`, `docs/RTK.md`, `docs/testing.md`, `docs/architecture.md`, `agent-index.md`.

- Use `proof-lanes.toml` for change-type to proof-lane mapping.
- Read the `//! Owner:`, `//! Proof:`, and `//! Invariants:` headers in any `src/*.rs` file before editing it.
- Keep module ownership and route details in the generated maps and in `agent/JANKURAI_STANDARD.md`.
- Prefer module-local edits over cross-module edits.
- After compile failures, parse `reason: "compiler-message"` lines from JSON output.
- Run `cargo run -p jeryu -- repo render-agent-index` after changing proof lanes, root routing docs, or module ownership comments.
- Default proof commands live in `docs/testing.md`.

## Docs index

These are the agent-readable docs. Read the one that matches the current change before editing code.

- Architecture: `docs/architecture.md` (thin index; full spec in `docs/ARCHITECTURE.md`).
- Boundaries: `agent/boundaries.toml` (source of truth).
- Testing: `docs/testing.md` (lanes, required proof commands, budgets).
- Release: `docs/release.md` (release readiness, cost budgets).
- Release: `docs/release.md` (release readiness, gates, cost-stop conditions).
- Generated zones: `agent/generated-zones.toml` (manifest, currently empty).
- Audit rules: `agent/JANKURAI_STANDARD.md` and `jankurai_tracker.md`.
- Mission and standard: `docs/MISSION.md`, `agent/JANKURAI_STANDARD.md`.
- API and TUI surfaces: `docs/API.md`, `docs/JERYU_TUI.md`.
