#!/bin/bash
# Install jeryu-gc systemd timer (belt-and-suspenders GC safety net)
# Usage: sudo bash ops/ci/install-gc-timer.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Installing jeryu-gc systemd units from $SCRIPT_DIR..."
cp "$SCRIPT_DIR/jeryu-gc.service" /etc/systemd/system/
cp "$SCRIPT_DIR/jeryu-gc.timer"   /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now jeryu-gc.timer
echo "✅ jeryu-gc.timer installed and active."
systemctl status jeryu-gc.timer --no-pager
