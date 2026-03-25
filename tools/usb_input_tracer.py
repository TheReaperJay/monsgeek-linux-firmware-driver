#!/usr/bin/env python3
"""Dual-source input latency tracer: usbmon + /dev/input correlation.

Captures USB HID interrupt IN packets from usbmon (raw USB wire) and
correlates them with kernel input events from /dev/input/event* to measure:

1. USB wire → kernel input event latency (HID driver processing time)
2. Kernel input event → userspace read latency (scheduler/delivery)
3. Total: USB wire → userspace read (end-to-end kernel overhead)

Also captures libinput timing for compositor-layer measurement.

Run: sudo python3 tools/usb_input_tracer.py
Stop: Ctrl+C — prints full latency distribution.
"""

import ctypes
import fcntl
import math
import os
import select
import struct
import sys
import threading
import time
from collections import defaultdict
from pathlib import Path

# --- Constants ---
EV_KEY = 0x01
EV_SYN = 0x00
SYN_REPORT = 0x00
EVENT_SIZE = 24
EVENT_FMT = "llHHi"
CLOCK_MONOTONIC = 1
EVIOCSCLOCKID = 0x400445A0

# HID keyboard boot protocol report: 8 bytes
# byte 0: modifier bitmap
# byte 1: reserved
# bytes 2-7: keycodes (up to 6 simultaneous)

KEY_NAMES = {
    1: "ESC", 2: "1", 3: "2", 4: "3", 5: "4", 6: "5", 7: "6", 8: "7",
    9: "8", 10: "9", 11: "0", 14: "BKSP", 15: "TAB", 16: "Q", 17: "W",
    18: "E", 19: "R", 20: "T", 21: "Y", 22: "U", 23: "I", 24: "O", 25: "P",
    26: "[", 27: "]", 28: "ENTER", 29: "LCTRL", 30: "A", 31: "S", 32: "D",
    33: "F", 34: "G", 35: "H", 36: "J", 37: "K", 38: "L", 39: ";", 40: "'",
    41: "`", 42: "LSHIFT", 43: "\\", 44: "Z", 45: "X", 46: "C", 47: "V",
    48: "B", 49: "N", 50: "M", 51: ",", 52: ".", 53: "/", 54: "RSHIFT",
    56: "LALT", 57: "SPACE", 58: "CAPS", 100: "RALT", 97: "RCTRL",
}

# HID usage ID → Linux input keycode mapping (boot protocol subset)
# HID keyboard usage table (0x04 = A, 0x05 = B, ..., 0x1D = Z, etc.)
HID_TO_LINUX = {
    0x04: 30,  # A
    0x05: 48,  # B
    0x06: 46,  # C
    0x07: 32,  # D
    0x08: 18,  # E
    0x09: 33,  # F
    0x0A: 34,  # G
    0x0B: 35,  # H
    0x0C: 23,  # I
    0x0D: 36,  # J
    0x0E: 37,  # K
    0x0F: 38,  # L
    0x10: 50,  # M
    0x11: 49,  # N
    0x12: 24,  # O
    0x13: 25,  # P
    0x14: 16,  # Q
    0x15: 19,  # R
    0x16: 31,  # S
    0x17: 20,  # T
    0x18: 22,  # U
    0x19: 47,  # V
    0x1A: 17,  # W
    0x1B: 45,  # X
    0x1C: 21,  # Y
    0x1D: 44,  # Z
    0x1E: 2,   # 1
    0x1F: 3,   # 2
    0x20: 4,   # 3
    0x21: 5,   # 4
    0x22: 6,   # 5
    0x23: 7,   # 6
    0x24: 8,   # 7
    0x25: 9,   # 8
    0x26: 10,  # 9
    0x27: 11,  # 0
    0x28: 28,  # ENTER
    0x29: 1,   # ESC
    0x2A: 14,  # BACKSPACE
    0x2B: 15,  # TAB
    0x2C: 57,  # SPACE
    0xE0: 29,  # LCTRL
    0xE1: 42,  # LSHIFT
    0xE2: 56,  # LALT
    0xE5: 54,  # RSHIFT
    0xE6: 100, # RALT
    0xE4: 97,  # RCTRL
}


