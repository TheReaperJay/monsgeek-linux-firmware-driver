#!/usr/bin/env bash
set -u
set -o pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

ADDR="${MG_ADDR:-127.0.0.1:3814}"
LOG_FILE="${MG_DRIVER_LOG:-/tmp/mg-driver.log}"
WATCH_FILE="${MG_WATCH_OUT:-/tmp/mg-watch.out}"
GRPC_ERR_FILE="${MG_GRPC_ERR_OUT:-/tmp/mg-grpc.err}"
STRESS_SEND_OUT="${MG_STRESS_SEND_OUT:-/tmp/mg-stress-send.out}"
STRESS_SEND_ERR="${MG_STRESS_SEND_ERR:-/tmp/mg-stress-send.err}"
STRESS_READ_OUT="${MG_STRESS_READ_OUT:-/tmp/mg-stress-read.out}"
STRESS_READ_ERR="${MG_STRESS_READ_ERR:-/tmp/mg-stress-read.err}"

RUST_LOG_DEFAULT="monsgeek_driver=info,monsgeek_transport::usb=info,monsgeek_transport::discovery=debug"
RUST_LOG_VALUE="${RUST_LOG:-$RUST_LOG_DEFAULT}"
LAYER_STRESS=0
STRESS_ITERS="${MG_LAYER_STRESS_ITERS:-20}"
STRESS_READ_TIMEOUT="${MG_LAYER_READ_TIMEOUT_SEC:-1}"
STRESS_MSG_A="${MG_LAYER_MSG_A:-BQI=}"
STRESS_MSG_B="${MG_LAYER_MSG_B:-BQE=}"
MACRO_STRESS=0
MACRO_STRESS_ITERS="${MG_MACRO_STRESS_ITERS:-50}"
MACRO_STRESS_READ_TIMEOUT="${MG_MACRO_READ_TIMEOUT_SEC:-1}"
MACRO_MSG_A="${MG_MACRO_MSG_A:-CwAAOAE=}"
MACRO_MSG_B="${MG_MACRO_MSG_B:-CwABOAE=}"

usage() {
    cat <<'EOF'
Usage: bash tools/test.sh [options]

Options:
  --layer-stress           Run layer switch stress loop after smoke test
  --macro-stress           Run macro write stress loop after smoke test
  --iterations N           Layer stress iterations (default: 20)
  --read-timeout SEC       readMsg timeout seconds in stress mode (default: 1)
  --macro-iterations N     Macro stress iterations (default: 50)
  --macro-read-timeout SEC readMsg timeout seconds in macro stress mode (default: 1)
  --skip-tests             Skip focused cargo tests
  --help                   Show this help

Environment overrides:
  MG_ADDR                  gRPC address (default: 127.0.0.1:3814)
  MG_SKIP_TESTS            1 to skip focused tests
  MG_LAYER_STRESS_ITERS    default iteration count
  MG_LAYER_READ_TIMEOUT_SEC default read timeout seconds
  MG_LAYER_MSG_A           base64 message for odd iterations (default: BQI=)
  MG_LAYER_MSG_B           base64 message for even iterations (default: BQE=)
  MG_MACRO_STRESS_ITERS    default macro iteration count
  MG_MACRO_READ_TIMEOUT_SEC default macro read timeout seconds
  MG_MACRO_MSG_A           base64 macro message for odd iterations (default: CwAAOAE=)
  MG_MACRO_MSG_B           base64 macro message for even iterations (default: CwABOAE=)
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --layer-stress)
            LAYER_STRESS=1
            ;;
        --macro-stress)
            MACRO_STRESS=1
            ;;
        --iterations)
            shift
            STRESS_ITERS="${1:-}"
            ;;
        --read-timeout)
            shift
            STRESS_READ_TIMEOUT="${1:-}"
            ;;
        --macro-iterations)
            shift
            MACRO_STRESS_ITERS="${1:-}"
            ;;
        --macro-read-timeout)
            shift
            MACRO_STRESS_READ_TIMEOUT="${1:-}"
            ;;
        --skip-tests)
            MG_SKIP_TESTS=1
            ;;
        --help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
    shift
done

if ! [[ "$STRESS_ITERS" =~ ^[0-9]+$ ]] || [[ "$STRESS_ITERS" -lt 1 ]]; then
    echo "Invalid --iterations value: $STRESS_ITERS"
    exit 1
fi

