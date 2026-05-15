# jankurai Repo Score

- Standard: `jankurai`
- Auditor: `0.8.0`
- Schema: `1.5.0`
- Paper edition: `2026.05-ed8`
- Target stack ID: `rust-ts-vite-react-postgres-bounded-python`
- Target stack: `Rust core + TypeScript/React/Vite + PostgreSQL + generated contracts + exception-only Python AI/data service`
- Repo: `.`
- Run ID: `1778816414`
- Started at: `1778816414`
- Elapsed: `1775` ms
- Scope: `full`
- Raw score: `94`
- Final score: `70`
- Decision: `advisory`
- Minimum score: `85`
- Caps applied: `ci-local-parity`

## Hard Rule Caps

| Rule | Max Score | Applied |
| --- | ---: | --- |
| `no-root-agent-instructions` | 75 | no |
| `no-one-command-setup-or-validation` | 70 | no |
| `no-deterministic-fast-lane` | 65 | no |
| `no-security-lane-on-high-risk-repo` | 60 | no |
| `generated-contracts-or-public-api-drift-untested` | 80 | no |
| `python-direct-product-truth-or-db-ownership` | 72 | no |
| `no-secret-or-dependency-scanning-in-ci` | 78 | no |
| `no-jankurai-audit-lane-in-ci` | 82 | no |
| `jankurai-required-tool-ci-evidence-gap` | 88 | no |
| `non-optimal-product-language-found` | 74 | no |
| `too-much-python-in-product-surface` | 72 | no |
| `boundary-reclassification-evidence-gap` | 72 | no |
| `vibe-placeholders-in-product-code` | 68 | no |
| `fallback-soup-in-product-code` | 70 | no |
| `future-hostile-dead-language-in-product-code` | 64 | no |
| `severe-duplication-in-product-code` | 70 | no |
| `generated-zone-mutation-risk` | 76 | no |
| `direct-db-access-from-wrong-layer` | 66 | no |
| `missing-web-e2e-lane` | 82 | no |
| `missing-rendered-ux-qa-lane` | 84 | no |
| `prompt-injection-risk` | 78 | no |
| `overbroad-agent-agency` | 65 | no |
| `secret-like-content-detected` | 60 | no |
| `false-green-test-risk` | 76 | no |
| `destructive-migration-risk` | 70 | no |
| `authz-or-data-isolation-gap` | 78 | no |
| `input-boundary-gap` | 78 | no |
| `agent-tool-supply-chain-gap` | 78 | no |
| `release-readiness-gap` | 80 | no |
| `missing-rust-property-or-integration-tests` | 82 | no |
| `no-agent-friendly-exception-pattern` | 76 | no |
| `missing-agent-readable-docs` | 80 | no |
| `streaming-runtime-drift` | 78 | no |
| `rust-bad-behavior` | 72 | no |
| `sql-bad-behavior` | 72 | no |
| `typescript-bad-behavior` | 72 | no |
| `docker-bad-behavior` | 72 | no |
| `python-bad-behavior` | 72 | no |
| `ci-bad-behavior` | 70 | no |
| `git-bad-behavior` | 70 | no |
| `gittools-bad-behavior` | 70 | no |
| `release-bad-behavior` | 70 | no |
| `web-security-bad-behavior` | 68 | no |
| `repo-rot-bad-behavior` | 88 | no |
| `comment-hygiene-dangerous-residue` | 72 | no |
| `ci-local-parity` | 70 | yes |

## Copy-Code Redundancy

- Status: `review` hard=`0` warning=`6` files=`12`
- Policy: min-lines=`10` min-tokens=`100` max-findings=`50` include-tests=`false` strict=`false`
- Duplicate volume: lines=`6` tokens=`18` bytes=`178`

- Notes:
  - hard classes are limited to exact active-source file matches and substantial exact same-name units
  - warning classes include same-body different-name units and token/block duplication
  - tests, fixtures, stories, config, Docker, and migrations are omitted unless --include-tests is set

| Kind | Severity | Language | Lines | Tokens | Instances | Reason |
| --- | --- | --- | ---: | ---: | --- | --- |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 4 | `db/state.rs:1471-1472, db/state.rs:1534-1535, db/state.rs:1625-1626, db/state.rs:1638-1639, db/state.rs:1677-1678, db/state.rs:2599-2600, db/state.rs:2626-2627, db/state.rs:2653-2654, db/state.rs:2672-2673, db/state.rs:3104-3105` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 4 | `db/state.rs:3374-3375, db/state.rs:3384-3385, db/state.rs:3394-3395, db/state.rs:3411-3412, db/state.rs:3420-3421` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `db/state.rs:1910-1911, db/state.rs:2699-2700, db/state.rs:2737-2738, db/state.rs:2839-2840` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 5 | `db/state.rs:1812-1813, db/state.rs:2442-2443` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 2 | `db/state.rs:647-648, db/state.rs:654-655` | `same body appears under different names across files` |
| `ExactUnitDifferentName` | `Warning` | `rust` | 1 | 1 | `db/state.rs:706-707, db/state.rs:773-774` | `same body appears under different names across files` |