def key_name(code):
    return KEY_NAMES.get(code, f"KEY_{code}")


class Timespec(ctypes.Structure):
    _fields_ = [("tv_sec", ctypes.c_long), ("tv_nsec", ctypes.c_long)]

_librt = ctypes.CDLL("librt.so.1", use_errno=True)
_clock_gettime = _librt.clock_gettime
_clock_gettime.argtypes = [ctypes.c_int, ctypes.POINTER(Timespec)]
_clock_gettime.restype = ctypes.c_int


def monotonic_ns():
    ts = Timespec()
    if _clock_gettime(CLOCK_MONOTONIC, ctypes.byref(ts)) != 0:
        raise OSError(ctypes.get_errno(), "clock_gettime failed")
    return ts.tv_sec * 1_000_000_000 + ts.tv_nsec


def find_monsgeek_event_device():
    by_id = Path("/dev/input/by-id")
    for link in by_id.iterdir():
        if "MonsGeek_Keyboard-event-kbd" in link.name and "2.4G" not in link.name:
            return str(link.resolve())
    for link in by_id.iterdir():
        if "MonsGeek" in link.name and "event-kbd" in link.name:
            return str(link.resolve())
    return None


def find_monsgeek_usb_device():
    """Find MonsGeek wired keyboard bus and device numbers."""
    for dev_dir in Path("/sys/bus/usb/devices").iterdir():
        product_file = dev_dir / "product"
        if not product_file.exists():
            continue
        try:
            product = product_file.read_text().strip()
        except OSError:
            continue
        if "MonsGeek Keyboard" in product and "2.4G" not in product:
            busnum = int((dev_dir / "busnum").read_text().strip())
            devnum = int((dev_dir / "devnum").read_text().strip())
            return busnum, devnum
    return None, None


def parse_hid_report(data_bytes):
    """Parse an 8-byte HID boot protocol keyboard report.

    Returns (modifiers_set, keycodes_set) where keycodes are Linux input keycodes.
    """
    if len(data_bytes) < 8:
        return set(), set()

    modifiers = set()
    mod_byte = data_bytes[0]
    mod_map = {
        0x01: 29,   # LCTRL
        0x02: 42,   # LSHIFT
        0x04: 56,   # LALT
        0x10: 97,   # RCTRL
        0x20: 54,   # RSHIFT
        0x40: 100,  # RALT
    }
    for bit, linux_code in mod_map.items():
        if mod_byte & bit:
            modifiers.add(linux_code)

    keycodes = set()
    for i in range(2, 8):
        hid_code = data_bytes[i]
        if hid_code == 0:
            continue
        if hid_code == 1:  # rollover error
            continue
        linux_code = HID_TO_LINUX.get(hid_code)
        if linux_code is not None:
            keycodes.add(linux_code)

    return modifiers, keycodes


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

    range_v = max_v - min_v
    if range_v <= 0:
        print(f"    [all values = {min_v:.1f}us]")
        return

    max_bin = int(range_v / bin_width_us) + 1
    if max_bin > 30:
        bin_width_us = int(range_v / 30) + 1
        max_bin = int(range_v / bin_width_us) + 1

    bins = [0] * (max_bin + 1)
    for v in sorted_v:
        idx = int((v - min_v) / bin_width_us)
        if idx >= len(bins):
            idx = len(bins) - 1
        bins[idx] += 1

    max_count = max(bins) if bins else 1
    bar_width = 40

    for i, count in enumerate(bins):
        if count == 0:
            continue
        low = min_v + i * bin_width_us
        high = low + bin_width_us
        bar_len = int(count / max_count * bar_width)
        bar = "#" * max(bar_len, 1)
        pct = count / len(sorted_v) * 100
        print(f"    [{low:7.0f}-{high:7.0f})us | {bar:<{bar_width}} | {count:5d} ({pct:5.1f}%)")


