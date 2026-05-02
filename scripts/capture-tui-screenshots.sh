#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="${1:-$ROOT_DIR/paper/assets}"
DEBUG_DIR="$ROOT_DIR/target/tui-capture"

COLS="${VGIT_TUI_CAPTURE_COLS:-160}"
ROWS="${VGIT_TUI_CAPTURE_ROWS:-48}"
FONT_PATH="${VGIT_TUI_CAPTURE_FONT:-/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf}"
FONT_SIZE="${VGIT_TUI_CAPTURE_FONT_SIZE:-19}"
CELL_W="${VGIT_TUI_CAPTURE_CELL_W:-12}"
CELL_H="${VGIT_TUI_CAPTURE_CELL_H:-23}"
BG="${VGIT_TUI_CAPTURE_BG:-#17212b}"
FG="${VGIT_TUI_CAPTURE_FG:-#f4fbff}"
BRIGHTEN="${VGIT_TUI_CAPTURE_BRIGHTEN:-1.35}"
MAX_WAIT_MS="${VGIT_TUI_CAPTURE_MAX_WAIT_MS:-8000}"
MIN_WAIT_MS="${VGIT_TUI_CAPTURE_MIN_WAIT_MS:-1200}"
QUIET_MS="${VGIT_TUI_CAPTURE_QUIET_MS:-300}"

mkdir -p "$OUTPUT_DIR" "$DEBUG_DIR"

if [ ! -f "$FONT_PATH" ]; then
  echo "missing screenshot font: $FONT_PATH" >&2
  echo "install fonts-dejavu-core or set VGIT_TUI_CAPTURE_FONT" >&2
  exit 1
fi

cargo build --release -p vgit -p tui-capture

run_once() {
  local tab="$1"
  local output="$2"
  local ready_file
  ready_file="$(mktemp)"
  rm -f "$ready_file"

  "$ROOT_DIR/target/release/tui-capture" \
    --cols "$COLS" \
    --rows "$ROWS" \
    --out "$output" \
    --font "$FONT_PATH" \
    --font-size "$FONT_SIZE" \
    --cell-w "$CELL_W" \
    --cell-h "$CELL_H" \
    --bg "$BG" \
    --fg "$FG" \
    --brighten "$BRIGHTEN" \
    --min-wait-ms "$MIN_WAIT_MS" \
    --max-wait-ms "$MAX_WAIT_MS" \
    --quiet-ms "$QUIET_MS" \
    --ready-file "$ready_file" \
    --dump-text "$DEBUG_DIR/${tab}.txt" \
    -- "$ROOT_DIR/target/release/vgit" tui --screenshot --tab "$tab" --screenshot-hold-ms 10000

  if [ ! -s "$ready_file" ]; then
    echo "TUI did not signal readiness for tab: $tab" >&2
    echo "See $DEBUG_DIR/${tab}.txt" >&2
    exit 1
  fi

  rm -f "$ready_file"
  echo "captured ${output}"
}

declare -a shots=(
  "mission:$OUTPUT_DIR/vgit-tui-mission.png"
  "jobs:$OUTPUT_DIR/vgit-tui-jobs-flow.png"
  "agents:$OUTPUT_DIR/vgit-tui-agents.png"
  "tests:$OUTPUT_DIR/vgit-tui-tests-vti.png"
  "evidence:$OUTPUT_DIR/vgit-tui-evidence.png"
  "release:$OUTPUT_DIR/vgit-tui-release.png"
)

for shot in "${shots[@]}"; do
  tab="${shot%%:*}"
  output="${shot#*:}"
  run_once "$tab" "$output"
done
