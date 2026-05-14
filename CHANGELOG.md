# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [3.1.0] - 2026-05-14
### Added
- **`jeryu-gcd` always-on disk daemon** (`crates/jeryu-gcd/`) that watches
  root-disk pressure every 60 s and runs pressure-tier GC to maintain
  ≥ 80 GiB free (`ROOT_DISK_HEADROOM_MIN_FREE_BYTES` floor in
  `src/cache/types.rs`). `Type=notify` systemd service at
  `ops/ci/jeryu-gcd.service`. Reuses the existing
  `gc_disk_cache_with_pressure` machinery — no duplicate GC logic.
- **`sweep_incremental_caches`** (`src/cache/runtime_gc.rs`) sweeps
  `target/.../incremental/` directories under JeRyu cache roots at
  Warning ≥ 30 min age, Critical at any age, and Emergency without age
  bound (workspace local sweep stays opt-in via
  `JERYU_GCD_ALLOW_LOCAL_TARGET_SWEEP=1`). Active leases are preserved.
- **Bootstrap auto-install** of `jeryu-gcd.service` (`src/bootstrap.rs`
  step 8 of 9). Skipped via `JERYU_BOOTSTRAP_SKIP_GCD=1` on systems
  without systemd.
- **TUI Cache → Disk Pressure panel**
  (`src/tui/ui_panels_body_more_cache.rs`) shows live free space,
  pressure level, and color-coded state.
- **`jeryu host install-gcd-service --allow-sudo`** CLI command for
  manual install/recovery.
- **Workspace-wide thin CI lane scripts**: `ops/ci/rust-lane.sh`
  (fmt/clippy/build/deny/witness/vrc/aer) joins
  `ops/ci/release-lane.sh`, `ops/ci/release-ready-lane.sh`, and
  `ops/ci/jankurai-lane.sh` as a single source of truth for what CI
  runs.
- Interactive Ratatui Rust TUI for God-Mode control dashboard.
- GitHub templates and OSS documentation structure.
- Initial GitLab Omnibus bootstrap logic and execution engine.

### Changed
- **Workspace clippy is now zero under `-D warnings`**. `cargo clippy
  --all-targets --all-features -- -D warnings` is the local CI gate
  (matches the command in `.github/workflows/rust.yml`). Auto-fixable
  lints (~90) resolved via `cargo clippy --fix`; design-decision lints
  (`too_many_arguments`, glob imports, private-in-public, large-Err)
  addressed with targeted `pub(crate)` promotions, allow-only-when-
  schema-is-flat annotations, and dead-code removal.
- **`cargo deny check` is clean** with one documented advisory ignore
  in `deny.toml` (`RUSTSEC-2021-0140` — `rusttype` is a dev-dep-only
  via `tuiwright`; migration to `ab_glyph` tracked as a follow-up
  issue).
- `ops/ci/jeryu-gc.timer` cadence dropped 6 h → 12 h (the daemon owns
  the fast path now; the timer is a deep-sweep safety net).
- `df_usage` (`src/cache_reports.rs`) promoted to `pub` so `jeryu-gcd`
  can reuse it without duplicating parsing logic.
- All formerly disk-bound integration tests (`test_agent_lifecycle`,
  `test_full_lifecycle`, `test_job_cycle`, `test_pool_*`) now pass
  locally without manual `--skip` flags — the daemon keeps df above
  the 80 GiB runner-fanout headroom.

### Fixed
- `cargo-aer scan` reports **0 findings** — added `[package.metadata.agent]`
  blocks to `crates/adapters/cache-brain/Cargo.toml` and
  `crates/tui-capture/Cargo.toml` (purpose, owned_paths, invariants,
  local_validate, risk, consumers).
- `crates/witness-rt/src/packet.rs::for_assert` clippy warning
  silenced with a scoped allow — the 8-arg signature is a flat
  fixed-schema assert packet.
- Duplicate `mod tests` include in `src/test_runner_runtime.rs`
  (`test_runner_tests.rs` was being loaded twice).

### Follow-up issues (filed by this PR, not implemented here)
- HLT-001 — split `src/tui/app_runtime_sync.rs` (360 LOC).
- HLT-016 — wire dependency-review/SBOM/provenance into a blocking
  security lane.
- HLT-013 — Playwright e2e + Storybook/UX-QA for `apps/web/`.
- HLT-008 — proptest coverage across `crates/`.
- HLT-006 — DB query-layer audit for `db/`.
- HLT-016-rusttype — migrate `tuiwright` from `rusttype` to `ab_glyph`
  (lifts the `cargo deny` ignore).

## [3.0.1] - 2026-04-27
### Added
- V3.01 capability request envelope with request id, actor, nonce, expiry, optional budget, optional grant proof, and length-prefixed JSON framing support while retaining  `AgentIntent` compatibility.
- VTI proof receipts for internal plans plus `jeryu test select --emit-receipt`.
- Merge-gate VTI receipt enforcement so smart-skipped validation cannot satisfy the default policy without a proof receipt.
- Explicit strict sandbox backend reporting with fail-closed behavior when strict isolation is requested but no `bwrap` or `unshare` backend is available.
- Real `agent list` support through GitLab issue label queries.
- Deterministic `jeryu tui --capture` PNG export for paper, review, and agent evidence workflows.
- Action-first TUI Mission Control landing view with Top Signal, Attention Queue, Proof Stack, metric tiles, next actions, and compact sparkline context.
- Agent Cockpit TUI view with session phase, progress, branch/SHA, grants, timeline, and action guidance.
- Command palette preview pane backed by the action registry, showing risk, side effects, required grants, dry-run availability, disabled reasons, and execution guidance.
- IEEE-style V3.01 working paper sources, agent-friendly Markdown, bibliography, and generated TUI screenshots.
- Version control files: `VERSION` and `version.json`.
- Postgres-primary state backend with SQLite fallback, bootstrap-managed Postgres Compose service, optional `JERYU_TEST_POSTGRES_URL` smoke coverage, and a disposable `just postgres-state-proof` harness.
- Backend-neutral state SQL placeholder handling for core Postgres operations across pools, managers, job/event tracking, VTI records, capability grants, and admission decisions.
- Backend-aware cache control managers for epoch invalidation, taint propagation, and CacheBrain decisions; executor cache writes now go through `Db` methods.

### Changed
- `RunTests` capability requests now return pipeline trigger errors instead of silently succeeding after branch creation.
- Dynamic CI YAML for capability-triggered test branches now uses a typed serializer and a fixed scope allowlist.
- GitLab TLS certificate validation is secure by default; insecure cert acceptance requires explicit `JERYU_GITLAB_INSECURE_TLS`.
- Group webhook creation now enables push events so engine supersedence and VTI planning receive the events they depend on.
- Capability action listing now derives from the canonical action registry.
- The TUI now opens on Mission instead of Jobs so operators see blockers, missing proof, and next actions first.
- VTI subsystem ownership patterns now include nested TUI, gateway, and test-intelligence modules.
- Unknown file changes now conservatively select full validation instead of docs-only validation.
- API and TUI docs now describe the current nine-tab TUI and screenshot capture path.
- Shared state upserts now use portable `ON CONFLICT` SQL instead of SQLite-only `INSERT OR REPLACE` forms.