class UsbmonReader(threading.Thread):
    """Read usbmon text interface for USB interrupt IN packets from the keyboard."""

    def __init__(self, bus, devnum):
        super().__init__(daemon=True)
        self.bus = bus
        self.devnum = devnum
        self.events = []  # [(monotonic_ns_at_capture, usbmon_full_ns, keycodes_set, raw_bytes)]
        self.lock = threading.Lock()
        self.running = True
        self.error = None

    def run(self):
        usbmon_path = f"/sys/kernel/debug/usb/usbmon/{self.bus}u"
        try:
            fd = os.open(usbmon_path, os.O_RDONLY)
        except OSError as e:
            self.error = f"Cannot open {usbmon_path}: {e}"
            return

        # Calibrate clock: usbmon uses CLOCK_MONOTONIC in microseconds
        # Our monotonic_ns() also uses CLOCK_MONOTONIC, so timestamps should align
        # directly (usbmon_us * 1000 ≈ monotonic_ns at same instant)

        f = os.fdopen(fd, "r")
        try:
            while self.running:
                line = f.readline()
                if not line:
                    continue
                wall_ns = monotonic_ns()
                self._parse_line(line.strip(), wall_ns)
        except Exception as e:
            if self.running:
                self.error = str(e)
        finally:
            f.close()

    def _parse_line(self, line, wall_ns):
        """Parse a usbmon text line.

        Actual format from kernel (verified against live capture):
          parts[0] = URB tag        (hex pointer, e.g. ffff8f428b66a540)
          parts[1] = timestamp      (decimal microseconds, wraps every 4096s)
          parts[2] = event type     (S=submit, C=complete, E=error)
          parts[3] = address        (e.g. Ii:3:048:1)
          parts[4] = status:interval (e.g. 0:1)
          parts[5] = data length    (e.g. 8)
          parts[6] = '='            (data separator, or '<' for submit)
          parts[7+] = data words    (e.g. 00000c00 00000000)
        """
        parts = line.split()
        if len(parts) < 8:
            return

        # Only completed transfers
        if parts[2] != "C":
            return

        # Only interrupt IN on our device's endpoint 1 (IF0 keyboard)
        address = parts[3]
        if not address.startswith("Ii:"):
            return

        addr_parts = address.split(":")
        if len(addr_parts) < 4:
            return

        try:
            bus = int(addr_parts[1])
            dev = int(addr_parts[2])
            ep = int(addr_parts[3])
        except ValueError:
            return

        if bus != self.bus or dev != self.devnum or ep != 1:
            return

        # Parse status (first part of status:interval)
        try:
            status = int(parts[4].split(":")[0])
        except ValueError:
            return
        if status != 0:
            return

        # Parse data length
        try:
            data_len = int(parts[5])
        except ValueError:
            return
        if data_len < 8:
            return

        # Data must follow '=' separator
        if parts[6] != "=":
            return

        data_hex = "".join(parts[7:])
        try:
            data_bytes = bytes.fromhex(data_hex)
        except ValueError:
            return

        if len(data_bytes) < 8:
            return

        # Parse timestamp: decimal microseconds, wraps at (seconds % 4096) * 1e6
        # Reconstruct full CLOCK_MONOTONIC nanoseconds using wall_ns as reference
        try:
            usbmon_us = int(parts[1])
        except ValueError:
            return

        usbmon_full_ns = self._unwrap_timestamp(usbmon_us, wall_ns)

        modifiers, keycodes = parse_hid_report(data_bytes)
        all_keys = modifiers | keycodes

        with self.lock:
            self.events.append((wall_ns, usbmon_full_ns, all_keys, data_bytes[:8]))

    def _unwrap_timestamp(self, usbmon_us, reference_ns):
        """Convert wrapped usbmon timestamp to full CLOCK_MONOTONIC nanoseconds.

        usbmon stamps use: (monotonic_seconds % 4096) * 1_000_000 + microseconds
        So they wrap every 4096 seconds. We use our wall clock reference to
        determine the correct epoch.
        """
        ref_us = reference_ns // 1000
        wrap_us = 4096 * 1_000_000  # 4096 seconds in microseconds

        # Find the epoch multiple that puts usbmon_us closest to ref_us
        # usbmon_us is already (sec%4096)*1e6 + usec, so it's in [0, wrap_us)
        epoch_base = (ref_us // wrap_us) * wrap_us
        full_us = epoch_base + usbmon_us

        # If that puts us too far ahead of reference, go back one epoch
        if full_us > ref_us + 1_000_000:  # more than 1s ahead
            full_us -= wrap_us
        # If too far behind, go forward one epoch
        elif full_us < ref_us - wrap_us + 1_000_000:
            full_us += wrap_us

        return full_us * 1000  # convert to nanoseconds

    def stop(self):
        self.running = False

    def get_events(self):
        with self.lock:
            return list(self.events)


class InputReader(threading.Thread):
    """Read kernel input events from /dev/input/event*."""

    def __init__(self, dev_path):
        super().__init__(daemon=True)
        self.dev_path = dev_path
        self.events = []  # [(wall_ns, event_ts_ns, keycode, value)]
        self.syn_events = []  # [(wall_ns, event_ts_ns)]
        self.lock = threading.Lock()
        self.running = True
        self.error = None

    def run(self):
        try:
            fd = os.open(self.dev_path, os.O_RDWR)
        except OSError as e:
            self.error = f"Cannot open {self.dev_path}: {e}"
            return

        # Set clock to CLOCK_MONOTONIC
        clock_val = ctypes.c_uint32(CLOCK_MONOTONIC)
        try:
            fcntl.ioctl(fd, EVIOCSCLOCKID, clock_val)
        except OSError as e:
            self.error = f"Failed to set CLOCK_MONOTONIC: {e}"
            os.close(fd)
            return

        try:
            while self.running:
                data = os.read(fd, EVENT_SIZE)
                wall_ns = monotonic_ns()
                if len(data) < EVENT_SIZE:
                    continue

                tv_sec, tv_usec, ev_type, code, value = struct.unpack(EVENT_FMT, data)
                event_ns = tv_sec * 1_000_000_000 + tv_usec * 1_000

                if ev_type == EV_KEY:
                    with self.lock:
                        self.events.append((wall_ns, event_ns, code, value))
                elif ev_type == EV_SYN and code == SYN_REPORT:
                    with self.lock:
                        self.syn_events.append((wall_ns, event_ns))
        except Exception as e:
            if self.running:
                self.error = str(e)
        finally:
            os.close(fd)

    def stop(self):
        self.running = False

    def get_key_events(self):
        with self.lock:
            return list(self.events)

    def get_syn_events(self):
        with self.lock:
            return list(self.syn_events)


def correlate_events(usb_events, input_key_events):
    """Correlate USB HID reports with kernel input events.

    For each USB report that shows a new key pressed, find the corresponding
    kernel input event (EV_KEY with value=1 for the same keycode) and measure
    the time delta.

    Returns list of (keycode, usb_ts_ns, input_ts_ns, delta_us, usb_wall_ns, input_wall_ns)
    """
    correlations = []

    # Build a timeline of key presses from USB reports
    prev_keys = set()
    usb_presses = []  # [(usbmon_ts_ns, keycode, wall_ns)]

    for wall_ns, usbmon_ts_ns, all_keys, _ in usb_events:
        new_keys = all_keys - prev_keys
        for keycode in new_keys:
            usb_presses.append((usbmon_ts_ns, keycode, wall_ns))
        prev_keys = all_keys

    # Build a timeline of key presses from input events
    input_presses = []  # [(event_ts_ns, keycode, wall_ns)]
    for wall_ns, event_ns, code, value in input_key_events:
        if value == 1:  # press
            input_presses.append((event_ns, code, wall_ns))

    for usb_ts_ns, usb_keycode, usb_wall_ns in usb_presses:
        best_match = None
        best_delta = float("inf")

        for inp_ts_ns, inp_code, inp_wall_ns in input_presses:
            if inp_code != usb_keycode:
                continue
            # Input event should come AFTER USB packet
            delta_ns = inp_ts_ns - usb_ts_ns
            delta_us = delta_ns / 1000.0
            # Allow -500us to +10000us window (slight clock skew possible)
            if -500 < delta_us < 10000:
                if abs(delta_us) < abs(best_delta):
                    best_delta = delta_us
                    best_match = (inp_ts_ns, inp_wall_ns)

        if best_match is not None:
            inp_ts_ns, inp_wall_ns = best_match
            delta_us = (inp_ts_ns - usb_ts_ns) / 1000.0
            correlations.append((
                usb_keycode,
                usb_ts_ns,
                inp_ts_ns,
                delta_us,
                usb_wall_ns,
                inp_wall_ns,
            ))

    return correlations


def main():
    if os.geteuid() != 0:
        print("ERROR: Must run as root (sudo)", file=sys.stderr)
        sys.exit(1)

    dev_path = find_monsgeek_event_device()
    if not dev_path:
        print("ERROR: No MonsGeek keyboard event device found", file=sys.stderr)
        sys.exit(1)

    bus, devnum = find_monsgeek_usb_device()
    if bus is None:
        print("ERROR: Cannot find MonsGeek USB device in sysfs", file=sys.stderr)
        sys.exit(1)

    print(f"Input device:  {dev_path}")
    print(f"USB device:    bus {bus}, device {devnum}")
    print(f"usbmon source: /sys/kernel/debug/usb/usbmon/{bus}u")
    print()

    # Check usbmon is accessible
    usbmon_path = f"/sys/kernel/debug/usb/usbmon/{bus}u"
    if not os.path.exists(usbmon_path):
        print(f"ERROR: {usbmon_path} does not exist.")
        print("       Run: sudo modprobe usbmon")
        print("       (usbmon is builtin on your kernel but debugfs may need remounting)")
        # Check if we just need to access it differently
        alt_path = f"/dev/usbmon{bus}"
        if os.path.exists(alt_path):
            print(f"       Alternative: {alt_path} exists")
        sys.exit(1)

    print("Starting capture threads...")
    print("Type normally. Press Ctrl+C to stop and see correlation analysis.")
    print()

    # Start both readers
    usb_reader = UsbmonReader(bus, devnum)
    input_reader = InputReader(dev_path)

    usb_reader.start()
    time.sleep(0.1)  # Let usbmon thread initialize

    if usb_reader.error:
        print(f"ERROR: usbmon reader failed: {usb_reader.error}", file=sys.stderr)
        sys.exit(1)

    input_reader.start()
    time.sleep(0.1)

    if input_reader.error:
        print(f"ERROR: input reader failed: {input_reader.error}", file=sys.stderr)
        usb_reader.stop()
        sys.exit(1)

    print("Both readers active. Type to generate events...")
    print()

    start = time.monotonic()
    try:
        while True:
            time.sleep(0.5)
            usb_count = len(usb_reader.get_events())
            input_count = len(input_reader.get_key_events())
            elapsed = time.monotonic() - start
            print(f"\r  [{elapsed:.0f}s] USB reports: {usb_count}  Input events: {input_count}", end="", flush=True)
    except KeyboardInterrupt:
        pass

    print("\n\nStopping readers...")
    usb_reader.stop()
    input_reader.stop()
    time.sleep(0.2)

    usb_events = usb_reader.get_events()
    input_key_events = input_reader.get_key_events()
    input_syn_events = input_reader.get_syn_events()

    print()
    print("=" * 70)
    print("USB + INPUT CORRELATION ANALYSIS")
    print("=" * 70)
    print(f"USB HID reports captured:  {len(usb_events)}")
    print(f"Input key events captured: {len(input_key_events)}")
    print(f"Input SYN reports:         {len(input_syn_events)}")

    if not usb_events:
        print("\nNo USB events captured. Is usbmon working?")
        if usb_reader.error:
            print(f"  Reader error: {usb_reader.error}")
        return

    if not input_key_events:
        print("\nNo input events captured.")
        if input_reader.error:
            print(f"  Reader error: {input_reader.error}")
        return

    # --- Layer 1: USB wire → kernel input event (HID driver processing) ---
    correlations = correlate_events(usb_events, input_key_events)
    hid_deltas = [c[3] for c in correlations]
    if correlations:
        print_histogram(hid_deltas, "Layer 1: USB packet → kernel input event (HID driver processing)")

        # Per-key breakdown
        per_key = defaultdict(list)
        for keycode, _, _, delta_us, _, _ in correlations:
            per_key[keycode].append(delta_us)

        print("\n  Per-key HID processing latency:")
        print(f"    {'Key':>8}  {'n':>5}  {'min':>8}  {'mean':>8}  {'p50':>8}  {'p95':>8}  {'max':>8}  (us)")
        for code in sorted(per_key.keys()):
            vals = sorted(per_key[code])
            n = len(vals)
            if n < 1:
                continue
            mn = vals[0]
            mx = vals[-1]
            avg = sum(vals) / n
            p50 = percentile(vals, 50)
            p95 = percentile(vals, 95) if n >= 2 else mx
            print(f"    {key_name(code):>8}  {n:5d}  {mn:8.1f}  {avg:8.1f}  {p50:8.1f}  {p95:8.1f}  {mx:8.1f}")
    else:
        print("\n  Could not correlate USB and input events.")

    # --- Layer 2: Kernel input event → userspace read (delivery latency) ---
    delivery_us = []
    for wall_ns, event_ns, _, _ in input_key_events:
        d = (wall_ns - event_ns) / 1000.0
        delivery_us.append(d)

    print_histogram(delivery_us, "Layer 2: kernel input event → userspace read (delivery)")

    syn_delivery_us = []
    for wall_ns, event_ns in input_syn_events:
        d = (wall_ns - event_ns) / 1000.0
        syn_delivery_us.append(d)

    print_histogram(syn_delivery_us, "Layer 2b: SYN_REPORT delivery")

    # --- Layer 3: USB inter-packet timing (actual polling rate) ---
    usb_intervals = []
    prev_ts = None
    for _, usbmon_ts_ns, _, _ in usb_events:
        if prev_ts is not None:
            gap_us = (usbmon_ts_ns - prev_ts) / 1000.0
            if gap_us < 100_000:  # Filter idle gaps
                usb_intervals.append(gap_us)
        prev_ts = usbmon_ts_ns

    print_histogram(usb_intervals, "Layer 3: USB inter-packet interval (actual polling rate)")

    # --- Layer 4: Total end-to-end (USB wire → userspace read) ---
    if correlations:
        e2e_us = []
        for _, usb_ts_ns, _, _, _, inp_wall_ns in correlations:
            total = (inp_wall_ns - usb_ts_ns) / 1000.0
            e2e_us.append(total)

        print_histogram(e2e_us, "Layer 4: Total USB wire → userspace read (end-to-end)")

    # --- Interpretation ---
    print("\n" + "-" * 70)
    print("INTERPRETATION:")

    if correlations:
        hid_sorted = sorted(hid_deltas)
        p50_hid = percentile(hid_sorted, 50)
        print(f"  HID driver processing: p50={p50_hid:.0f}us", end="")
        if p50_hid < 100:
            print(" — fast, not a bottleneck")
        elif p50_hid < 500:
            print(" — normal")
        else:
            print(f" — ELEVATED, HID driver is adding {p50_hid:.0f}us")

    if delivery_us:
        del_sorted = sorted(delivery_us)
        p50_del = percentile(del_sorted, 50)
        print(f"  Kernel→userspace delivery: p50={p50_del:.0f}us", end="")
        if p50_del < 200:
            print(" — fast, not a bottleneck")
        elif p50_del < 1000:
            print(" — normal range")
        else:
            print(f" — HIGH, scheduler adding {p50_del:.0f}us")

    if usb_intervals:
        ir_sorted = sorted(usb_intervals)
        p50_ir = percentile(ir_sorted, 50)
        # Check if there's a cluster at ~1ms (1000Hz polling)
        sub_2ms = [v for v in ir_sorted if v < 2000]
        if sub_2ms:
            pct_1ms = len(sub_2ms) / len(ir_sorted) * 100
            print(f"  USB polling: {pct_1ms:.0f}% of intervals < 2ms (1000Hz NAK→ACK polling confirmed)")
        print(f"  Inter-report p50 = {p50_ir:.0f}us (includes idle time between keystrokes)")

    print("-" * 70)


if __name__ == "__main__":
    main()
