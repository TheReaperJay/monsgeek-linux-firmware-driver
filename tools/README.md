# Keyboard Input Diagnostic Tools

Diagnostic utilities for tracing and measuring keyboard input latency through the Linux input stack. Built during investigation of input quality issues (ghosting, double characters, wrong key ordering) with MonsGeek keyboards on Linux.

These tools measure latency at each layer of the input pipeline to identify where delays and jitter originate:

```
Physical switch → Firmware → USB wire → Kernel HID driver → Input subsystem → Compositor → Application
                                        ▲                   ▲                  ▲
                                        usb_input_tracer     latency_tracer     compositor_tracer
```

All tools require root access to read from `/dev/input/event*` or `/sys/kernel/debug/usb/usbmon/`.

## Tools

### input_monitor.py

Watches `/dev/input/event*` for a MonsGeek keyboard and flags input anomalies in real time.

**What it detects:**
- **Switch bounce** — rapid release→repress of the same key within a configurable window (default 20ms). Indicates the physical switch is bouncing faster than the firmware debounce can filter.
- **Same-report multi-key presses** — two or more keys changing state in a single USB report. The order within the report is matrix scan order, not physical press order, which causes wrong letter ordering.
- **Duplicate press without release** — a key reporting pressed when it's already tracked as pressed.
- **Phantom releases** — a key releasing when it was never tracked as pressed.

**Usage:**
```
sudo python3 tools/input_monitor.py
```

Type normally. Anomalies print as they occur. Press Ctrl+C for a summary of all anomalies detected during the session.

**Key findings from this tool:** Spacebar switch bounce at 6-12ms passes through the keyboard's 1ms firmware debounce, producing double spaces.

---

### latency_tracer.py

Measures delivery latency from the kernel input subsystem to userspace.

For every input event, captures `CLOCK_MONOTONIC` immediately after `read()` returns and compares it to the kernel's event timestamp (also `CLOCK_MONOTONIC`, set via `EVIOCSCLOCKID` ioctl). The delta is the time between the kernel creating the event and our process reading it — this includes kernel input processing, scheduler wake-up latency, and any buffering.

**What it measures:**
- **Delivery latency** — kernel event timestamp vs. wall-clock read time (per-event, with histogram)
- **Inter-report interval** — time between consecutive `SYN_REPORT` events during active typing
- **Per-key breakdown** — delivery latency grouped by keycode to identify if specific keys are worse

**Usage:**
```
sudo python3 tools/latency_tracer.py [--duration SECONDS]
```

Type normally. Press Ctrl+C (or wait for `--duration`) for the full latency distribution.

**Key findings from this tool:** Kernel→userspace delivery is p50=109us, p95=212us. The kernel input stack is not the bottleneck.

---

### usb_input_tracer.py

Correlates raw USB packets (via usbmon) with kernel input events to measure HID driver processing time — the gap between the USB host controller receiving a HID report and the kernel generating the corresponding input event.

**Requires:** usbmon kernel module loaded (`sudo modprobe usbmon`).

**What it measures (four layers):**
1. **USB packet → kernel input event** — HID driver processing time. How long the kernel takes to parse the HID report and generate input events.
2. **Kernel input event → userspace read** — scheduler/delivery latency (same as `latency_tracer.py`).
3. **USB inter-packet interval** — actual USB polling rate on the wire. Verifies the keyboard is polling at the rate its `bInterval` descriptor claims.
4. **Total end-to-end** — USB wire arrival to userspace read.

**How it works:**
- Reads usbmon text output from `/sys/kernel/debug/usb/usbmon/{bus}u`
- Parses completed interrupt IN transfers for the keyboard's endpoint 1 (IF0 boot protocol)
- Unwraps usbmon's 4096-second wrapping timestamps back to full `CLOCK_MONOTONIC` nanoseconds
- Simultaneously reads `/dev/input/event*` with `CLOCK_MONOTONIC` timestamps
- Matches USB HID reports to kernel input events by keycode and timing window

**Usage:**
```
sudo modprobe usbmon
sudo python3 tools/usb_input_tracer.py
```

Type normally for 30+ seconds. Press Ctrl+C for the correlation analysis.

**Key findings from this tool:** HID driver processing is p50=17us, p95=32us. USB→kernel→userspace end-to-end is p50=104us, max=322us. The entire kernel stack adds under 0.3ms.

---

### compositor_tracer.py

Measures the full Wayland compositor pipeline latency — from kernel input event creation to the application's GTK callback firing.

**Requires:** PyGObject with GTK4 (`pip3 install --user --break-system-packages PyGObject`). Also requires the `cairo-gobject-devel` system package for building PyGObject.

**How it works:**
- Background thread reads raw kernel input events from `/dev/input/event*` with `CLOCK_MONOTONIC` timestamps
- Main thread runs a GTK4 window with an `EventControllerKey` that captures `CLOCK_MONOTONIC` at callback time
- GTK4 on Wayland provides hardware keycodes (X11 convention: Linux keycode + 8), which are converted back for correlation
- After capture, matches kernel press events to GTK press events by keycode sequence and computes the delta

The delta includes: libinput processing, Mutter/GNOME event dispatch, GTK event delivery.

**Usage:**
```
sudo GI_TYPELIB_PATH=/usr/lib64/girepository-1.0 python3 tools/compositor_tracer.py
```

A window opens. Type in the window for 30+ seconds. Close the window or press Ctrl+C for the analysis.

**Key findings from this tool:** Compositor adds p50=342us (fast), but p95=2476us and max=18ms. 12% of keystrokes are delayed 1-18ms by Mutter's frame-clock batching. This jitter, combined with switch bounce, is the root cause of perceived input lag. This finding justified the Phase 5.1 userspace input daemon.

---

## Service Smoke Test

Use the service smoke mode when `monsgeek-driver.service` and `monsgeek-inputd.service` are managed by systemd:

```bash
bash tools/test.sh --service-smoke
```

What it checks:
- `systemctl is-active monsgeek-driver.service`
- `systemctl is-active monsgeek-inputd.service`
- CLI smoke call: `monsgeek-cli info`
