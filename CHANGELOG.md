# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] - 2026-05-14
### Fixed
- **[2:Release] tab now shows live pipeline progress** even when no formal release attempt exists. Previously the tab was blank whenever the release lifecycle had not been invoked, because rendering was gated entirely on a `release_attempts` DB row.
- `pipeline_progress_view` was never populated in the real sync path (only in demo mode). The background sync now builds it from `ci_job_runs` (proper stage names) with a fallback to `job_events` grouped by `pool_name`.
- `tick()` preserved the old `pipeline_progress_view` unconditionally, preventing background-sync values from propagating. Fixed to only preserve when background sync found nothing (demo mode parity maintained).

### Changed
- **Release tab visual redesign**: left panel now splits vertically — gate matrix on top (12 rows when an attempt is active, 4 rows when waiting), live pipeline progress bars (`████▓░`) below with per-stage breakdown, ETA, and overall %.
- Right panel replaced plain-text inspector with a color-coded **job list** filtered to the active pipeline: ● green=success, ◉ cyan=running, ✕ red=failed, ○ yellow=pending.
- Gate matrix badge color `[RUN]` changed from Blue to Cyan for better terminal contrast.

### Added
- `build_stage_progress_from_ci_runs` — groups `ci_job_runs` by stage, computes per-stage counts and derived status.
- `build_stage_progress_from_events` — fallback that groups `job_events` by `pool_name` when `ci_job_runs` is empty.
- 5 new unit tests covering stage grouping, insertion-order preservation, pipeline-id filtering, status derivation, and the weighted-running progress formula.

## [1.0.0] - 2026-05-07
### Added
- Interactive Ratatui Rust TUI for God-Mode control dashboard.
- GitHub templates and OSS documentation structure.
- Initial GitLab Omnibus bootstrap logic and execution engine.

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
