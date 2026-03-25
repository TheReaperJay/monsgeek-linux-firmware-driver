#!/usr/bin/env python3
"""Measure compositor pipeline latency: kernel input event → GTK application.

Runs a GTK4 window that captures key events while simultaneously reading
raw kernel input events from /dev/input/event*. Correlates the two streams
by keycode to measure exactly how much time the Wayland compositor
(Mutter/GNOME) adds to keyboard input delivery.

Latency measured = libinput processing + Mutter event dispatch + GTK delivery.

Run: sudo GI_TYPELIB_PATH=/usr/lib64/girepository-1.0 python3 tools/compositor_tracer.py
Stop: Ctrl+C or close the window — prints latency distribution.
"""

import ctypes
import fcntl
import math
import os
import struct
import sys
import threading
import time
from collections import defaultdict
from pathlib import Path

os.environ.setdefault("GI_TYPELIB_PATH", "/usr/lib64/girepository-1.0")

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gtk, Gdk, GLib

# linux/input.h
EV_KEY = 0x01
EV_SYN = 0x00
SYN_REPORT = 0x00
EVENT_SIZE = 24
EVENT_FMT = "llHHi"
CLOCK_MONOTONIC = 1
EVIOCSCLOCKID = 0x400445A0


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

# GDK keyval → Linux keycode mapping (common keys)
# GDK uses X11 keysyms; we map the ones we care about
GDK_TO_LINUX = {}

def _build_gdk_map():
    """Build GDK keyval → Linux keycode mapping from Gdk constants."""
    mapping = {
        Gdk.KEY_Escape: 1, Gdk.KEY_1: 2, Gdk.KEY_2: 3, Gdk.KEY_3: 4,
        Gdk.KEY_4: 5, Gdk.KEY_5: 6, Gdk.KEY_6: 7, Gdk.KEY_7: 8,
        Gdk.KEY_8: 9, Gdk.KEY_9: 10, Gdk.KEY_0: 11,
        Gdk.KEY_BackSpace: 14, Gdk.KEY_Tab: 15,
        Gdk.KEY_q: 16, Gdk.KEY_w: 17, Gdk.KEY_e: 18, Gdk.KEY_r: 19,
        Gdk.KEY_t: 20, Gdk.KEY_y: 21, Gdk.KEY_u: 22, Gdk.KEY_i: 23,
        Gdk.KEY_o: 24, Gdk.KEY_p: 25,
        Gdk.KEY_bracketleft: 26, Gdk.KEY_bracketright: 27,
        Gdk.KEY_Return: 28, Gdk.KEY_Control_L: 29,
        Gdk.KEY_a: 30, Gdk.KEY_s: 31, Gdk.KEY_d: 32, Gdk.KEY_f: 33,
        Gdk.KEY_g: 34, Gdk.KEY_h: 35, Gdk.KEY_j: 36, Gdk.KEY_k: 37,
        Gdk.KEY_l: 38, Gdk.KEY_semicolon: 39, Gdk.KEY_apostrophe: 40,
        Gdk.KEY_grave: 41, Gdk.KEY_Shift_L: 42, Gdk.KEY_backslash: 43,
        Gdk.KEY_z: 44, Gdk.KEY_x: 45, Gdk.KEY_c: 46, Gdk.KEY_v: 47,
        Gdk.KEY_b: 48, Gdk.KEY_n: 49, Gdk.KEY_m: 50,
        Gdk.KEY_comma: 51, Gdk.KEY_period: 52, Gdk.KEY_slash: 53,
        Gdk.KEY_Shift_R: 54, Gdk.KEY_Alt_L: 56, Gdk.KEY_space: 57,
        Gdk.KEY_Caps_Lock: 58, Gdk.KEY_Alt_R: 100, Gdk.KEY_Control_R: 97,
        Gdk.KEY_Up: 103, Gdk.KEY_Left: 105, Gdk.KEY_Right: 106, Gdk.KEY_Down: 108,
        # Uppercase variants (shift held)
        Gdk.KEY_Q: 16, Gdk.KEY_W: 17, Gdk.KEY_E: 18, Gdk.KEY_R: 19,
        Gdk.KEY_T: 20, Gdk.KEY_Y: 21, Gdk.KEY_U: 22, Gdk.KEY_I: 23,
        Gdk.KEY_O: 24, Gdk.KEY_P: 25,
        Gdk.KEY_A: 30, Gdk.KEY_S: 31, Gdk.KEY_D: 32, Gdk.KEY_F: 33,
        Gdk.KEY_G: 34, Gdk.KEY_H: 35, Gdk.KEY_J: 36, Gdk.KEY_K: 37,
        Gdk.KEY_L: 38,
        Gdk.KEY_Z: 44, Gdk.KEY_X: 45, Gdk.KEY_C: 46, Gdk.KEY_V: 47,
        Gdk.KEY_B: 48, Gdk.KEY_N: 49, Gdk.KEY_M: 50,
        Gdk.KEY_exclam: 2, Gdk.KEY_at: 3, Gdk.KEY_numbersign: 4,
        Gdk.KEY_dollar: 5, Gdk.KEY_percent: 6, Gdk.KEY_asciicircum: 7,
        Gdk.KEY_ampersand: 8, Gdk.KEY_asterisk: 9, Gdk.KEY_parenleft: 10,
        Gdk.KEY_parenright: 11,
    }
    GDK_TO_LINUX.update(mapping)

