#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  scripts/firmware-flash-checkpoint.sh [--firmware /path/to/image.bin] [--endpoint URL] [--log-dir DIR]

This script runs the guarded firmware checkpoint flow for M5W:
1. starts monsgeek-driver with --enable-ota
2. runs preflight validation (official auto-download by default)
3. enforces typed phrase confirmation (FLASH M5W)
4. runs firmware flash command and captures output
5. queries post-flash firmware version
6. writes session metadata and recovery context to a log file
USAGE
}

FIRMWARE=""
ENDPOINT="http://127.0.0.1:3814"
LOG_DIR="./logs"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --firmware)
      FIRMWARE="${2:-}"
      shift 2
      ;;
    --endpoint)
      ENDPOINT="${2:-}"
      shift 2
      ;;
    --log-dir)
      LOG_DIR="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -n "$FIRMWARE" && ! -f "$FIRMWARE" ]]; then
  echo "Firmware file not found: $FIRMWARE" >&2
  exit 1
fi

mkdir -p "$LOG_DIR"
TS="$(date -u +"%Y%m%dT%H%M%SZ")"
SESSION_LOG="$LOG_DIR/firmware-checkpoint-$TS.log"
META_LOG="$LOG_DIR/firmware-checkpoint-$TS.meta"

cleanup() {
  if [[ -n "${DRIVER_PID:-}" ]]; then
    kill "$DRIVER_PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

{
  echo "== Firmware Checkpoint Session =="
  echo "timestamp_utc=$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  if [[ -n "$FIRMWARE" ]]; then
    echo "firmware=$FIRMWARE"
    echo "firmware_sha256=$(sha256sum "$FIRMWARE" | awk '{print $1}')"
    echo "firmware_source=local_unofficial"
  else
    echo "firmware=auto-download"
    echo "firmware_sha256=resolved_at_runtime"
    echo "firmware_source=vendor_official"
  fi
  echo "endpoint=$ENDPOINT"
  echo "warning=Firmware update can erase application config region"
  echo "backup_path=(record manually if backup is produced by your workflow)"
  echo
} | tee "$META_LOG"

echo "[1/6] Starting monsgeek-driver with --enable-ota" | tee -a "$SESSION_LOG"
monsgeek-driver --enable-ota >"$LOG_DIR/driver-$TS.log" 2>&1 &
DRIVER_PID=$!
sleep 1

echo "[2/6] Running firmware preflight validation" | tee -a "$SESSION_LOG"
VALIDATE_ARGS=(--endpoint "$ENDPOINT" firmware validate)
FLASH_ARGS=(--endpoint "$ENDPOINT" firmware flash --typed-phrase "FLASH M5W")
if [[ -n "$FIRMWARE" ]]; then
  VALIDATE_ARGS+=(--file "$FIRMWARE" --allow-unofficial)
  FLASH_ARGS+=(--file "$FIRMWARE" --allow-unofficial)
fi
monsgeek-cli "${VALIDATE_ARGS[@]}" 2>&1 | tee -a "$SESSION_LOG"

echo "[3/6] Typed phrase confirmation required" | tee -a "$SESSION_LOG"
read -r -p "Type FLASH M5W to continue: " PHRASE
if [[ "$PHRASE" != "FLASH M5W" ]]; then
  echo "typed phrase mismatch; aborting" | tee -a "$SESSION_LOG"
  exit 1
fi

echo "[4/6] Running guarded flash command" | tee -a "$SESSION_LOG"
monsgeek-cli "${FLASH_ARGS[@]}" 2>&1 | tee -a "$SESSION_LOG"

echo "[5/6] Querying post-flash firmware version" | tee -a "$SESSION_LOG"
monsgeek-cli --endpoint "$ENDPOINT" firmware version 2>&1 | tee -a "$SESSION_LOG"

echo "[6/6] Session complete" | tee -a "$SESSION_LOG"
echo "session_log=$SESSION_LOG" | tee -a "$META_LOG"
echo "driver_log=$LOG_DIR/driver-$TS.log" | tee -a "$META_LOG"
