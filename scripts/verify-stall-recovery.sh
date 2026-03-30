#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

OFFLINE_FLAG=""
if [[ "${1:-}" == "--offline" ]]; then
    OFFLINE_FLAG="--offline"
    shift
fi

if [[ $# -ne 0 ]]; then
    echo "Usage: bash scripts/verify-stall-recovery.sh [--offline]"
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "Missing required command: cargo"
    exit 1
fi

LOG_FILE="$(mktemp)"
trap 'rm -f "$LOG_FILE"' EXIT

echo "==> Running hardware stall-recovery test"
echo "Command: cargo test -p monsgeek-transport $OFFLINE_FLAG --features hardware --test stall_recovery -- --nocapture"
cargo test -p monsgeek-transport $OFFLINE_FLAG --features hardware --test stall_recovery -- --nocapture 2>&1 | tee "$LOG_FILE"

if grep -q "PASS: reset-then-reopen clears STALL" "$LOG_FILE"; then
    echo "PASS: stall recovery confirmed"
else
    echo "FAIL: stall recovery proof marker not found in output"
    exit 2
fi