## Dimensions

| Dimension | Weight | Score | Weighted | Evidence |
| --- | ---: | ---: | ---: | --- |
| Ownership and navigation surface | 13 | 100 | 13.00 | root `AGENTS.md` present; `CODEOWNERS` present |
| Contract and boundary integrity | 13 | 100 | 13.00 | contract surface found; generated contract artifacts found |
| Proof lanes and test routing | 12 | 100 | 12.00 | one-command setup/validation lane found; deterministic fast lane found |
| Security and supply-chain posture | 12 | 86 | 10.32 | lockfile present; secret or dependency scan tooling found |
| Code shape and semantic surface | 12 | 100 | 12.00 | largest authored code file: test_font3.rs (16 LOC); most code files stay under 300 LOC |
| Data truth and workflow safety | 8 | 85 | 6.80 | database surface present; structured db boundary manifest present |
| Observability and repair evidence | 8 | 98 | 7.84 | observability libraries or patterns found; diagnostic shaping hints found |
| Context economy and agent instructions | 7 | 100 | 7.00 | root `AGENTS.md` present; root `AGENTS.md` stays short |
| Jankurai tool adoption and CI replacement | 7 | 64 | 4.48 | control-plane files present; applicable=17 |
| Python containment and polyglot hygiene | 4 | 100 | 4.00 | no Python files in scope |
| Build speed signals | 4 | 95 | 3.80 | build acceleration markers found; targeted test/build commands found |

## Reference Profile Structure

- Applicable cells: `5` canonical=`5` noncanonical=`0` guidance missing=`0`

| Cell | Status | Canonical | Detected | Aliases | Guidance | Owner | Proof lane | Agent fix |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `web` | `canonical` | `apps/web/` | `apps/web` | `frontend/, ui/, packages/web/, packages/ui/` | `present` | `apps/web` | `rendered UX / Playwright` | `keep `apps/web/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `api` | `canonical` | `apps/api/` | `apps/api` | `api/, server/, backend/` | `present` | `apps/api` | `edge handler / contract tests` | `keep `apps/api/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `domain` | `not_applicable` | `crates/domain/` | `-` | `domain/, core/` | `not_required` | `crates/domain` | `unit / property tests` | `no action` |
| `application` | `not_applicable` | `crates/application/` | `-` | `application/, usecases/, use-cases/` | `not_required` | `crates/application` | `use-case / authz tests` | `no action` |
| `adapters` | `not_applicable` | `crates/adapters/` | `-` | `adapters/, infra/, integrations/` | `not_required` | `crates/adapters` | `adapter integration tests` | `no action` |
| `workers` | `not_applicable` | `crates/workers/` | `-` | `workers/, jobs/, scheduler/, queue/` | `not_required` | `crates/workers` | `workflow / replay tests` | `no action` |
| `contracts` | `canonical` | `contracts/` | `contracts` | `openapi/, protobuf/, json-schema/, generated/` | `present` | `contracts` | `generation / drift checks` | `keep `contracts/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `db` | `canonical` | `db/` | `db` | `migrations/, constraints/, sql/` | `present` | `db` | `migration / constraint tests` | `keep `db/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |
| `python-ai` | `not_applicable` | `python/ai-service/` | `-` | `python/, ai-service/, evals/, embeddings/, model/` | `not_required` | `python/ai-service` | `eval / contract tests` | `no action` |
| `ops` | `canonical` | `ops/` | `.github, .github/workflows, ops` | `.github/, .github/workflows/, ci/, release/, observability/, security/` | `present` | `ops` | `security lane / workflow lint` | `keep `ops/AGENTS.md` aligned with owns / forbidden / proof lane guidance` |

## Rendered UX QA

- Web surface: `true`
- Layered UX lane: `true`
- Missing: `none`

## Tool Adoption

- Control plane present: `true`
- Applicable tools: `17`
- Configured: `11`
- CI evidence: `11`
- Artifact verified: `4`
- Replaced count: `11`
- Missing CI evidence: `audit-ci, proof-routing, copy-code, ci-bad-behavior, git-bad-behavior, release-bad-behavior, ux-qa, db-migration-analyze, contract-drift, authz-matrix, agent-tool-supply, release-readiness, cost-budget`

