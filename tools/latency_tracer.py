#!/usr/bin/env python3
"""Trace keyboard input latency through the Linux input stack.

Measures three latency components:
1. Delivery latency: kernel event timestamp → wall-clock time when userspace reads it
2. Inter-report interval: time between consecutive SYN_REPORT events (should be ~1ms at 1000Hz)
3. Key lifecycle: press→release duration, release→repress gap

Uses CLOCK_MONOTONIC for wall-clock measurements and the kernel's input_event
timestamp (also CLOCK_MONOTONIC on modern kernels) for direct comparison.

Run: sudo python3 tools/latency_tracer.py [--duration SECONDS]
Stop: Ctrl+C — prints full latency distribution.
"""

import argparse
import ctypes
import math
import os
import re
import struct
import sys
import time
from collections import defaultdict
from pathlib import Path

# linux/input.h
EV_KEY = 0x01
EV_SYN = 0x00
SYN_REPORT = 0x00

EVENT_SIZE = 24
EVENT_FMT = "llHHi"

# CLOCK_MONOTONIC via ctypes for precise wall-clock measurement
CLOCK_MONOTONIC = 1
CLOCK_MONOTONIC_RAW = 4

class Timespec(ctypes.Structure):
    _fields_ = [("tv_sec", ctypes.c_long), ("tv_nsec", ctypes.c_long)]

_librt = ctypes.CDLL("librt.so.1", use_errno=True)
_clock_gettime = _librt.clock_gettime
_clock_gettime.argtypes = [ctypes.c_int, ctypes.POINTER(Timespec)]
_clock_gettime.restype = ctypes.c_int


def monotonic_ns():
    """Get CLOCK_MONOTONIC time in nanoseconds."""
    ts = Timespec()
    if _clock_gettime(CLOCK_MONOTONIC, ctypes.byref(ts)) != 0:
        raise OSError(ctypes.get_errno(), "clock_gettime failed")
    return ts.tv_sec * 1_000_000_000 + ts.tv_nsec


def monotonic_raw_ns():
    """Get CLOCK_MONOTONIC_RAW time in nanoseconds (no NTP adjustment)."""
    ts = Timespec()
    if _clock_gettime(CLOCK_MONOTONIC_RAW, ctypes.byref(ts)) != 0:
        raise OSError(ctypes.get_errno(), "clock_gettime failed")
    return ts.tv_sec * 1_000_000_000 + ts.tv_nsec


KEY_NAMES = {
    1: "ESC", 2: "1", 3: "2", 4: "3", 5: "4", 6: "5", 7: "6", 8: "7",
    9: "8", 10: "9", 11: "0", 14: "BKSP", 15: "TAB", 16: "Q", 17: "W",
    18: "E", 19: "R", 20: "T", 21: "Y", 22: "U", 23: "I", 24: "O", 25: "P",
    26: "[", 27: "]", 28: "ENTER", 29: "LCTRL", 30: "A", 31: "S", 32: "D",
    33: "F", 34: "G", 35: "H", 36: "J", 37: "K", 38: "L", 39: ";", 40: "'",
    41: "`", 42: "LSHIFT", 43: "\\", 44: "Z", 45: "X", 46: "C", 47: "V",
    48: "B", 49: "N", 50: "M", 51: ",", 52: ".", 53: "/", 54: "RSHIFT",
    56: "LALT", 57: "SPACE", 58: "CAPS", 100: "RALT", 97: "RCTRL",
    103: "UP", 105: "LEFT", 106: "RIGHT", 108: "DOWN",
}


def key_name(code):
    return KEY_NAMES.get(code, f"KEY_{code}")


def list_monsgeek_event_devices():
    devices = []
    by_id = Path("/dev/input/by-id")
    if by_id.exists():
        for link in sorted(by_id.iterdir()):
            name_lc = link.name.lower()
            if "monsgeek" in name_lc and "event-kbd" in name_lc:
                try:
                    devices.append((link.name, str(link.resolve())))
                except OSError:
                    continue

    # Also scan /proc so we include virtual inputd keyboards even when by-id exists.
    proc_devices = Path("/proc/bus/input/devices")
    if proc_devices.exists():
        try:
            content = proc_devices.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            content = ""
        blocks = [b for b in content.split("\n\n") if "monsgeek" in b.lower()]
        for block in blocks:
            name_match = re.search(r'N: Name="([^"]+)"', block)
            handlers_match = re.search(r"H: Handlers=(.+)", block)
            if not handlers_match:
                continue
            handlers = handlers_match.group(1).split()
            event_nodes = [h for h in handlers if h.startswith("event")]
            display = name_match.group(1) if name_match else "MonsGeek"
            for event_node in event_nodes:
                devices.append((f"{display} ({event_node})", f"/dev/input/{event_node}"))

    # Deduplicate by resolved path while preserving order.
    seen_paths = set()
    deduped = []
    for name, path in devices:
        if path in seen_paths:
            continue
        seen_paths.add(path)
        deduped.append((name, path))
    return deduped


