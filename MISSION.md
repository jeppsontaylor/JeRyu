# MISSION.md

## Mission

`vgit` exists to replace wasteful, brittle, human-babysat Git processes with an agent-governed Rust control plane.

The project is opinionated about where control should live:

- Git remains the ledger of truth.
- GitLab is the first infrastructure substrate, not the product itself.
- Rust owns orchestration, policy, and safety-critical decisions.
- Agents propose, inspect, recover, and escalate inside explicit capability bounds.

The mission is not to add more YAML glue or another thin wrapper around existing CI. The mission is to build a system that can decide what work is still worth doing, prove why it decided that, and act with bounded authority.

## Why Now

The most common public complaints about Git processes and CI/CD are no longer edge cases. They are structural problems:

- Superseded pipelines keep running after a newer commit makes older work obsolete.
- Selective testing is too brittle, especially in monorepos, and often fights branch protection rules.
- Flaky tests and flaky infrastructure make red and green status less trustworthy than teams need.
- Retry and resume behavior is too coarse, so a small failure often causes a large rerun.
- Logs and status views do not explain clearly why a job ran, skipped, stalled, failed, or was canceled.
- Approval, secrets, and credential models are designed around humans and broad automation tokens, not around scoped autonomous agents.

These are not separate annoyances. They are all symptoms of the same missing layer: current CI systems are good at executing predefined work, but weak at making good decisions when the world changes underneath a run.

That is the opening for `vgit`.

## What `vgit` Already Has

This repository already has meaningful foundations for an agent-first control plane:

- **GitLab bootstrap and runner-pool control**: `vgit` can stand up a local GitLab stack, provision runner pools, scale managers, rotate tokens, pause/drain capacity, and reconcile Docker-backed runtime state.
- **Webhook-driven engine and state ledger**: the engine receives job, pipeline, and push events and records lifecycle state into SQLite so orchestration can be event-driven and auditable.
- **Custom executor and isolated workspaces**: the execution plane can provision per-job sandboxes and intercept runner lifecycle stages instead of treating the runner as an untouchable black box.
- **Ephemeral bot identities and agent workflow scaffolding**: agents can get disposable GitLab identities, create branches, track work via issues, open merge requests, inspect results, and operate with tighter attribution than a shared service account.

These are real strengths. They mean the repository is already more than a concept deck.

Just as important, the codebase shows where the implementation is strongest today:

- Infrastructure and agent substrate: mostly real.
- Decision-making layer: partially scaffolded.
- Intelligent CI control plane: the main opportunity.

In practical terms, the runner, state, GitLab client, and agent-management paths are more mature than the semantic CI paths. The push-hook and capability-based semantic execution flow exists as a hook and prototype, not yet as the finished product promise.

## What Is Still Missing

The highest-value pain in Git processes is only partially addressed in this repo today. The missing layer is the actual decision system.

`vgit` does not yet fully deliver:

- **Commit supersedence with partial DAG reuse**: the system does not yet natively decide which work is obsolete, which work should be degraded, and which prior evidence remains reusable after a new commit lands.
- **Semantic impact analysis tied to safe required checks**: there is not yet a finished impact engine that can map a diff to the smallest safe job and test set while still satisfying merge and policy requirements.
- **Flake memory with confidence-based retry and quarantine**: the system does not yet keep enough durable reliability history to distinguish likely transient failures from probable regressions.
- **Structured causal explanations**: it does not yet expose a complete machine-readable explanation layer for why work ran, skipped, waited, failed, retried, or was canceled.
- **Policy-driven agent authority for risky actions**: it does not yet enforce a complete least-privilege model for high-risk merges, deploys, secret use, or destructive operations under agent control.

This is the most important framing discipline for contributors: `vgit` already has compelling infrastructure control, but it has not yet finished the intelligence layer that would resolve the most common CI complaints.

## The Product Thesis

`vgit` should be built and judged according to a simple thesis:

- **GitLab-first today**: the current substrate is GitLab, local-first, and intentionally close to the runner and webhook machinery.
- **Rust-first in control and policy**: orchestration, safety rules, state transitions, and authority boundaries should live in typed Rust systems rather than in ad hoc CI configuration.
- **Agent-first in decision-making**: the product should assume autonomous workers are first-class actors, not accidental afterthoughts bolted onto human-centric pipelines.
- **Portable in long-term vision**: the principles should outlast any one CI vendor, even if GitLab is the right place to prove them first.

The strategic bet is not "AI writes YAML." The strategic bet is that the winning system decides what work is still valid, what must run next, what can be reused safely, and when an agent must escalate.

## Strategic Roadmap

