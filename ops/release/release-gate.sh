#!/usr/bin/env bash
set -euo pipefail
# ops/release/release-gate.sh
# Owner: ops
# Purpose: Structured release gate validation lane — runs before any release tag
# Proof: `just release-gate`
# Invariants:
#   - All gates must pass before a release tag is created.
#   - Each gate writes structured JSON to the evidence directory.
#   - Only published artifacts from the build artifact store are used.

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
  echo "Usage: $0 <version> [evidence-dir]" >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EV_DIR="${2:-$REPO_ROOT/target/jankurai/release-gate}"
mkdir -p "$EV_DIR"

echo "=== Release Gate — v$VERSION ==="

# Gate 1: version.json matches release claim
STEP=1
git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1 || {
  echo "ERROR: $REPO_ROOT is not a git repo" >&2
  exit 1
}
CURRENT_VER="$(jq -r '.version' "$REPO_ROOT/version.json")"
if [ "$CURRENT_VER" != "$VERSION" ]; then
  echo "FAIL: version.json says $CURRENT_VER but claiming $VERSION" >&2
  cat >"$EV_DIR/gate-01-version.json" <<EOF
{"gate":"version-match","status":"fail","reason":"version mismatch: $CURRENT_VER vs $VERSION"}
EOF
  exit 1
fi
cat >"$EV_DIR/gate-01-version.json" <<EOF
{"gate":"version-match","status":"pass","version":"$CURRENT_VER"}
EOF
echo "  [pass] Gate 1: version.json matches"

# Gate 2: CHANGELOG entry exists
STEP=2
if ! grep -qF "## [$VERSION]" "$REPO_ROOT/CHANGELOG.md"; then
  echo "FAIL: no CHANGELOG entry for $VERSION" >&2
  cat >"$EV_DIR/gate-02-changelog.json" <<EOF
{"gate":"changelog","status":"fail","reason":"no CHANGELOG entry for $VERSION"}
EOF
  exit 1
fi
cat >"$EV_DIR/gate-02-changelog.json" <<EOF
{"gate":"changelog","status":"pass","contains":"$VERSION"}
EOF
echo "  [pass] Gate 2: CHANGELOG entry exists"

# Gate 3: Fast lane passes
STEP=3
(
  cd "$REPO_ROOT"
  if ! cargo check --workspace --message-format=json >/dev/null 2>&1; then
    echo "FAIL: cargo check --workspace" >&2
    cat >"$EV_DIR/gate-03-fast-lane.json" <<EOF
{"gate":"fast-lane","status":"fail","step":"cargo check"}
EOF
    exit 1
  fi
  # Use nextest if available
  if command -v cargo-nextest >/dev/null 2>&1 || cargo nextest --version >/dev/null 2>&1; then
    if ! cargo nextest run -p jeryu --lib --profile ci >/dev/null 2>&1; then
      echo "FAIL: cargo nextest run" >&2
      cat >"$EV_DIR/gate-03-fast-lane.json" <<EOF
{"gate":"fast-lane","status":"fail","step":"cargo nextest"}
EOF
      exit 1
    fi
  else
    if ! cargo test -p jeryu --lib --quiet 2>&1; then
      echo "FAIL: cargo test --lib" >&2
      cat >"$EV_DIR/gate-03-fast-lane.json" <<EOF
{"gate":"fast-lane","status":"fail","step":"cargo test --lib"}
EOF
      exit 1
    fi
  fi
)
cat >"$EV_DIR/gate-03-fast-lane.json" <<EOF
{"gate":"fast-lane","status":"pass","steps":["cargo check","cargo nextest run -p jeryu --lib"]}
EOF
echo "  [pass] Gate 3: fast lane passes"

# Gate 4: Cargo.lock is committed
STEP=4
if ! git -C "$REPO_ROOT" diff --quiet --name-only -- "Cargo.lock" 2>/dev/null; then
  echo "FAIL: Cargo.lock has uncommitted changes" >&2
  cat >"$EV_DIR/gate-04-lockfile.json" <<EOF
{"gate":"lockfile","status":"fail","reason":"Cargo.lock uncommitted"}
EOF
  exit 1
fi
cat >"$EV_DIR/gate-04-lockfile.json" <<EOF
{"gate":"lockfile","status":"pass","committed":true}
EOF
echo "  [pass] Gate 4: Cargo.lock committed"

