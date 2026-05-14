default:
    @just --list

agent-index:
    cargo run -p jeryu -- repo render-agent-index

agent-audit:
    cargo run -p jeryu -- repo audit-agent-surface --json

agent-refresh:
    cargo run -p jeryu -- repo render-agent-index
    cargo run -p jeryu -- repo audit-agent-surface --json

fast:
    # fast-lane
    mkdir -p target/jankurai/cache
    CARGO_INCREMENTAL=0 cargo check --workspace --message-format=json
    CARGO_INCREMENTAL=0 cargo nextest run -p jeryu --lib

proof:
    mkdir -p target/jankurai
    jankurai proof . --changed-fast --out target/jankurai/fast-score.json

audit-fast base="origin/main":
    mkdir -p target/jankurai
    jankurai audit . --changed-fast --changed-from {{base}} --json target/jankurai/audit-fast.json --md target/jankurai/audit-fast.md --timings-json target/jankurai/audit-timings.json --mode advisory

jankurai-src-check JANKURAI_SRC="../jankurai":
    cargo check -p jankurai --manifest-path {{JANKURAI_SRC}}/Cargo.toml --locked

bench:
    cargo bench --workspace --no-fail-fast

check-fast:
    CARGO_INCREMENTAL=1 cargo check -p jeryu --tests --locked

test-fast:
    CARGO_INCREMENTAL=1 cargo nextest run -p jeryu --lib --no-fail-fast

medium:
    CARGO_INCREMENTAL=0 cargo check --workspace --message-format=json
    CARGO_INCREMENTAL=0 cargo nextest run -p jeryu --lib
    CARGO_INCREMENTAL=0 cargo test -p jeryu --tests -- --test-threads=1
    cargo run -p cargo-witness -- build
    cargo run -p cargo-vrc -- map --output-dir .

postgres-state-proof:
    scripts/postgres-state-proof.sh

deep:
    cargo nextest run -p jeryu
    cargo run -p cargo-witness -- diagnose

security:
    bash tools/security-lane.sh .

dependency-check:
    ./tools/check-dependencies.sh

release:
    cargo build --release -p jeryu
    cargo run -p cargo-aer -- scan --output aer-findings.json
    cargo run -p cargo-vrc -- map --output-dir .

tui-screenshots:
    scripts/capture-tui-screenshots.sh

tui-screenshot-smoke:
    cargo run --release -p tui-capture -- --cols 48 --rows 6 --out target/tui-capture/smoke.png --dump-text target/tui-capture/smoke.txt -- bash -lc "printf '┌────────────────────────┐\n│ Unicode border test    │\n│ Blocks: █ ▓ ▒ ░        │\n└────────────────────────┘\n'; sleep 2"
score:
	jankurai audit . --mode advisory --json agent/repo-score.json --md agent/repo-score.md --score-history agent/score-history.jsonl --score-history-csv agent/score-history.csv
doctor:
	jankurai doctor --fail-on high
	jankurai security run . --out target/jankurai/security/evidence.json
rust-map:
	jankurai rust map .
rust-witness:
	jankurai rust witness build .
rust-diagnose:
	jankurai rust diagnose .
check: fast score security rust-map rust-witness rust-diagnose
# jankurai scaffold Justfile
