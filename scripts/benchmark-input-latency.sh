#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

ENDPOINT="http://127.0.0.1:3814"
ITERATIONS=30
LATENCY_DURATION=20
RUN_LATENCY_TRACER=1
RUN_USB_TRACER=0
RUN_COMPOSITOR_TRACER=0
TARGET_PATH=""
OUTPUT_DIR="${TMPDIR:-/tmp}/monsgeek-benchmark-$(date +%Y%m%d-%H%M%S)"

DEVICES_LIST_P95_MAX_MS=200
FIRMWARE_VERSION_P95_MAX_MS=1500

usage() {
    cat <<'EOF'
Usage: bash scripts/benchmark-input-latency.sh [options]

Options:
  --endpoint URL             Driver endpoint (default: http://127.0.0.1:3814)
  --iterations N             Number of timing runs per CLI command (default: 30)
  --path BRIDGE_PATH         Force target path for firmware version benchmark
  --latency-duration SEC     Duration for latency_tracer.py (default: 20)
  --output-dir DIR           Output directory for logs/results
  --skip-latency-tracer      Skip tools/latency_tracer.py
  --run-usb-tracer           Also run tools/usb_input_tracer.py (interactive; Ctrl+C)
  --run-compositor-tracer    Also run tools/compositor_tracer.py (interactive; Ctrl+C)
  -h, --help                 Show help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --endpoint)
            ENDPOINT="${2:-}"
            shift 2
            ;;
        --iterations)
            ITERATIONS="${2:-}"
            shift 2
            ;;
        --path)
            TARGET_PATH="${2:-}"
            shift 2
            ;;
        --latency-duration)
            LATENCY_DURATION="${2:-}"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="${2:-}"
            shift 2
            ;;
        --skip-latency-tracer)
            RUN_LATENCY_TRACER=0
            shift
            ;;
        --run-usb-tracer)
            RUN_USB_TRACER=1
            shift
            ;;
        --run-compositor-tracer)
            RUN_COMPOSITOR_TRACER=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

if ! [[ "$ITERATIONS" =~ ^[0-9]+$ ]] || [[ "$ITERATIONS" -lt 1 ]]; then
    echo "Invalid --iterations: $ITERATIONS"
    exit 1
fi

if ! [[ "$LATENCY_DURATION" =~ ^[0-9]+$ ]] || [[ "$LATENCY_DURATION" -lt 1 ]]; then
    echo "Invalid --latency-duration: $LATENCY_DURATION"
    exit 1
fi

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1"
        exit 1
    fi
}

for cmd in monsgeek-cli lsusb systemctl awk sed grep sort head tail date tee python3; do
    require_cmd "$cmd"
done

mkdir -p "$OUTPUT_DIR"
SUMMARY_FILE="$OUTPUT_DIR/SUMMARY.txt"
touch "$SUMMARY_FILE"

log() {
    echo "$*" | tee -a "$SUMMARY_FILE"
}

stats_line() {
    local label="$1"
    local file="$2"
    local n
    n="$(wc -l <"$file" | tr -d ' ')"
    if [[ "$n" -eq 0 ]]; then
        log "$label: no successful samples"
        return
    fi
    local sorted="$file.sorted"
    sort -n "$file" >"$sorted"
    local min max avg p95
    min="$(head -n1 "$sorted")"
    max="$(tail -n1 "$sorted")"
    avg="$(awk '{s+=$1} END {printf "%.2f", s/NR}' "$file")"
    p95="$(awk -v n="$n" 'NR==int((n*95+99)/100){print; exit}' "$sorted")"
    log "$label: n=$n min=${min}ms avg=${avg}ms p95=${p95}ms max=${max}ms"
}

bench_cmd() {
    local label="$1"
    local threshold_ms="$2"
    shift 2
    local out_file="$OUTPUT_DIR/${label}.ms"
    local err_file="$OUTPUT_DIR/${label}.errors.log"
    : >"$out_file"
    : >"$err_file"

    local failures=0
    for i in $(seq 1 "$ITERATIONS"); do
        local start_ns end_ns elapsed_ms
        start_ns="$(date +%s%N)"
        if "$@" >/dev/null 2>>"$err_file"; then
            end_ns="$(date +%s%N)"
            elapsed_ms=$(( (end_ns - start_ns) / 1000000 ))
            echo "$elapsed_ms" >>"$out_file"
        else
            failures=$((failures + 1))
        fi
    done

    stats_line "$label" "$out_file"
    if [[ "$failures" -gt 0 ]]; then
        log "$label: failures=$failures (see $err_file)"
    fi

    if [[ -s "$out_file" ]]; then
        local n p95
        n="$(wc -l <"$out_file" | tr -d ' ')"
        p95="$(sort -n "$out_file" | awk -v n="$n" 'NR==int((n*95+99)/100){print; exit}')"
        if [[ "$p95" -gt "$threshold_ms" ]]; then
            log "$label: WARNING p95=${p95}ms > threshold=${threshold_ms}ms"
            return 2
        fi
    fi

    if [[ "$failures" -gt 0 ]]; then
        return 1
    fi
    return 0
}

