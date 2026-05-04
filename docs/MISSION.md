# JeRyu Mission: Agent-Native Git for Autonomous Software

This is the canonical mission statement. The root [`MISSION.md`](../MISSION.md)
remains as a compatibility pointer for existing links and contributor habits.

## North Star

JeRyu should become the #1 agent-friendly version of Git: a Git-compatible
control plane where every agent action can be captured, explained, tested,
cached, distributed, governed, and recovered.

The product is not "AI writes YAML" and it is not another thin wrapper around a
CI vendor. JeRyu is a Rust-first, agent-native version control and testing
control plane that keeps Git as the ledger while adding the decision layer that
autonomous software work now needs.

Git should remain familiar. Agent work should become accountable.

## Product Promise

JeRyu should be installable without asking teams to abandon existing remotes,
branch habits, or CI practices on day one. It should wrap Git first, preserve the
current `origin`, and let developers adopt agent-aware control gradually.

When configured correctly, the agent-installed Git wrapper should record future
agent Git commands into local JeRyu state. That state becomes the basis for
evidence, recovery, policy, cache decisions, and later coordination with remote
runners.

Local install and uninstall on macOS and Linux must feel polished, reversible,
user-space by default, and safe for CI. JeRyu should not mutate shell startup
files or global machine state unless the user explicitly asks for it.

Remote SSH install should make a bigger runner machine easy to provision. A
developer should be able to point JeRyu at a Linux host and get a useful remote
execution surface without turning setup into infrastructure work.

MCP support should be world-class. CLI, TUI, and MCP surfaces should share the
same policy model, grants, evidence records, and action registry so agents and
humans are operating through one coherent system.

VTI, SmartCache, distributed runners, and proof receipts should stop agents from
wasting time on unnecessary local test runs. Skipping work must be conservative
on uncertainty and explainable when it happens.

The TUI should be a true mission-control surface for developers: action-first,
proof-rich, fast, beautiful, and operationally useful. It should make agent work,
test evidence, runner capacity, risk gates, cache decisions, and recovery options
visible without forcing developers to dig through raw logs.

## What JeRyu Is For

The most common complaints about modern Git and CI workflows are structural:

- superseded pipelines keep running after newer commits make older work obsolete;
- selective testing is brittle, especially in monorepos;
- flaky tests and flaky infrastructure make red and green status hard to trust;
- retry and resume behavior is too coarse, so small failures cause large reruns;
- logs and status views rarely explain why work ran, skipped, stalled, failed,
  retried, or was canceled;
- approval, secrets, and credential models were designed around humans and broad
  automation tokens, not scoped autonomous agents.

These are all symptoms of the same missing layer. Current systems are good at
executing predefined work, but weak at deciding what work is still valid after
the repository changes.

JeRyu should supply that layer.

## Current Foundation

This repository already has real foundations for that mission:

- Git-compatible install paths and a Git wrapper surface;
- local and remote install flows;
- GitLab bootstrap and runner-pool control;
- webhook-driven engine and durable state;
- custom executor and isolated workspaces;
- agent workflow scaffolding and scoped identities;
- VTI smart test selection tooling;
- SmartCache, trust, taint, epoch, and proof-routing primitives;
- proof-scoped workspace tools for witness graphs, routing maps, audits, and
  benchmarks.

Those foundations matter, but they are not the finished product promise. The
mission is to turn them into a coherent agent-native version control experience
that is easy to install, safe to govern, and hard to waste time with.

## Strategic Direction

Rust owns orchestration, installers, policy, state transitions, and
safety-sensitive behavior. Shell and Python glue should be avoided except where
it is unavoidable, temporary, and on a path back into the Rust control plane.

Every important action should produce structured evidence. A developer or agent
should be able to ask what happened, why it happened, who or what was allowed to
do it, what proof was produced, and what can be safely reused.

Test skipping must be explainable and conservative on uncertainty. JeRyu should
prefer running extra validation over silently under-testing ambiguous changes,
but it should also avoid wasting local agent time when durable proof already
exists or a remote runner can do the work better.

Deployment, release, secrets, rollback, runner management, cache policy, and
remote machine management should increasingly be handled by JeRyu instead of ad
hoc scripts. The goal is not to centralize everything for its own sake. The goal
is to make dangerous or expensive actions policy-aware, evidence-producing, and
recoverable.

Agent authority should be scoped. Agents should be able to propose, inspect,
repair, test, cache, distribute, recover, and escalate inside explicit capability
bounds. Risky actions should go through Risk Gates, grants, and audit trails
instead of broad ambient credentials.

