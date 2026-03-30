#!/usr/bin/env bash
set -euo pipefail

ENDPOINT="http://127.0.0.1:3814"
LOG_WAIT_SECONDS=10
if [[ "${1:-}" == "--endpoint" ]]; then
    ENDPOINT="${2:-}"
    shift 2
fi

if [[ $# -ne 0 ]]; then
    echo "Usage: bash scripts/verify-discovery-paths.sh [--endpoint URL]"
    exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1"
        exit 1
    fi
}

for cmd in monsgeek-cli systemctl lsusb journalctl date mktemp grep tee; do
    require_cmd "$cmd"
done

SINCE="$(date '+%Y-%m-%d %H:%M:%S')"
OUT_JSON="$(mktemp)"
trap 'rm -f "$OUT_JSON"' EXIT

echo "==> Service status"
systemctl is-active monsgeek-driver.service monsgeek-inputd.service

echo "==> USB endpoints (VID 3151)"
lsusb -d 3151: || true

echo "==> Driver discovery result"
start_ts="$(date +%s)"
if ! monsgeek-cli --endpoint "$ENDPOINT" --json devices list | tee "$OUT_JSON"; then
    echo "FAIL: monsgeek-cli devices list failed"
    exit 1
fi
end_ts="$(date +%s)"
echo "devices_list_elapsed_s=$((end_ts - start_ts))"

echo "==> Waiting ${LOG_WAIT_SECONDS}s for probe completion logs"
sleep "$LOG_WAIT_SECONDS"

echo "==> Driver probe logs"
journalctl -u monsgeek-driver.service --since "$SINCE" --no-pager -l \
    | grep -E "probe_summary|probe_attempt|probe_candidate_|probe_target_selected|watch_dev_list: sending Init|USB: opening device|USB: found device|USB: claimed IF2|dongle-forward" \
    || true

if grep -q '"path"' "$OUT_JSON"; then
    echo "PASS: at least one online supported device discovered"
else
    echo "FAIL: no online supported devices discovered"
    echo "Hint: check probe_summary/probe_attempt lines above for exact failure mode"
    exit 2
fi