| Tool | Category | Mode | Status | Replaced | Artifacts |
| --- | --- | --- | --- | --- | --- |
| `audit-ci` | `audit` | `auto` | `ci_evidence` | `manual repo scoring, ad hoc score gates` | `agent/repo-score.json, agent/repo-score.md` |
| `proof-routing` | `proof` | `auto` | `ci_evidence` | `ad hoc proof lane selection, manual proof receipts` | `agent/repo-score.json, agent/repo-score.md, target/jankurai/repair-queue.jsonl` |
| `proofbind` | `proof` | `auto` | `artifact_verified` | `manual changed-surface routing, ad hoc proof obligation lists` | `target/jankurai/proofbind/surface-witness.json, target/jankurai/proofbind/obligations.json` |
| `proofmark-rust` | `proof` | `auto` | `artifact_verified` | `line-only coverage review, manual in-diff mutation review` | `target/jankurai/proofmark/proofmark-receipt.json, target/jankurai/proofmark/proof-receipt.json` |
| `copy-code` | `audit` | `auto` | `missing` | `ad hoc copy-code review, manual duplication triage` | `target/jankurai/copy-code.json, target/jankurai/copy-code.md` |
| `security` | `security` | `auto` | `artifact_verified` | `gitleaks, dependency review, SBOM/provenance` | `target/jankurai/security/evidence.json` |
| `ci-bad-behavior` | `security` | `auto` | `configured` | `mutable workflow refs, secret echo/debug workflow checks, non-blocking security scans` | `target/jankurai/language-bad-behavior.log` |
| `git-bad-behavior` | `audit` | `auto` | `configured` | `destructive git automation, force-push release scripts, hidden stash-based state` | `target/jankurai/language-bad-behavior.log` |
| `release-bad-behavior` | `release` | `auto` | `configured` | `manual release checklist, ad hoc tag and artifact review, manual provenance review` | `target/jankurai/language-bad-behavior.log` |
| `ux-qa` | `ux` | `auto` | `configured` | `playwright, axe-core, visual baselines` | `target/jankurai/ux-qa.json` |
| `db-migration-analyze` | `db` | `auto` | `configured` | `manual migration review` | `target/jankurai/migration-report.json` |
| `contract-drift` | `contract` | `auto` | `ci_evidence` | `handwritten contract drift checks, openapi diff` | `agent/repo-score.json, agent/repo-score.md` |
| `rust-witness` | `rust` | `auto` | `artifact_verified` | `manual witness graphing` | `target/jankurai/rust/witness-graph.json` |
| `vibe-coverage` | `audit` | `auto` | `not_applicable` | `manual vibe-coding coverage spreadsheet` | `target/jankurai/vibe-coverage.json, target/jankurai/vibe-coverage.md` |
| `coverage-evidence` | `proof` | `auto` | `not_applicable` | `manual coverage report review, ad hoc mutation survivor review` | `target/jankurai/coverage/coverage-audit.json, target/jankurai/coverage/coverage-audit.md` |
| `authz-matrix` | `security` | `auto` | `ci_evidence` | `manual authz matrix review` | `agent/repo-score.json, agent/repo-score.md` |
| `input-boundary` | `security` | `auto` | `not_applicable` | `manual unsafe sink review` | `agent/repo-score.json, agent/repo-score.md` |
| `agent-tool-supply` | `security` | `auto` | `ci_evidence` | `manual MCP/tool trust review` | `agent/repo-score.json, agent/repo-score.md` |
| `release-readiness` | `release` | `auto` | `ci_evidence` | `manual launch checklist` | `agent/repo-score.json, agent/repo-score.md` |
| `cost-budget` | `release` | `auto` | `ci_evidence` | `manual spend review` | `agent/repo-score.json, agent/repo-score.md` |

## Boundary manifest (ingested)

- Path: `agent/boundaries.toml`
- Stack: `rust-ts-vite-react-postgres` · version: `0.4.0`
- Queue path counts — adapter: `2`, event_contract: `1`, generated_type: `1`, client_marker: `7`, streaming_exception: `1`
- Content fingerprint: `sha256:3576da495134b252ca6cdfa69ff4c777cc70c4497991a3b9f0131cd85f9428f9`

## Boundary Reclassifications

No audited runtime boundary reclassifications declared.

## Findings

1. `high` `ci` `.github/workflows/jankurai.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.lib-missing`
   Reason: ops/ci/lib.sh is the shared helper module (artifact assertions, tool pins) every lane sources
   Fix: add ops/ci/lib.sh defining shared helpers and tool version pins
   Rerun: `just fast`
   Fingerprint: `sha256:37915fba1911bbff8067832d71760cb1c395b643f8bf5d61e8dd1f4ab2bcc5ca`
   Evidence: detector=ci.local-parity.lib-missing, path=.github/workflows/jankurai.yml, line=1, proof_window=None, snippet=name: jankurai
