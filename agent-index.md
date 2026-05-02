# Agent Index

Generated: `2026-04-27T14:12:20.103842141+00:00`

| Module | Change Type | Proof Commands | Owner |
|---|---|---|---|
| `src/admission.rs` | `security-relevant` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1<br>cargo test -p vgit -- secrets exec honeypot admission | Git Hook Admission Control |
| `src/agent.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Autonomous Agent System |
| `src/agent_surface.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Agent Surface |
| `src/bootstrap.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Bootstrap subsystem |
| `src/buildkit.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | BuildKit Configuration (Per-Trust-Namespace Rootless Builders) |
| `src/cache.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | SmartCache & Disk Management |
| `src/cache_brain.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Cache Decision Brain (Trust + Taint + Epoch Integration) |
| `src/cache_proxy.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Cache Proxy (sccache TCP Proxy) |
| `src/capability.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Capability API (Structured AgentIntent Payloads) |
| `src/capsule.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Failure Capsule subsystem |
| `src/cli.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | CLI Definitions |
| `src/config.rs` | `cross-module` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1<br>cargo nextest run -p vgit | Configuration & Templates subsystem |
| `src/decision.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Agent Decision Engine (Risk Gates, Supersedence, Impact Classification) |
| `src/dispatch.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | CLI Dispatch |
| `src/docker.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Docker Control Plane subsystem |
| `src/engine.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | Engine Core (Webhook + Reconciliation) |
| `src/epoch.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Epoch-Based Cache Invalidation |
| `src/exec.rs` | `security-relevant` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1<br>cargo test -p vgit -- secrets exec honeypot admission | Custom Executor & Sandbox Isolation |
| `src/explain.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Pipeline Explain subsystem |
| `src/gateway/cargo.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Cache Gateway subsystem — Cargo registry proxy |
| `src/gateway/git.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Cache Gateway subsystem — Git objects proxy |
| `src/gateway/mod.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Cache Gateway subsystem (module root) |
| `src/gateway/npm.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Cache Gateway subsystem — npm registry proxy |
| `src/gateway/oci.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Cache Gateway subsystem — OCI image proxy |
| `src/gateway/singleflight.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Cache Gateway subsystem — singleflight deduplication |
| `src/gitlab_client.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | GitLab REST Client subsystem |
| `src/honeypot.rs` | `security-relevant` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1<br>cargo test -p vgit -- secrets exec honeypot admission | Supply-Chain Detonation / Honey Token Detection |
| `src/impact.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Change Impact Analysis |
| `src/lib.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | vgit crate root (see module map below) |
| `src/logs.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Logging & Observability subsystem |
| `src/main.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | CLI dispatcher — no business logic |
| `src/policy.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Trust Policy (TrustTier, Cache Promotion Gates) |
| `src/pool.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | Runner Fleet / Pool Management |
| `src/reclaim.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Storage Audit & GC |
| `src/release.rs` | `release-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | Release Pipeline |
| `src/sandbox.rs` | `security-relevant` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1<br>cargo test -p vgit -- secrets exec honeypot admission | Workload Sandbox (Network-Namespace Isolation) |
| `src/sccache_mgr.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | sccache Management subsystem |
| `src/secrets.rs` | `security-relevant` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1<br>cargo test -p vgit -- secrets exec honeypot admission | Secrets & Vault Lifecycle |
| `src/settings.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | User settings subsystem |
| `src/shadow.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Shadow Remote Mirroring |
| `src/state.rs` | `state-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | State Store (Postgres primary, SQLite fallback) |
| `src/taint.rs` | `security-relevant` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1<br>cargo test -p vgit -- secrets exec honeypot admission | Taint Tracking (Detonation Lane) |
| `src/telemetry.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Runner Telemetry subsystem |
| `src/test_intel/cache.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | VTI Test Intelligence subsystem — plan cache |
| `src/test_intel/ci_gen.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — CI pipeline generation |
| `src/test_intel/explain.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — plan explanation |
| `src/test_intel/mod.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem (module root) |
| `src/test_intel/nightly.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — nightly oracle |
| `src/test_intel/planner.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — test plan algorithm |
| `src/test_intel/subsystem.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — subsystem ownership graph |
| `src/test_intel/testmap.rs` | `api-change` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib<br>cargo test -p vgit --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — testmap.toml parser |
| `src/test_runner.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | CI Test Runner subsystem |
| `src/tui/action_registry.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | TUI action surface and capability action contract. |
| `src/tui/app.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — application state and refresh loop |
| `src/tui/events.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — event handling stubs |
| `src/tui/flow/builder.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — flow graph builder |
| `src/tui/flow/collector.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — flow snapshot collector |
| `src/tui/flow/eta.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — flow ETA estimation |
| `src/tui/flow/inspector.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — flow inspector pane |
| `src/tui/flow/mod.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — CI flow view |
| `src/tui/flow/model.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — flow data model |
| `src/tui/flow/widget.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — flow graph widget |
| `src/tui/graph.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — pipeline graph rendering |
| `src/tui/mod.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem (module root) |
| `src/tui/ui.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Interactive TUI subsystem — rendering logic |
| `src/witness.rs` | `leaf-bugfix` | cargo check -p vgit --message-format=json<br>cargo nextest run -p vgit --lib | Build Witness (Cacheability Classification) |