## Platform Primitives

JeRyu's roadmap and code should keep converging around a small set of product
interfaces:

- **Supersedence Policy**: decides what obsolete work gets canceled, degraded,
  preserved, or resumed when newer intent arrives.
- **Impact Graph**: maps changed code to affected jobs, tests, deploy checks,
  owners, and trust boundaries.
- **Evidence Capsule**: stores structured failure context, trace lineage,
  artifact references, action provenance, and retry history.
- **Confidence Gate**: classifies whether a failure is likely transient,
  infrastructural, flaky, or a real product regression.
- **Risk Gate**: enforces least-privilege policy for merges, deploys, secrets,
  remote access, cache promotion, token use, and destructive operations.
- **Proof Receipt**: records why a validation plan was sufficient, what ran, what
  was skipped, what evidence was reused, and what uncertainty remains.

These primitives are the bridge between public developer pain and the concrete
system JeRyu needs to become.

## Product Expectations

JeRyu should be Git-compatible by default. Existing remotes, especially
`origin`, should survive installation. The developer's normal Git muscle memory
should keep working unless they intentionally opt into a stronger JeRyu behavior.

JeRyu should be Rust-first in implementation. Safety-sensitive behavior belongs
in typed, testable Rust paths rather than scattered shell scripts.

JeRyu should be local-first but distributed when useful. A laptop should be
enough to start, while remote runners should make heavier work feel natural.

JeRyu should be agent-native without becoming agent-only. Humans need clear
controls, readable evidence, and a high-quality TUI. Agents need structured
actions, durable grants, MCP tools, policy checks, and machine-readable proof.

JeRyu should treat docs as a first-class product surface. Installation, Git
wrapper behavior, test selection, runners, MCP, TUI workflows, policy, and
recovery paths should be documented well enough that an agent and a human can
arrive at the same operational model.

## Roadmap Focus

### Git-Compatible Agent Capture

The Git wrapper should preserve normal Git behavior while adding structured
capture for agent actions. Future agent Git commands should be attributable,
recoverable, and linked to JeRyu state when the wrapper is installed and active.

### Install and Remote Provisioning

Install, uninstall, doctor, dry-run, and remote SSH flows should feel like a
polished developer product. Local setup should be reversible and user-space by
default. Remote setup should make larger runner machines available without
requiring the developer to become an infrastructure operator.

### VTI and Smart Test Execution

The VTI subsystem should map changes to the smallest conservative validation
plan. SmartCache, proof receipts, and distributed runners should let agents avoid
unnecessary local work while preserving trust in the result.

### Supersedence and Impact

The system should decide which in-flight jobs are obsolete, which older evidence
is reusable, which changes require broader proof, and which validation can be
safely skipped. Path globs are not enough. The answer should be based on impact,
ownership, dependencies, policy, and evidence.

### Evidence, Recovery, and Confidence

JeRyu should preserve enough structured evidence to support resumable execution,
failure diagnosis, flake-aware retries, cache promotion, and future agent
reasoning. Confidence Gates should help the system retry when appropriate and
stop when risk is rising.

### Policy, Grants, and Risk

Risk Gates should govern merges, deploys, credential use, cache promotion,
remote access, rollbacks, and destructive actions. The model should be scoped,
auditable, and shared across CLI, TUI, MCP, and autonomous agents.

### Developer TUI

The TUI should be a dream developer cockpit for agent-native Git work. It should
show active actions, runner capacity, validation plans, proof receipts,
superseded work, cache decisions, policy prompts, recovery choices, and release
state in one fast, operational interface.

## Contributor Guidance

When making product and engineering decisions, prefer work that moves JeRyu from
"can execute jobs and spawn agents" toward "can decide and justify the minimum
safe next action."

Prioritize:

- decision quality over feature count;
- structured evidence over more raw logs;
- policy and bounded authority over convenience shortcuts;
- reuse and resumability over full reruns;
- clear install and uninstall behavior over clever setup;
- conservative test skipping over opaque speedups;
- shared CLI, TUI, and MCP semantics over disconnected surfaces;
- Rust-owned behavior over permanent shell glue;
- explainability over heuristics that cannot be audited.

If a feature makes the system faster but harder to explain, govern, recover, or
trust, it is probably the wrong optimization.

## Public Evidence / References

The pain described here is visible in recurring public complaints and official
platform documentation. Useful references include:

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

These references are evidence of repeated user pain, not a complete
requirements list. JeRyu's mission is to turn that pain into a coherent,
Git-compatible, agent-native control plane built in Rust.
