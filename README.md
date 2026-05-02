# vgit — Agent-First GitLab Control Plane

`vgit` is a single-binary Rust control plane designed to turn a local GitLab instance into a high-performance orchestration engine for autonomous AI agents. 

It gives supervised agents typed control over issues, branches, runners, validation, evidence, merge gates, release gates, and TUI review surfaces without relying on the GitLab Web UI as the primary automation boundary.

## 🚀 One-Click Quickstart

Get your autonomous environment running from zero in seconds:

```bash
# 1. Build and install the binary
cargo install --path .

# 2. Initialize the entire stack (GitLab + DB + Runners + Smoke Test)
vgit init

# 3. Start the engine and process automated jobs
vgit serve
```

## ✨ Key Features

- **Autonomous Control Plane**: Rust coordinates runner lifecycle, job execution, validation proof, and release evidence.
- **Fail-Closed Sandbox Modes**: Soft local execution is explicit; strict network isolation uses available `bwrap`/`unshare` backends and fails closed when unavailable.
- **Ephemeral Matrix Identities**: Dynamically provisions one-time GitLab Bot Users for every agent task, ensuring clear Git attribution and Least Privilege.
- **Custom Executor Control**: Intercepts CI prepare, run, and cleanup phases to attach cache, sandbox, honeypot, and evidence behavior.
- **Proof-Scoped CI/CD**: Uses VTI receipts, conservative fallback, capability grants, and merge gates so smart test skipping remains auditable.
- **Extreme Memory Tuning**: Re-configured GitLab Omnibus to run headless on as little as 1.2GB of RAM.

## 🛠 Command Reference

- `vgit init`: Full setup of secrets, Docker containers, and default runner pools.
- `vgit serve`: Starts the engine and scaling reconciliation loop.
- `vgit status`: Comprehensive health check of GitLab, pools, managed containers, recent jobs, and cache footprint.
- `vgit cache status`: Show cache and disk-footprint details before large validation runs.
- `vgit local cargo --repo /home/ubuntu/dougx -- check`: Run a repo-local Cargo command through vgit-owned cache roots instead of the worktree `target/` directory.
- `vgit release status --ref-name main` (legacy alias: `--ref`): Inspect the latest release attempt, canary state, and evidence paths. Add `--json` for automation.
- `vgit release watch --ref-name main` (legacy alias: `--ref`): Continuously refresh the same release view in-place.
- `vgit shadow status --repo <path>`: Inspect the remote layout for a repository that should shadow to GitLab.
- `vgit shadow ensure --repo <path> --url <gitlab-url>`: Create or update a dedicated shadow remote.
- `vgit shadow push --repo <path>`: Push the current HEAD or `--mirror` to the configured shadow remote.
- `vgit agent spawn`: Launch a new autonomous agent with a unique identity and sandbox.
- `vgit pool scale`: Manually override runner manager counts.
- `vgit test plan --command "<cmd>"`: Preview inferred runner class, timeout, and tags before dispatch.
- `vgit test impact --base <sha> --head <sha>`: Ask the project impact policy which jobs, full-build gates, and canary release gates the diff requires.
- `vgit test run --command "<cmd>"`: Run a single validation command through a GitLab pipeline. If `--tags` is omitted, vgit infers a smart runner class from the command.
- `vgit test batch --command "<cmd>" --command "<cmd2>" ...`: Dispatch multiple commands in parallel through separate GitLab pipelines.
- `vgit serve`: When a release-impacting protected `main` full build turns green, the engine records the release attempt and triggers the exact-SHA `release-execution` GitLab pipeline with `VGIT_CANARY_APPROVED=1`.
- `vgit down`: Gracefully drain all runners and stop the GitLab stack.

For runner cache rollouts, drain and recreate each pool after updating the runner config template so new cache mounts take effect. For cleanup, use `vgit cache status` first and `vgit cache gc --dry-run` before deleting anything by hand; that GC now reports and can reclaim nested `target/nextest/extract/*` scratch trees under Cargo target roots.

## 🏗 Architecture

`vgit` is built on a modern async Rust stack:
- **Tokio**: High-concurrency runtime.
- **Axum**: Webhook receiver and API bridge.
- **Bollard**: Native Docker API integration.
- **SQLx + Postgres/SQLite**: Postgres-primary state for concurrent agent fleets with SQLite fallback for local development and tests.
- **Git2-RS**: Semantic repository and diff analysis.

## 🛡 Security & Isolation

Each agent task is associated with scoped identity, branch, validation, and evidence records. `vgit` manages project access tokens and capability grants, and strict sandbox requests fail closed unless an isolation backend is available.

---
Built for proof-carrying agent delivery.