log "output_dir=$OUTPUT_DIR"
log "endpoint=$ENDPOINT iterations=$ITERATIONS latency_duration=${LATENCY_DURATION}s"
log "timestamp=$(date -Is)"
log ""

log "== Service Status =="
systemctl is-active monsgeek-driver.service monsgeek-inputd.service | tee -a "$SUMMARY_FILE"
log ""

log "== USB Endpoints (VID 3151) =="
lsusb -d 3151: | tee "$OUTPUT_DIR/lsusb-3151.txt" | tee -a "$SUMMARY_FILE"
log ""

log "== USB Topology =="
lsusb -t | tee "$OUTPUT_DIR/lsusb-tree.txt" | tee -a "$SUMMARY_FILE"
log ""

log "== Discovery Snapshot =="
monsgeek-cli --endpoint "$ENDPOINT" --json devices list \
    | tee "$OUTPUT_DIR/devices-list.json" | tee -a "$SUMMARY_FILE"
log ""

if [[ -z "$TARGET_PATH" ]]; then
    TARGET_PATH="$(grep -o '"path":[[:space:]]*"[^"]*"' "$OUTPUT_DIR/devices-list.json" | head -n1 | cut -d'"' -f4 || true)"
fi

if [[ -z "$TARGET_PATH" ]]; then
    log "FAIL: could not resolve target device path from devices list"
    exit 2
fi

log "target_path=$TARGET_PATH"
log ""

log "== Bridge Command Timings =="
bench_failures=0
if ! bench_cmd "devices_list" "$DEVICES_LIST_P95_MAX_MS" monsgeek-cli --endpoint "$ENDPOINT" --json devices list; then
    bench_failures=$((bench_failures + 1))
fi
if ! bench_cmd "firmware_version" "$FIRMWARE_VERSION_P95_MAX_MS" monsgeek-cli --endpoint "$ENDPOINT" --path "$TARGET_PATH" firmware version; then
    bench_failures=$((bench_failures + 1))
fi
log ""

if [[ "$RUN_LATENCY_TRACER" -eq 1 ]]; then
    log "== latency_tracer.py (${LATENCY_DURATION}s) =="
    if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
        python3 tools/latency_tracer.py --duration "$LATENCY_DURATION" \
            | tee "$OUTPUT_DIR/latency_tracer.txt" | tee -a "$SUMMARY_FILE"
    else
        sudo python3 tools/latency_tracer.py --duration "$LATENCY_DURATION" \
            | tee "$OUTPUT_DIR/latency_tracer.txt" | tee -a "$SUMMARY_FILE"
    fi
    log ""
fi

if [[ "$RUN_USB_TRACER" -eq 1 ]]; then
    log "== usb_input_tracer.py (interactive; Ctrl+C to stop) =="
    if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
        python3 tools/usb_input_tracer.py | tee "$OUTPUT_DIR/usb_input_tracer.txt"
    else
        sudo python3 tools/usb_input_tracer.py | tee "$OUTPUT_DIR/usb_input_tracer.txt"
    fi
    log ""
fi

if [[ "$RUN_COMPOSITOR_TRACER" -eq 1 ]]; then
    log "== compositor_tracer.py (interactive; Ctrl+C/close window to stop) =="
    if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
        GI_TYPELIB_PATH=/usr/lib64/girepository-1.0 python3 tools/compositor_tracer.py \
            | tee "$OUTPUT_DIR/compositor_tracer.txt"
    else
        sudo GI_TYPELIB_PATH=/usr/lib64/girepository-1.0 python3 tools/compositor_tracer.py \
            | tee "$OUTPUT_DIR/compositor_tracer.txt"
    fi
    log ""
fi

if [[ "$bench_failures" -eq 0 ]]; then
    log "RESULT: PASS (bridge latency benchmarks within thresholds)"
else
    log "RESULT: WARN ($bench_failures benchmark group(s) exceeded threshold or had failures)"
fi

log "artifacts=$OUTPUT_DIR"
