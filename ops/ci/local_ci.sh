#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

run() {
    local label="$1"
    shift
    printf '\n==> %s\n' "$label"
    "$@"
}

require() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "missing required tool: $1" >&2
        exit 1
    }
}

require cargo
require just
require jankurai
require docker

run "cargo fmt --all -- --check" cargo fmt --all -- --check
run "cargo clippy --workspace --exclude jeryu --all-targets --all-features -- -D warnings" \
    cargo clippy --workspace --exclude jeryu --all-targets --all-features -- -D warnings
run "just fast" just fast
run "cargo build --verbose" cargo build --verbose
run "cargo test --tests --verbose" cargo test --tests --verbose
run "TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1" \
    env TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1
run "cargo test --test ssh_install_test -- --ignored --test-threads=1" \
    cargo test --test ssh_install_test -- --ignored --test-threads=1
run "cargo test --test tui_recording -- --ignored --exact tui_demo_recording" \
    cargo test --test tui_recording -- --ignored --exact tui_demo_recording
run "fixture project validation" bash -lc 'cd tests/fixtures/fixture_project && cargo test --verbose'
run "bash tools/security-lane.sh ." bash tools/security-lane.sh .
run "jankurai audit" jankurai audit . --mode advisory --json target/ci-local/repo-score.json --md target/ci-local/repo-score.md
