# Audit Rubric (local stub)

This file is a thin local pointer so findings that cite
`docs/audit-rubric.md#...` resolve inside this repo. The canonical rubric
lives in upstream `jankurai` and is the source of truth for rule shape,
matched terms, and confidence thresholds.

## Where the real rubric lives

- Upstream rules: `crates/jankurai/src/audit/` in the jankurai repo.
- Rule catalog used here: `agent/audit-policy.toml`.
- The score and per-finding evidence for this repo: `agent/repo-score.md`,
  `agent/repo-score.json`.

## Local anchors used by findings

These anchors are referenced from `agent/repo-score.md` and exist so that
finding URLs resolve. The substantive definition is upstream.

### top-level-risk-mapping

How findings are mapped to TLR (Top-Level Risk) groups such as
`Security, secrets, agency` and `Verification`. Configured upstream;
local routing surface is in `agent/proof-lanes.toml`.

### required-shape

The required shape for boundary, contract, and data findings. Local
boundary surface: `agent/boundaries.toml` (also see
`docs/boundaries.md`).

### future-hostile-language-rule

The dead-marker / fallback-soup rule. Local owner header convention is
documented in `agent/JANKURAI_STANDARD.md`.

### known-vibe-coding-insults

Catalog of duplicated-block, pseudo-fallback, and shape-bypass patterns
the scanner flags as `vibe`. Definitions are upstream.

## How to update this file

If a finding cites a new anchor under `docs/audit-rubric.md`, add the
heading here pointing at the upstream definition. Do not paraphrase the
upstream rule body locally; keep this file as a routing stub only.