if ! [[ "$STRESS_READ_TIMEOUT" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    echo "Invalid --read-timeout value: $STRESS_READ_TIMEOUT"
    exit 1
fi

if ! [[ "$MACRO_STRESS_ITERS" =~ ^[0-9]+$ ]] || [[ "$MACRO_STRESS_ITERS" -lt 1 ]]; then
    echo "Invalid --macro-iterations value: $MACRO_STRESS_ITERS"
    exit 1
fi

if ! [[ "$MACRO_STRESS_READ_TIMEOUT" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    echo "Invalid --macro-read-timeout value: $MACRO_STRESS_READ_TIMEOUT"
    exit 1
fi

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1"
        return 1
    fi
}

cleanup() {
    if [[ -n "${DRIVER_PID:-}" ]] && kill -0 "$DRIVER_PID" >/dev/null 2>&1; then
        kill "$DRIVER_PID" >/dev/null 2>&1 || true
        wait "$DRIVER_PID" >/dev/null 2>&1 || true
    fi
}

trap cleanup EXIT

for cmd in cargo grpcurl rg timeout pkill; do
    require_cmd "$cmd" || exit 1
done

echo "==> Optional focused tests"
if [[ "${MG_SKIP_TESTS:-0}" != "1" ]]; then
    cargo test -p monsgeek-protocol registry::tests::test_supports_runtime_vid_pid_with_alias -- --exact || exit 1
    cargo test -p monsgeek-protocol registry::tests::test_find_by_runtime_vid_pid_includes_aliases -- --exact || exit 1
    cargo test -p monsgeek-transport discovery::tests::test_unique_runtime_match_for_alias_pid -- --exact || exit 1
    cargo test -p monsgeek-driver service::tests::send_command_rpc_rejects_empty_device_path -- --exact || exit 1
    cargo test -p monsgeek-driver service::tests::read_response_rpc_rejects_empty_device_path -- --exact || exit 1
else
    echo "Skipped tests because MG_SKIP_TESTS=1"
fi

echo "==> Starting driver"
pkill -f monsgeek-driver >/dev/null 2>&1 || true
RUST_LOG="$RUST_LOG_VALUE" cargo run -p monsgeek-driver -- --addr "$ADDR" >"$LOG_FILE" 2>&1 &
DRIVER_PID=$!

for _ in $(seq 1 60); do
    if rg -q "Starting monsgeek-driver on $ADDR" "$LOG_FILE" 2>/dev/null; then
        break
    fi
    sleep 0.25
done

if ! rg -q "Starting monsgeek-driver on $ADDR" "$LOG_FILE" 2>/dev/null; then
    echo "Driver did not start. See $LOG_FILE"
    exit 1
fi

echo "==> watchDevList snapshot"
timeout 10 grpcurl -plaintext -import-path crates/monsgeek-driver/proto -proto driver.proto -d '{}' "$ADDR" driver.DriverGrpc/watchDevList >"$WATCH_FILE" 2>"$GRPC_ERR_FILE" || true
cat "$WATCH_FILE"

DEVPATH="$(rg -o '"path":\s*"[^"]+"' -m1 "$WATCH_FILE" | cut -d'"' -f4 || true)"
printf 'DEVPATH=<%s>\n' "$DEVPATH"

if [[ -n "$DEVPATH" ]]; then
    echo "==> send/read smoke"
    SEND_PAYLOAD=$(printf '{"devicePath":"%s","msg":"BQI=","checkSumType":0}' "$DEVPATH")
    READ_PAYLOAD=$(printf '{"devicePath":"%s"}' "$DEVPATH")
    grpcurl -plaintext -import-path crates/monsgeek-driver/proto -proto driver.proto -d "$SEND_PAYLOAD" "$ADDR" driver.DriverGrpc/sendMsg || true
    timeout 2 grpcurl -plaintext -import-path crates/monsgeek-driver/proto -proto driver.proto -d "$READ_PAYLOAD" "$ADDR" driver.DriverGrpc/readMsg || true

    if [[ "$LAYER_STRESS" == "1" ]]; then
        echo "==> layer stress (${STRESS_ITERS} iterations)"
        STRESS_TIMEOUT_ARG="${STRESS_READ_TIMEOUT}s"
        STRESS_FAILED=0

        for i in $(seq 1 "$STRESS_ITERS"); do
            if (( i % 2 == 1 )); then
                MSG="$STRESS_MSG_A"
            else
                MSG="$STRESS_MSG_B"
            fi

            SEND_PAYLOAD=$(printf '{"devicePath":"%s","msg":"%s","checkSumType":0}' "$DEVPATH" "$MSG")
            READ_PAYLOAD=$(printf '{"devicePath":"%s"}' "$DEVPATH")

            if ! grpcurl -plaintext -import-path crates/monsgeek-driver/proto -proto driver.proto -d "$SEND_PAYLOAD" "$ADDR" driver.DriverGrpc/sendMsg >"$STRESS_SEND_OUT" 2>"$STRESS_SEND_ERR"; then
                echo "layer-stress fail iter=$i stage=send grpc transport error"
                STRESS_FAILED=1
                break
            fi

            SEND_ERR_VAL="$(rg -o '"err":\s*"[^"]*"' -m1 "$STRESS_SEND_OUT" | cut -d'"' -f4 || true)"
            if [[ -n "$SEND_ERR_VAL" ]]; then
                echo "layer-stress fail iter=$i stage=send err=$SEND_ERR_VAL"
                STRESS_FAILED=1
                break
            fi

            if ! timeout "$STRESS_TIMEOUT_ARG" grpcurl -plaintext -import-path crates/monsgeek-driver/proto -proto driver.proto -d "$READ_PAYLOAD" "$ADDR" driver.DriverGrpc/readMsg >"$STRESS_READ_OUT" 2>"$STRESS_READ_ERR"; then
                echo "layer-stress fail iter=$i stage=read timeout_or_rpc_error"
                STRESS_FAILED=1
                break
            fi

            READ_ERR_VAL="$(rg -o '"err":\s*"[^"]*"' -m1 "$STRESS_READ_OUT" | cut -d'"' -f4 || true)"
            if [[ -n "$READ_ERR_VAL" ]]; then
                echo "layer-stress fail iter=$i stage=read err=$READ_ERR_VAL"
                STRESS_FAILED=1
                break
            fi

            if ! kill -0 "$DRIVER_PID" >/dev/null 2>&1; then
                echo "layer-stress fail iter=$i stage=driver driver_process_exited"
                STRESS_FAILED=1
                break
            fi

            echo "layer-stress iter=$i ok"
        done

        if [[ "$STRESS_FAILED" == "1" ]]; then
            echo "RESULT: FAIL (layer stress)"
            rg -n "send_msg:|read_msg:|cmd=0x05|Transport thread shutting down|Hot-plug: device removed|device path is empty|PID 0x4011|PID 0x4015" "$LOG_FILE" || true
            exit 3
        fi
    fi

    if [[ "$MACRO_STRESS" == "1" ]]; then
        echo "==> macro stress (${MACRO_STRESS_ITERS} iterations)"
        MACRO_TIMEOUT_ARG="${MACRO_STRESS_READ_TIMEOUT}s"
        MACRO_FAILED=0

        for i in $(seq 1 "$MACRO_STRESS_ITERS"); do
            if (( i % 2 == 1 )); then
                MSG="$MACRO_MSG_A"
            else
                MSG="$MACRO_MSG_B"
            fi

            SEND_PAYLOAD=$(printf '{"devicePath":"%s","msg":"%s","checkSumType":0}' "$DEVPATH" "$MSG")
            READ_PAYLOAD=$(printf '{"devicePath":"%s"}' "$DEVPATH")

            if ! grpcurl -plaintext -import-path crates/monsgeek-driver/proto -proto driver.proto -d "$SEND_PAYLOAD" "$ADDR" driver.DriverGrpc/sendMsg >"$STRESS_SEND_OUT" 2>"$STRESS_SEND_ERR"; then
                echo "macro-stress fail iter=$i stage=send grpc transport error"
                MACRO_FAILED=1
                break
            fi

            SEND_ERR_VAL="$(rg -o '"err":\s*"[^"]*"' -m1 "$STRESS_SEND_OUT" | cut -d'"' -f4 || true)"
            if [[ -n "$SEND_ERR_VAL" ]]; then
                echo "macro-stress fail iter=$i stage=send err=$SEND_ERR_VAL"
                MACRO_FAILED=1
                break
            fi

            if ! timeout "$MACRO_TIMEOUT_ARG" grpcurl -plaintext -import-path crates/monsgeek-driver/proto -proto driver.proto -d "$READ_PAYLOAD" "$ADDR" driver.DriverGrpc/readMsg >"$STRESS_READ_OUT" 2>"$STRESS_READ_ERR"; then
                echo "macro-stress fail iter=$i stage=read timeout_or_rpc_error"
                MACRO_FAILED=1
                break
            fi

            READ_ERR_VAL="$(rg -o '"err":\s*"[^"]*"' -m1 "$STRESS_READ_OUT" | cut -d'"' -f4 || true)"
            if [[ -n "$READ_ERR_VAL" ]]; then
                echo "macro-stress fail iter=$i stage=read err=$READ_ERR_VAL"
                MACRO_FAILED=1
                break
            fi

            if ! kill -0 "$DRIVER_PID" >/dev/null 2>&1; then
                echo "macro-stress fail iter=$i stage=driver driver_process_exited"
                MACRO_FAILED=1
                break
            fi

            echo "macro-stress iter=$i ok"
        done

        if [[ "$MACRO_FAILED" == "1" ]]; then
            echo "RESULT: FAIL (macro stress)"
            rg -n "send_msg:|read_msg:|cmd=0x0B|Transport thread shutting down|Hot-plug: device removed|device path is empty|PID 0x4011|PID 0x4015" "$LOG_FILE" || true
            exit 4
        fi
    fi
else
    echo "Skipping send/read smoke: DEVPATH empty"
fi

echo "==> Key logs"
rg -n "watch_dev_list|Probe fallback|Probe query failed|probe_device_at: query failed|PID 0x4011|PID 0x4015|claimed IF2|device path is empty" "$LOG_FILE" || true

if [[ -z "$DEVPATH" ]]; then
    echo "RESULT: FAIL (empty DEVPATH)"
    exit 2
fi

echo "RESULT: PASS (DEVPATH resolved)"