2. `high` `ci` `.github/workflows/jankurai.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.pre-push-hook-missing`
   Reason: without a mandatory pre-push gate, broken code can be pushed and CI is the first place a failure shows up
   Fix: add ops/git-hooks/pre-push that runs `bash ops/ci/quality-gates.sh` and wire it via `git config core.hooksPath ops/git-hooks`
   Rerun: `just fast`
   Fingerprint: `sha256:1eddecd5e7ed9fc3919ef4f85fe6c719399ff0704e889c28f7134662a1512fd7`
   Evidence: detector=ci.local-parity.pre-push-hook-missing, path=.github/workflows/jankurai.yml, line=1, proof_window=None, snippet=name: jankurai
3. `high` `ci` `.github/workflows/jankurai.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.doctor-missing`
   Reason: without a doctor script, developers cannot confirm their local environment matches CI
   Fix: add scripts/ci-doctor.sh listing every tool the ops/ci scripts depend on
   Rerun: `just fast`
   Fingerprint: `sha256:4f3bac5529ba53c005699b913af04e7bdeaa4a5538bcd3cd0d736f210438a6e5`
   Evidence: detector=ci.local-parity.doctor-missing, path=.github/workflows/jankurai.yml, line=1, proof_window=None, snippet=name: jankurai
4. `high` `ci` `.github/workflows/jankurai.yml:1`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.runner-missing`
   Reason: scripts/ci-local.sh is the local entry point that delegates to the same ops/ci scripts the workflows call
   Fix: add scripts/ci-local.sh exposing each CI lane locally
   Rerun: `just fast`
   Fingerprint: `sha256:8c0df3fda16a40e9a6b8ccf848f557b954f5560e130076937fc056ae021e3fce`
   Evidence: detector=ci.local-parity.runner-missing, path=.github/workflows/jankurai.yml, line=1, proof_window=None, snippet=name: jankurai
5. `high` `ci` `.github/workflows/jankurai.yml:12`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.workflow-not-thin`
   Reason: without a single source of truth, local runs drift from CI and breakage is only visible after push
   Fix: extract the workflow steps into ops/ci/<lane>.sh and call them with `bash ops/ci/<lane>.sh`
   Rerun: `just fast`
   Fingerprint: `sha256:ba344f9676c430768708b53b4ec1933119d58e63d9c2e754504cbc03add48853`
   Evidence: detector=ci.local-parity.workflow-not-thin, path=.github/workflows/jankurai.yml, line=12, proof_window=None, snippet=jobs:
6. `high` `ci` `.github/workflows/release.yml:24`
   Rule: `HLT-042-CI-LOCAL-PARITY`
   Check: `HLT-042-CI-LOCAL-PARITY:ci` `hard` confidence `0.95`
   Route: TLR `Verification`, lane `fast`, owner `ops`
   Docs: `docs/ci-local.md`
   Matched term: `ci.local-parity.workflow-not-thin`
   Reason: without a single source of truth, local runs drift from CI and breakage is only visible after push
   Fix: extract the workflow steps into ops/ci/<lane>.sh and call them with `bash ops/ci/<lane>.sh`
   Rerun: `just fast`
   Fingerprint: `sha256:f280e695679fbe0c4b1256ce68614ebb4a9012f8f98c5a4c088c15400a8ccb50`
   Evidence: detector=ci.local-parity.workflow-not-thin, path=.github/workflows/release.yml, line=24, proof_window=None, snippet=jobs:

## Policy

- Policy file: `./agent/audit-policy.toml`
- Minimum score: `85`
- Fail on: `critical, high`

## Agent Fix Queue

1. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - add ops/ci/lib.sh defining shared helpers and tool version pins
   Route: `Verification`/`fast`
2. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - add ops/git-hooks/pre-push that runs `bash ops/ci/quality-gates.sh` and wire it via `git config core.hooksPath ops/git-hooks`
   Route: `Verification`/`fast`
3. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - add scripts/ci-doctor.sh listing every tool the ops/ci scripts depend on
   Route: `Verification`/`fast`
4. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - add scripts/ci-local.sh exposing each CI lane locally
   Route: `Verification`/`fast`
5. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/jankurai.yml` - extract the workflow steps into ops/ci/<lane>.sh and call them with `bash ops/ci/<lane>.sh`
   Route: `Verification`/`fast`
6. `high` `HLT-042-CI-LOCAL-PARITY` `.github/workflows/release.yml` - extract the workflow steps into ops/ci/<lane>.sh and call them with `bash ops/ci/<lane>.sh`
   Route: `Verification`/`fast`
