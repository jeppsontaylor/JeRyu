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
    cargo check --workspace
    cargo nextest run -p jeryu --lib

medium:
    cargo check --workspace
    cargo nextest run -p jeryu --lib
    cargo test -p jeryu --tests -- --test-threads=1
    cargo run -p cargo-witness -- build
    cargo run -p cargo-vrc -- map --output-dir .

postgres-state-proof:
    scripts/postgres-state-proof.sh

deep:
    cargo nextest run -p jeryu
    cargo run -p cargo-witness -- diagnose

security:
    cargo deny check
    cargo run -p cargo-aer -- scan --output aer-findings.json

release:
    cargo build --release -p jeryu
    cargo run -p cargo-aer -- scan --output aer-findings.json
    cargo run -p cargo-vrc -- map --output-dir .

tui-screenshots:
    scripts/capture-tui-screenshots.sh

tui-screenshot-smoke:
    cargo run --release -p tui-capture -- --cols 48 --rows 6 --out target/tui-capture/smoke.png --dump-text target/tui-capture/smoke.txt -- bash -lc "printf '┌────────────────────────┐\n│ Unicode border test    │\n│ Blocks: █ ▓ ▒ ░        │\n└────────────────────────┘\n'; sleep 2"
