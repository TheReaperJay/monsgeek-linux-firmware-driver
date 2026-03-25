#!/usr/bin/env python3
"""Monitor keyboard input events and flag anomalies.

Watches /dev/input/event* for a MonsGeek keyboard and logs:
- Same-report multi-key presses (ordering ambiguity)
- Rapid release→repress of same key (potential bounce)
- Duplicate press without intervening release
- Unusually long gaps between press and release

Run: sudo python3 tools/input_monitor.py
Stop: Ctrl+C — prints summary of all anomalies detected.
"""

import ctypes
import os
import struct
import sys
import time
from collections import defaultdict
from pathlib import Path

# linux/input.h constants
EV_KEY = 0x01
EV_SYN = 0x00
SYN_REPORT = 0x00

# input_event struct: time_sec(8) + time_usec(8) + type(2) + code(2) + value(4) = 24 bytes
EVENT_SIZE = 24
EVENT_FMT = "llHHi"

# Thresholds
BOUNCE_WINDOW_MS = 20  # release→repress within this = potential bounce
LONG_HOLD_MS = 5000    # press held longer than this without release = stuck key warning

# Key names (subset — expand as needed)
KEY_NAMES = {
    1: "ESC", 2: "1", 3: "2", 4: "3", 5: "4", 6: "5", 7: "6", 8: "7",
    9: "8", 10: "9", 11: "0", 14: "BACKSPACE", 15: "TAB", 16: "Q", 17: "W",
    18: "E", 19: "R", 20: "T", 21: "Y", 22: "U", 23: "I", 24: "O", 25: "P",
    28: "ENTER", 29: "LCTRL", 30: "A", 31: "S", 32: "D", 33: "F", 34: "G",
    35: "H", 36: "J", 37: "K", 38: "L", 42: "LSHIFT", 44: "Z", 45: "X",
    46: "C", 47: "V", 48: "B", 49: "N", 50: "M", 54: "RSHIFT", 56: "LALT",
    57: "SPACE", 100: "RALT", 97: "RCTRL",
}


def key_name(code):
    return KEY_NAMES.get(code, f"KEY_{code}")


def find_monsgeek_event_device():
    by_id = Path("/dev/input/by-id")
    for link in by_id.iterdir():
        if "MonsGeek_Keyboard-event-kbd" in link.name and "2.4G" not in link.name:
            return str(link.resolve())
    # fallback: try wireless
    for link in by_id.iterdir():
        if "MonsGeek" in link.name and "event-kbd" in link.name:
            return str(link.resolve())
    return None


def main():
    dev_path = find_monsgeek_event_device()
    if not dev_path:
        print("ERROR: No MonsGeek keyboard event device found", file=sys.stderr)
        sys.exit(1)

    print(f"Monitoring: {dev_path}")
    print(f"Bounce window: {BOUNCE_WINDOW_MS}ms")
    print("Type normally. Anomalies will be printed as they occur.")
    print("Press Ctrl+C to stop and see summary.\n")

    # State tracking
    pressed = {}           # keycode → press_timestamp
    last_release = {}      # keycode → release_timestamp
    current_report = []    # events accumulated before SYN_REPORT

    # Anomaly counters
    anomalies = defaultdict(list)
    total_presses = 0
    total_releases = 0
    total_reports = 0

    try:
        fd = os.open(dev_path, os.O_RDONLY)
    except PermissionError:
        print(f"ERROR: Permission denied. Run with sudo.", file=sys.stderr)
        sys.exit(1)

    try:
        while True:
            data = os.read(fd, EVENT_SIZE)
            if len(data) < EVENT_SIZE:
                continue

            tv_sec, tv_usec, ev_type, code, value = struct.unpack(EVENT_FMT, data)
            ts = tv_sec + tv_usec / 1_000_000
            ts_ms = ts * 1000

            if ev_type == EV_KEY:
                current_report.append((ts, code, value))

            elif ev_type == EV_SYN and code == SYN_REPORT:
                total_reports += 1

                # Check for multi-key state changes in single report
                presses_in_report = [(t, c) for t, c, v in current_report if v == 1]
                releases_in_report = [(t, c) for t, c, v in current_report if v == 0]

                if len(presses_in_report) > 1:
                    keys = [key_name(c) for _, c in presses_in_report]
                    msg = f"[SAME-REPORT] {len(presses_in_report)} keys pressed simultaneously: {', '.join(keys)}"
                    print(f"  ** {msg}")
                    anomalies["same_report_press"].append((ts, keys))

                # Process each event
                for ev_ts, ev_code, ev_value in current_report:
                    if ev_value == 1:  # press
                        total_presses += 1

                        # Check: duplicate press without release
                        if ev_code in pressed:
                            elapsed = (ev_ts - pressed[ev_code]) * 1000
                            msg = f"[DUP-PRESS] {key_name(ev_code)} pressed again without release (held {elapsed:.1f}ms)"
                            print(f"  ** {msg}")
                            anomalies["dup_press"].append((ev_ts, ev_code, elapsed))

                        # Check: rapid repress after release (bounce)
                        if ev_code in last_release:
                            gap_ms = (ev_ts - last_release[ev_code]) * 1000
                            if gap_ms < BOUNCE_WINDOW_MS:
                                msg = f"[BOUNCE?] {key_name(ev_code)} repressed {gap_ms:.1f}ms after release"
                                print(f"  ** {msg}")
                                anomalies["bounce"].append((ev_ts, ev_code, gap_ms))

                        pressed[ev_code] = ev_ts

                    elif ev_value == 0:  # release
                        total_releases += 1

                        if ev_code in pressed:
                            hold_ms = (ev_ts - pressed[ev_code]) * 1000
                            del pressed[ev_code]
                        else:
                            msg = f"[PHANTOM-RELEASE] {key_name(ev_code)} released but was not tracked as pressed"
                            print(f"  ** {msg}")
                            anomalies["phantom_release"].append((ev_ts, ev_code))

                        last_release[ev_code] = ev_ts

                current_report = []

    except KeyboardInterrupt:
        pass
    finally:
        os.close(fd)

    # Summary
    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    print(f"Total reports: {total_reports}")
    print(f"Total presses: {total_presses}")
    print(f"Total releases: {total_releases}")
    print()

    if not any(anomalies.values()):
        print("No anomalies detected.")
    else:
        for kind, events in sorted(anomalies.items()):
            print(f"{kind}: {len(events)} occurrences")
            for ev in events[:10]:  # show first 10
                if kind == "same_report_press":
                    _, keys = ev
                    print(f"    keys: {', '.join(keys)}")
                elif kind == "bounce":
                    _, code, gap = ev
                    print(f"    {key_name(code)} gap={gap:.1f}ms")
                elif kind == "dup_press":
                    _, code, elapsed = ev
                    print(f"    {key_name(code)} held={elapsed:.1f}ms")
                elif kind == "phantom_release":
                    _, code = ev
                    print(f"    {key_name(code)}")
            if len(events) > 10:
                print(f"    ... and {len(events) - 10} more")


if __name__ == "__main__":
    main()