_build_gdk_map()


def key_name(code):
    return KEY_NAMES.get(code, f"KEY_{code}")


def find_monsgeek_event_device():
    by_id = Path("/dev/input/by-id")
    for link in by_id.iterdir():
        if "MonsGeek_Keyboard-event-kbd" in link.name and "2.4G" not in link.name:
            return str(link.resolve())
    for link in by_id.iterdir():
        if "MonsGeek" in link.name and "event-kbd" in link.name:
            return str(link.resolve())
    return None


def percentile(sorted_data, p):
    if not sorted_data:
        return 0.0
    k = (len(sorted_data) - 1) * p / 100.0
    f = math.floor(k)
    c = math.ceil(k)
    if f == c:
        return sorted_data[int(k)]
    return sorted_data[f] * (c - k) + sorted_data[c] * (k - f)


def print_histogram(values_us, label, bin_width_us=100):
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


class KernelInputReader(threading.Thread):
    """Background thread: reads raw kernel input events."""

    def __init__(self, dev_path):
        super().__init__(daemon=True)
        self.dev_path = dev_path
        # Key press events: list of (kernel_event_ns, linux_keycode)
        self.presses = []
        self.lock = threading.Lock()
        self.running = True
        self.error = None

    def run(self):
        try:
            fd = os.open(self.dev_path, os.O_RDWR)
        except OSError as e:
            self.error = f"Cannot open {self.dev_path}: {e}"
            return

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
                if len(data) < EVENT_SIZE:
                    continue

                tv_sec, tv_usec, ev_type, code, value = struct.unpack(EVENT_FMT, data)

                if ev_type == EV_KEY and value == 1:
                    event_ns = tv_sec * 1_000_000_000 + tv_usec * 1_000
                    with self.lock:
                        self.presses.append((event_ns, code))
        except OSError:
            if self.running:
                self.error = "read error"
        finally:
            os.close(fd)

    def stop(self):
        self.running = False

    def get_presses(self):
        with self.lock:
            return list(self.presses)


class CompositorTracerApp(Gtk.Application):
    def __init__(self, kernel_reader):
        super().__init__(application_id="dev.monsgeek.compositor_tracer")
        self.kernel_reader = kernel_reader
        # GTK key press events: list of (wall_clock_ns, linux_keycode)
        self.gtk_presses = []
        self.event_count = 0

    def do_activate(self):
        win = Gtk.ApplicationWindow(application=self, title="Compositor Latency Tracer")
        win.set_default_size(600, 200)

        label = Gtk.Label()
        label.set_markup(
            "<big><b>Type here to measure compositor latency</b></big>\n\n"
            "This window captures key events through the full Wayland compositor pipeline.\n"
            "A background thread simultaneously reads the same events from the kernel.\n"
            "The difference = compositor latency.\n\n"
            "Close window or Ctrl+C to see results."
        )
        label.set_wrap(True)
        win.set_child(label)
        self.label = label

        controller = Gtk.EventControllerKey()
        controller.connect("key-pressed", self._on_key_pressed)
        win.add_controller(controller)

        win.present()

    def _on_key_pressed(self, controller, keyval, keycode, state):
        wall_ns = monotonic_ns()

        # GTK4 on Wayland: keycode is the hardware scan code + 8 (X11 convention)
        # Linux input keycode = keycode - 8
        linux_keycode = keycode - 8

        self.gtk_presses.append((wall_ns, linux_keycode))
        self.event_count += 1

        name = key_name(linux_keycode)
        kernel_count = len(self.kernel_reader.get_presses())
        self.label.set_markup(
            f"<big><b>Events: {self.event_count} (GTK) / {kernel_count} (kernel)</b></big>\n\n"
            f"Last key: {name} (code {linux_keycode})\n"
            f"Close window or Ctrl+C to see results."
        )
        return False


