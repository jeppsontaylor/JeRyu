#!/bin/bash
# Install vgit-gc systemd timer (belt-and-suspenders GC safety net)
# Usage: sudo bash ops/ci/install-gc-timer.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Installing vgit-gc systemd units from $SCRIPT_DIR..."
cp "$SCRIPT_DIR/vgit-gc.service" /etc/systemd/system/
cp "$SCRIPT_DIR/vgit-gc.timer"   /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now vgit-gc.timer
echo "✅ vgit-gc.timer installed and active."
systemctl status vgit-gc.timer --no-pager