def find_monsgeek_event_device(prefer_wireless=False):
    devices = list_monsgeek_event_devices()
    if not devices:
        return None, devices

    if prefer_wireless:
        for name, path in devices:
            if "2.4g" in name.lower():
                return path, devices
    else:
        # Prefer the monsgeek-inputd virtual keyboard for end-to-end typing latency.
        for name, path in devices:
            if "inputd" in name.lower():
                return path, devices
        for name, path in devices:
            if "2.4g" not in name.lower():
                return path, devices

    return devices[0][1], devices


def check_input_clock(dev_path):
    """Determine which clock the input device uses for timestamps.

    Modern kernels (4.x+) use CLOCK_MONOTONIC by default for input events.
    Check via the EVIOCSCLOCKID ioctl or by comparing timestamps.
    """
    import fcntl

    # EVIOCGCLOCKID = _IOR('E', 0xa4, int) = 0x80044ea4 (on 64-bit)
    EVIOCGCLOCKID = 0x80044ea4
    try:
        fd = os.open(dev_path, os.O_RDONLY | os.O_NONBLOCK)
        buf = ctypes.c_uint32(0)
        fcntl.ioctl(fd, EVIOCGCLOCKID, buf)
        os.close(fd)
        clock_id = buf.value
        clock_names = {0: "CLOCK_REALTIME", 1: "CLOCK_MONOTONIC", 4: "CLOCK_MONOTONIC_RAW"}
        return clock_id, clock_names.get(clock_id, f"CLOCK_{clock_id}")
    except (OSError, IOError) as e:
        return None, f"unknown (ioctl failed: {e})"


def percentile(sorted_data, p):
    if not sorted_data:
        return 0.0
    k = (len(sorted_data) - 1) * p / 100.0
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return sorted_data[int(k)]
    return sorted_data[f] * (c - k) + sorted_data[c] * (k - f)


def print_histogram(values_us, label, bin_width_us=50):
    """Print an ASCII histogram of latency values (in microseconds)."""
    if not values_us:
        print(f"  {label}: no data")
        return

    sorted_v = sorted(values_us)
    p50 = percentile(sorted_v, 50)
    p95 = percentile(sorted_v, 95)
    p99 = percentile(sorted_v, 99)
    min_v = sorted_v[0]
    max_v = sorted_v[-1]
    mean_v = sum(sorted_v) / len(sorted_v)

    print(f"\n  {label} (n={len(sorted_v)}):")
    print(f"    min={min_v:.1f}us  mean={mean_v:.1f}us  p50={p50:.1f}us  p95={p95:.1f}us  p99={p99:.1f}us  max={max_v:.1f}us")

    # Build histogram bins
    max_bin = int(max_v / bin_width_us) + 1
    # Limit to 30 bins for readability
    if max_bin > 30:
        bin_width_us = int(max_v / 30) + 1
        max_bin = int(max_v / bin_width_us) + 1

    bins = [0] * (max_bin + 1)
    for v in sorted_v:
        idx = int(v / bin_width_us)
        if idx >= len(bins):
            idx = len(bins) - 1
        bins[idx] += 1

    max_count = max(bins) if bins else 1
    bar_width = 40

    for i, count in enumerate(bins):
        if count == 0:
            continue
        low = i * bin_width_us
        high = (i + 1) * bin_width_us
        bar_len = int(count / max_count * bar_width)
        bar = "#" * bar_len
        pct = count / len(sorted_v) * 100
        print(f"    [{low:7.0f}-{high:7.0f})us | {bar:<{bar_width}} | {count:5d} ({pct:5.1f}%)")


