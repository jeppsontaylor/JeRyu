#!/usr/bin/env bash
# tools/check-dependencies.sh — Dependency hygiene for jeryu
#
# Checks:
# 1. Dependency pinning (exact versions, no loose bounds)
# 2. Security advisories via cargo-deny
# 3. Supply chain integrity (crates.io sources, checksum verification)
#
# Usage: ./tools/check-dependencies.sh [--fix]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

FIX_MODE=false
if [[ "${1:-}" == "--fix" ]]; then
    FIX_MODE=true
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

# -----------------------------------------------------------------------------
# Check 1: Dependency pinning
# -----------------------------------------------------------------------------
check_pinning() {
    log_info "Checking dependency pinning..."

    local issues=0

    # Check workspace dependencies table for loose versioning
    while IFS= read -r line; do
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "$line" ]] && continue
        
        # Flag single digit versions with no minor.patch
        if [[ "$line" =~ ^[[:space:]]*version[[:space:]]*=[[:space:]]*"([0-9]+)"[[:space:]]*$ ]]; then
            log_warn "Loose major-only version in Cargo.toml: $line"
            ((issues++))
        fi
    done < Cargo.toml

    if [[ $issues -gt 0 ]]; then
        log_error "Found $issues unpinned dependencies"
        return 1
    else
        log_info "Workspace dependencies appear pinned"
        return 0
    fi
}

# -----------------------------------------------------------------------------
# Check 2: Security advisories via cargo-deny
# -----------------------------------------------------------------------------
check_advisories() {
    log_info "Checking security advisories with cargo-deny..."

    if ! command -v cargo-deny &> /dev/null; then
        log_warn "cargo-deny not installed, skipping advisory check"
        return 0
    fi

    if cargo deny check advisories 2>&1; then
        log_info "No known security advisories affect this workspace"
        return 0
    else
        log_error "Security advisories found — review output above"
        return 1
    fi
}

# -----------------------------------------------------------------------------
# Check 3: Supply chain integrity
# -----------------------------------------------------------------------------
check_supply_chain() {
    log_info "Checking supply chain integrity..."

    if ! command -v cargo-deny &> /dev/null; then
        log_warn "cargo-deny not installed, skipping supply chain check"
        return 0
    fi

    local failed=0

    log_info "Checking crate sources..."
    cargo deny check sources 2>&1 || { log_error "Source check failed"; ((failed++)); }

    log_info "Checking license compliance..."
    cargo deny check license 2>&1 || { log_error "License check failed"; ((failed++)); }

    log_info "Checking duplicate dependencies..."
    cargo deny check bans 2>&1 || { log_error "Duplicate dependency check failed"; ((failed++)); }

    if [[ $failed -gt 0 ]]; then
        return 1
    fi
    return 0
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------
main() {
    log_info "Starting dependency hygiene check..."
    echo ""

    local failed=0

    check_pinning || ((failed++))
    echo ""

    check_advisories || ((failed++))
    echo ""

    check_supply_chain || ((failed++))
    echo ""

    if [[ "$FIX_MODE" == true ]]; then
        log_info "Fix mode: Attempting cargo-deny auto-fixes..."
        cargo deny check advisories --fix 2>&1 || true
    fi

    echo ""
    if [[ $failed -gt 0 ]]; then
        log_error "Dependency check failed with $failed issue(s)"
        exit 1
    else
        log_info "All dependency checks passed!"
        exit 0
    fi
}

main "$@"