def correlate_and_report(kernel_presses, gtk_presses):
    """Match kernel and GTK press events by keycode and compute latency."""
    # For each GTK press, find the most recent unmatched kernel press with the same keycode
    # The GTK event must arrive AFTER the kernel event
    kernel_by_code = defaultdict(list)
    for event_ns, code in kernel_presses:
        kernel_by_code[code].append(event_ns)

    # Sort each keycode's kernel events
    for code in kernel_by_code:
        kernel_by_code[code].sort()

    # Track consumption index per keycode
    consumed = defaultdict(int)

    compositor_latencies_us = []
    per_key_latencies = defaultdict(list)

    for gtk_ns, gtk_code in gtk_presses:
        candidates = kernel_by_code.get(gtk_code, [])
        idx = consumed[gtk_code]
        if idx >= len(candidates):
            continue

        kernel_ns = candidates[idx]
        consumed[gtk_code] = idx + 1

        delta_us = (gtk_ns - kernel_ns) / 1000.0

        # Sanity check: compositor latency should be positive and < 100ms
        if 0 < delta_us < 100_000:
            compositor_latencies_us.append(delta_us)
            per_key_latencies[gtk_code].append(delta_us)

    matched = len(compositor_latencies_us)
    unmatched_gtk = len(gtk_presses) - matched
    unmatched_kernel = len(kernel_presses) - matched

    print("=" * 70)
    print("COMPOSITOR LATENCY ANALYSIS")
    print("=" * 70)
    print(f"Kernel press events:  {len(kernel_presses)}")
    print(f"GTK press events:     {len(gtk_presses)}")
    print(f"Matched pairs:        {matched}")
    if unmatched_gtk > 0:
        print(f"Unmatched GTK events: {unmatched_gtk}")
    if unmatched_kernel > 0:
        print(f"Unmatched kernel:     {unmatched_kernel}")

    print_histogram(compositor_latencies_us,
                    "Compositor pipeline latency (kernel event → GTK callback)")

    if per_key_latencies:
        print(f"\n  Per-key compositor latency:")
        print(f"    {'Key':>8}  {'n':>5}  {'min':>8}  {'mean':>8}  {'p50':>8}  {'p95':>8}  {'max':>8}  (us)")
        for code in sorted(per_key_latencies.keys()):
            vals = sorted(per_key_latencies[code])
            n = len(vals)
            if n < 2:
                continue
            mn = vals[0]
            mx = vals[-1]
            avg = sum(vals) / n
            p50 = percentile(vals, 50)
            p95 = percentile(vals, 95)
            print(f"    {key_name(code):>8}  {n:5d}  {mn:8.1f}  {avg:8.1f}  {p50:8.1f}  {p95:8.1f}  {mx:8.1f}")

    print("\n" + "-" * 70)
    print("INTERPRETATION:")
    if compositor_latencies_us:
        sorted_c = sorted(compositor_latencies_us)
        p50 = percentile(sorted_c, 50)
        p95 = percentile(sorted_c, 95)

        if p50 < 1000:
            print(f"  - p50 = {p50:.0f}us ({p50/1000:.1f}ms): compositor adds < 1ms — fast")
        elif p50 < 5000:
            print(f"  - p50 = {p50:.0f}us ({p50/1000:.1f}ms): compositor adds {p50/1000:.1f}ms — noticeable")
        elif p50 < 16000:
            print(f"  - p50 = {p50:.0f}us ({p50/1000:.1f}ms): compositor adds ~1 frame of latency")
        else:
            print(f"  - p50 = {p50:.0f}us ({p50/1000:.1f}ms): HIGH — compositor adding multiple frames of latency")

        if p95 > p50 * 3:
            print(f"  - p95/p50 = {p95/p50:.1f}x: HIGH JITTER — inconsistent compositor delivery")
        elif p95 > p50 * 2:
            print(f"  - p95/p50 = {p95/p50:.1f}x: moderate jitter")
        else:
            print(f"  - p95/p50 = {p95/p50:.1f}x: consistent")

        # Compare with previous kernel-only measurement
        print(f"\n  For reference, kernel→userspace delivery was ~100us p50.")
        print(f"  Compositor adds {p50 - 100:.0f}us on top of that.")
    else:
        print("  No matched events — could not measure compositor latency.")
    print("-" * 70)


def main():
    if os.geteuid() != 0:
        print("ERROR: Must run as root (sudo) to read /dev/input/event*", file=sys.stderr)
        print("Run: sudo GI_TYPELIB_PATH=/usr/lib64/girepository-1.0 python3 tools/compositor_tracer.py",
              file=sys.stderr)
        sys.exit(1)

    # GTK on Wayland needs the user's display connection.
    # When running with sudo, inherit WAYLAND_DISPLAY and XDG_RUNTIME_DIR.
    if "WAYLAND_DISPLAY" not in os.environ:
        os.environ["WAYLAND_DISPLAY"] = "wayland-0"
    if "XDG_RUNTIME_DIR" not in os.environ:
        os.environ["XDG_RUNTIME_DIR"] = "/run/user/1000"

    dev_path = find_monsgeek_event_device()
    if not dev_path:
        print("ERROR: No MonsGeek keyboard event device found", file=sys.stderr)
        sys.exit(1)

    print(f"Kernel input: {dev_path}")
    print("Starting kernel reader thread...")

    kernel_reader = KernelInputReader(dev_path)
    kernel_reader.start()
    time.sleep(0.1)

    if kernel_reader.error:
        print(f"ERROR: {kernel_reader.error}", file=sys.stderr)
        sys.exit(1)

    print("Launching GTK window — type in the window to measure compositor latency.")
    print()

    app = CompositorTracerApp(kernel_reader)
    try:
        app.run(None)
    except KeyboardInterrupt:
        pass

    kernel_reader.stop()
    time.sleep(0.1)

    kernel_presses = kernel_reader.get_presses()
    gtk_presses = app.gtk_presses

    print()
    correlate_and_report(kernel_presses, gtk_presses)


if __name__ == "__main__":
    main()