def main():
    parser = argparse.ArgumentParser(description="Trace keyboard input latency")
    parser.add_argument("--duration", type=int, default=0, help="Auto-stop after N seconds (0=manual Ctrl+C)")
    parser.add_argument("--device", default="", help="Explicit input device path (e.g. /dev/input/event8)")
    parser.add_argument(
        "--list-devices",
        action="store_true",
        help="List detected MonsGeek/inputd keyboard event devices and exit",
    )
    parser.add_argument(
        "--prefer-wireless",
        action="store_true",
        help="When auto-selecting device, prefer 2.4G event node instead of wired",
    )
    args = parser.parse_args()

    detected = list_monsgeek_event_devices()
    if args.list_devices:
        if not detected:
            print("No MonsGeek keyboard event devices found.")
            sys.exit(1)
        print("Detected MonsGeek keyboard event devices:")
        for name, path in detected:
            print(f"  {name} -> {path}")
        return

    if args.device:
        dev_path = args.device
        if not Path(dev_path).exists():
            print(f"ERROR: input device path does not exist: {dev_path}", file=sys.stderr)
            sys.exit(1)
    else:
        dev_path, detected = find_monsgeek_event_device(prefer_wireless=args.prefer_wireless)
        if not dev_path:
            print("ERROR: No MonsGeek keyboard event device found", file=sys.stderr)
            sys.exit(1)

    if detected:
        print("Detected MonsGeek keyboard event devices:")
        for name, path in detected:
            print(f"  {name} -> {path}")
        print()

    print(f"Device: {dev_path}")
    print()
    print("Type normally. Press Ctrl+C to stop and see latency distribution.")
    if args.duration > 0:
        print(f"Auto-stop after {args.duration} seconds.")
    print()

    # Data collection
    delivery_latencies_us = []      # wall_clock - event_timestamp (microseconds)
    inter_report_us = []            # time between consecutive SYN_REPORTs
    key_press_delivery_us = defaultdict(list)  # per-key delivery latency on press

    last_syn_ts_ns = None           # nanosecond timestamp of last SYN_REPORT
    total_events = 0
    total_reports = 0
    total_key_events = 0
    start_time = time.monotonic()

    try:
        fd = os.open(dev_path, os.O_RDWR)
    except PermissionError:
        print("ERROR: Permission denied. Run with sudo.", file=sys.stderr)
        sys.exit(1)

    # Set the input device clock to CLOCK_MONOTONIC so event timestamps
    # are directly comparable to our CLOCK_MONOTONIC wall-clock reads.
    import fcntl
    EVIOCSCLOCKID = 0x400445A0  # _IOW('E', 0xA0, int)
    clock_val = ctypes.c_uint32(CLOCK_MONOTONIC)
    try:
        fcntl.ioctl(fd, EVIOCSCLOCKID, clock_val)
        print("Set input device clock to CLOCK_MONOTONIC")
    except OSError as e:
        print(f"WARNING: Failed to set CLOCK_MONOTONIC on device: {e}")
        print("         Delivery latency measurements will be invalid.")
        sys.exit(1)

    try:
        while True:
            if args.duration > 0 and (time.monotonic() - start_time) > args.duration:
                break

            data = os.read(fd, EVENT_SIZE)
            # Capture wall-clock IMMEDIATELY after read returns
            wall_ns = monotonic_ns()

            if len(data) < EVENT_SIZE:
                continue

            tv_sec, tv_usec, ev_type, code, value = struct.unpack(EVENT_FMT, data)
            # Convert event timestamp to nanoseconds
            event_ns = tv_sec * 1_000_000_000 + tv_usec * 1_000
            total_events += 1

            if ev_type == EV_SYN and code == SYN_REPORT:
                total_reports += 1

                # Delivery latency: how long between kernel timestamp and our read
                delivery_us = (wall_ns - event_ns) / 1000.0
                delivery_latencies_us.append(delivery_us)

                # Inter-report interval
                if last_syn_ts_ns is not None:
                    gap_us = (event_ns - last_syn_ts_ns) / 1000.0
                    inter_report_us.append(gap_us)

                last_syn_ts_ns = event_ns

                # Live output every 100 reports
                if total_reports % 100 == 0:
                    recent = delivery_latencies_us[-100:]
                    avg = sum(recent) / len(recent)
                    mx = max(recent)
                    print(f"  [{total_reports} reports] avg delivery: {avg:.0f}us  max: {mx:.0f}us  "
                          f"({total_key_events} key events)", end="\r")

            elif ev_type == EV_KEY:
                total_key_events += 1
                delivery_us = (wall_ns - event_ns) / 1000.0

                if value == 1:  # press
                    key_press_delivery_us[code].append(delivery_us)

                # Print individual events with latency
                action = {0: "UP", 1: "DN", 2: "RP"}[value] if value in (0, 1, 2) else f"v{value}"
                print(f"  {key_name(code):>8} {action}  event_t={tv_sec}.{tv_usec:06d}  "
                      f"delivery={delivery_us:8.1f}us  wall_ns={wall_ns}")

    except KeyboardInterrupt:
        pass
    finally:
        os.close(fd)

    elapsed = time.monotonic() - start_time

    # Summary
    print("\n" + "=" * 70)
    print(f"LATENCY TRACE SUMMARY  ({elapsed:.1f}s capture)")
    print("=" * 70)
    print(f"Total events: {total_events}  (SYN reports: {total_reports}, key events: {total_key_events})")
    print()

    # 1. Delivery latency distribution
    print_histogram(delivery_latencies_us, "Delivery latency (kernel→userspace)")

    # 2. Inter-report interval distribution
    # Filter out long gaps (> 100ms) which are just idle time between keystrokes
    active_inter_report = [v for v in inter_report_us if v < 100_000]
    print_histogram(active_inter_report, "Inter-report interval (active typing, <100ms gaps)")

    # 3. Per-key delivery latency
    print("\n  Per-key delivery latency (press events only):")
    print(f"    {'Key':>8}  {'n':>5}  {'min':>8}  {'mean':>8}  {'p50':>8}  {'p95':>8}  {'max':>8}  (us)")
    for code in sorted(key_press_delivery_us.keys()):
        vals = sorted(key_press_delivery_us[code])
        n = len(vals)
        if n < 2:
            continue
        mn = vals[0]
        mx = vals[-1]
        avg = sum(vals) / n
        p50 = percentile(vals, 50)
        p95 = percentile(vals, 95)
        print(f"    {key_name(code):>8}  {n:5d}  {mn:8.1f}  {avg:8.1f}  {p50:8.1f}  {p95:8.1f}  {mx:8.1f}")

    # 4. Outlier analysis
    if delivery_latencies_us:
        sorted_d = sorted(delivery_latencies_us)
        p99 = percentile(sorted_d, 99)
        outliers = [v for v in sorted_d if v > p99 * 2]
        if outliers:
            print(f"\n  OUTLIERS: {len(outliers)} events with delivery > {p99*2:.0f}us (2x p99)")
            print(f"    range: {min(outliers):.0f}us - {max(outliers):.0f}us")

    # 5. Interpretation
    print("\n" + "-" * 70)
    print("INTERPRETATION:")
    if delivery_latencies_us:
        sorted_d = sorted(delivery_latencies_us)
        p50 = percentile(sorted_d, 50)
        p95 = percentile(sorted_d, 95)

        if p50 < 100:
            print("  - p50 delivery < 100us: kernel→userspace path is fast")
        elif p50 < 500:
            print("  - p50 delivery 100-500us: normal range for desktop Linux")
        elif p50 < 2000:
            print(f"  - p50 delivery {p50:.0f}us: ELEVATED — scheduler or compositor may be adding latency")
        else:
            print(f"  - p50 delivery {p50:.0f}us: HIGH — significant kernel/scheduler overhead detected")

        if p95 > p50 * 5:
            print(f"  - p95/p50 ratio = {p95/p50:.1f}x: HIGH JITTER — inconsistent latency, likely scheduler contention")
        elif p95 > p50 * 2:
            print(f"  - p95/p50 ratio = {p95/p50:.1f}x: moderate jitter")
        else:
            print(f"  - p95/p50 ratio = {p95/p50:.1f}x: low jitter, consistent delivery")

    if active_inter_report:
        sorted_ir = sorted(active_inter_report)
        p50_ir = percentile(sorted_ir, 50)
        if p50_ir < 1500:
            print(f"  - Inter-report p50 = {p50_ir:.0f}us: polling at ~{1_000_000/p50_ir:.0f}Hz (matches bInterval=1)")
        else:
            print(f"  - Inter-report p50 = {p50_ir:.0f}us: effective rate ~{1_000_000/p50_ir:.0f}Hz (below 1000Hz)")

    print("-" * 70)


if __name__ == "__main__":
    main()