The roadmap should stay tightly coupled to the loudest real-world complaints.

### 1. Supersedence Engine

Build a first-class **Supersedence Policy** layer that decides what obsolete work gets canceled, degraded, preserved, or resumed when newer intent arrives.

This should answer questions like:

- A new commit landed. Which in-flight jobs should stop immediately?
- Which expensive legs can be downgraded because their result no longer gates anything important?
- Which artifacts or proofs from older work remain reusable for the new head?

This is the shortest path to reducing wasted CI minutes and restoring trust that the system is working on the right thing.

### 2. Impact-Aware Execution

Build an **Impact Graph** that maps changed code to jobs, tests, policy checks, and deployment gates.

The goal is not path-glob cleverness. The goal is safe, explainable minimal execution:

- compute the smallest safe validation plan from the diff
- understand dependency and ownership boundaries
- satisfy required-check and branch-protection expectations without dummy workflows

This is the core answer to brittle selective testing.

### 3. Evidence Capsules

Build **Evidence Capsules** as durable records of failure, retry history, artifact lineage, trace excerpts, and execution context.

The system should preserve enough structured evidence to support:

- resumable execution
- meaningful partial reuse
- better debugging than raw logs alone
- future agent reasoning over prior runs

This is the bridge between orchestration and trustworthy memory.

### 4. Flake-Aware Reliability Layer

Build a **Confidence Gate** that classifies failures as likely transient, likely environmental, or likely product regressions.

That layer should drive:

- retry budgets
- quarantine behavior
- escalation rules
- confidence-aware success and failure reporting

Without this, autonomous control stays too timid when it should retry and too reckless when it should stop.

### 5. Risk and Approval Policy

Build a **Risk Gate** for merges, deploys, credential use, and destructive operations.

The aim is not to remove governance. The aim is to make governance agent-native:

- scoped authority instead of blanket tokens
- explicit escalation instead of silent permission failures
- rollback hooks and audit trails for dangerous actions
- policy based on risk shifts, not just static human checkpoints

This is what turns automation into a production-safe agent system.

## Platform Primitives

Contributors should use the same conceptual interfaces across docs, code, and roadmap discussion:

- **Supersedence Policy**: defines what obsolete work gets canceled, degraded, preserved, or resumed.
- **Impact Graph**: maps changed code to affected jobs, tests, deploy checks, and trust boundaries.
- **Evidence Capsule**: stores structured failure context, trace lineage, artifact references, and retry history.
- **Confidence Gate**: scores whether a failure is likely transient, infrastructural, or a real regression.
- **Risk Gate**: enforces least-privilege policy for merge, deploy, token, and destructive actions.

These are not just naming preferences. They are the product interfaces that connect the public pain to the system design.

## Contributor Guidance

When making roadmap decisions, contributors should prefer work that moves `vgit` from "can execute jobs and spawn agents" toward "can decide and justify the minimum safe next action."

That means prioritizing:

- decision quality over feature count
- structured evidence over more logs
- policy and bounded authority over convenience shortcuts
- reuse and resumability over full reruns
- explainability over opaque heuristics

If a feature makes the system faster but harder to explain, govern, or audit, it is likely the wrong optimization.

## Public Evidence / References

The pain described above is visible in recurring public complaints and official platform documentation. Useful references include:

- GitHub Docs: troubleshooting required status checks  
  <https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/collaborating-on-repositories-with-code-quality-features/troubleshooting-required-status-checks>
- GitHub Docs: workflow concurrency and `cancel-in-progress`  
  <https://docs.github.com/actions/writing-workflows/choosing-what-your-workflow-does/control-the-concurrency-of-workflows-and-jobs>
- GitLab Support: auto-cancel redundant pipelines  
  <https://support.gitlab.com/hc/en-us/articles/22118112967068-How-to-auto-cancel-redundant-pipelines>
- GitHub Community: required checks and path filtering complaints  
  <https://github.com/orgs/community/discussions/44490>
- GitHub Community: retry and rerun limitations  
  <https://github.com/orgs/community/discussions/121211>
- GitHub Community: skipped-condition and log explainability complaints  
  <https://github.com/orgs/community/discussions/20640>
- GitHub Community: runner queue and pickup complaints  
  <https://github.com/orgs/community/discussions/162688>
- GitHub Community: native test-results dashboard requests  
  <https://github.com/orgs/community/discussions/163123>
- Empirical flaky-build study  
  <https://arxiv.org/abs/2602.02307>

These references should be treated as evidence of repeated user pain, not as product requirements by themselves. The mission of `vgit` is to turn those repeated complaints into a coherent agent-first control plane built in Rust.