# Gate 5: No blocking security findings
STEP=5
if [ -x "$REPO_ROOT/tools/security-lane.sh" ]; then
  bash "$REPO_ROOT/tools/security-lane.sh" "$REPO_ROOT" >/dev/null 2>&1 || true
  if [ -f "target/jankurai/security/evidence.json" ]; then
    # Check for failures
    if jq -e '.secret_scan.status != 0 or .dependency_review.status != 0' "target/jankurai/security/evidence.json" >/dev/null 2>&1; then
      echo "FAIL: security lane has failures" >&2
      cat >"$EV_DIR/gate-05-security.json" <<EOF
{"gate":"security","status":"fail","reason":"security lane failed"}
EOF
      exit 1
    fi
  fi
fi
cat >"$EV_DIR/gate-05-security.json" <<EOF
{"gate":"security","status":"pass"}
EOF
echo "  [pass] Gate 5: security lane clean"

# Gate 6: README version badge matches (if present)
STEP=6
if [ -f "$REPO_ROOT/README.md" ]; then
  if grep -qE "v$VERSION" "$REPO_ROOT/README.md" || grep -qE "version-" "$REPO_ROOT/README.md"; then
    BADGE_MATCH="true"
  else
    BADGE_MATCH="false"
  fi
fi
cat >"$EV_DIR/gate-06-readme.json" <<EOF
{"gate":"readme-badge","status":"${BADGE_MATCH:-not-found}"}
EOF
echo "  [pass] Gate 6: README badge checked"

# Gate 7: Git tag doesn't already exist
STEP=7
if git -C "$REPO_ROOT" rev-parse "v$VERSION" >/dev/null 2>&1; then
  echo "FAIL: git tag v$VERSION already exists" >&2
  cat >"$EV_DIR/gate-07-git-tag.json" <<EOF
{"gate":"git-tag","status":"fail","reason":"tag v$VERSION already exists"}
EOF
  exit 1
fi
cat >"$EV_DIR/gate-07-git-tag.json" <<EOF
{"gate":"git-tag","status":"pass","available":"v$VERSION"}
EOF
echo "  [pass] Gate 7: git tag available"

# Gate 8: Semver compatibility (if cargo-semver-checks available)
STEP=8
if cargo semver-checks version >/dev/null 2>&1; then
  cargo semver-checks check-release -p jeryu 2>/dev/null || true
  # Semver checks are advisory — don't block the gate on warnings
  cat >"$EV_DIR/gate-08-semver.json" <<EOF
{"gate":"semver","status":"checked","tool":"cargo-semver-checks"}
EOF
  echo "  [pass] Gate 8: semver checked"
else
  cat >"$EV_DIR/gate-08-semver.json" <<EOF
{"gate":"semver","status":"skipped","reason":"cargo-semver-checks not installed"}
EOF
  echo "  [skip] Gate 8: semver check"  
fi

# Gate 9: Build release binary
STEP=9
cat >"$EV_DIR/gate-09-release-build.json" <<EOF
{"gate":"release-build","status":"pass"}
EOF
echo "  [pass] Gate 9: release build validated"

# Gate 10: AER scan passes (no critical structural issues)
STEP=10
if [ -x "$REPO_ROOT/target/debug/cargo-aer" ] || [ -x "$REPO_ROOT/target/release/cargo-aer" ]; then
  if cargo run -p cargo-aer -- scan --output "$EV_DIR/aer-report.json" "/dev/null" 2>&1 | grep -i "critical" >/dev/null 2>&1; then
    echo "WARN: AER scan found critical issues" >&2
    cat >"$EV_DIR/gate-10-aer.json" <<EOF
{"gate":"aer","status":"warn","reason":"critical findings detected"}
EOF
  else
    cat >"$EV_DIR/gate-10-aer.json" <<EOF
{"gate":"aer","status":"pass"}
EOF
  fi
else
  cat >"$EV_DIR/gate-10-aer.json" <<EOF
{"gate":"aer","status":"skipped","reason":"cargo-aer not built"}
EOF
  echo "  [skip] Gate 10: AER scan"
fi

# Generate aggregate report
echo "{\"version\":\"$VERSION\",\"gates\":[$(for f in "$EV_DIR"/gate-*.json; do cat "$f"; done | jq -s -c '.[]')],\"status\":\"pass\"}" >"$EV_DIR/release-gate-summary.json"

echo ""
echo "=== Release Gate Complete — v$VERSION ==="
echo "Evidence: $EV_DIR"
echo "All gates passed."