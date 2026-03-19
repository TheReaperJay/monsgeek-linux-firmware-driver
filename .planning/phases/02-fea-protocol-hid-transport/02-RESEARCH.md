# Phase 2: FEA Protocol & HID Transport - Research

**Researched:** 2026-03-19
**Domain:** USB HID transport via rusb (libusb), kernel driver conflict resolution, hot-plug detection, udev rules
**Confidence:** HIGH

## Summary

Phase 2 fills the empty `monsgeek-transport` crate with USB device enumeration, raw HID I/O via `rusb` control transfers to IF2, safety guards (100ms inter-command throttling, stale-read retry with echo matching, key matrix bounds validation), hot-plug detection, and udev rules for non-root access. The M5W firmware (yc3121) has a critical hardware bug: it STALLs on HID report descriptor reads for IF1/IF2, which prevents `hidapi` from working. The solution is `rusb` for direct USB control transfers, bypassing the HID subsystem entirely. The kernel driver conflict (usbhid probing IF1/IF2 causes STALL/timeout/disconnect) requires udev-based driver unbind or HID_QUIRK_IGNORE as fallback.

Two proven reference implementations provide battle-tested patterns: `monsgeek-hid-driver` (Rust, rusb-based, same hardware target) and `monsgeek-akko-linux` (Rust, hidapi-based, architectural patterns for flow control). The monsgeek-hid-driver is the primary reference since it targets the same M5W hardware and uses the same `rusb` approach. Phase 1 already provides all protocol constants, checksum algorithms, command tables, device registry, and timing constants that Phase 2 consumes.

**Primary recommendation:** Follow the monsgeek-hid-driver's rusb pattern for IF2 control transfers, but use the akko reference's layered architecture (raw transport + flow control wrapper) for cleaner separation. The dedicated OS thread design from CONTEXT.md naturally serializes all HID I/O and enforces throttling.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **HID backend:** Use `rusb` (libusb Rust bindings) for USB access -- raw control transfers for HID SET_REPORT/GET_REPORT on IF2. hidapi is NOT viable for MonsGeek keyboards.
- **Kernel driver conflict:** Dual-path approach: try udev unbind first, fall back to HID_QUIRK_IGNORE. Hardware testing during Phase 2 determines which path works.
- **Async model:** Dedicated OS thread owns the rusb handle and processes commands from a channel. All HID I/O serialized through single thread. rusb stays off the tokio runtime.
- **Hot-plug detection:** Built in Phase 2 as part of the transport thread. rusb's `HotplugBuilder` with `device_arrived`/`device_left` callbacks.
- **Hardware validation:** Gated integration test suite: `cargo test --features hardware`. Feature gate keeps hardware tests out of CI.

### Claude's Discretion
- Transport thread channel design (bounded vs unbounded, backpressure strategy)
- Error type design for transport errors
- Echo byte matching implementation details
- Stale-read retry timing and strategy
- Key matrix bounds validation approach (HID-05)
- Integration test structure and organization

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| HID-01 | Driver detects and enumerates all yc3121-based MonsGeek keyboards (VID 0x3141) connected via USB | rusb `devices()` iteration with VID/PID matching against DeviceRegistry; hot-plug via `HotplugBuilder` |
| HID-02 | Driver sends FEA commands and receives responses using 64-byte HID Feature Reports with Bit7 checksums | rusb `write_control`/`read_control` on IF2 with `REQUEST_TYPE_OUT`/`REQUEST_TYPE_IN`, `HID_SET_REPORT`/`HID_GET_REPORT`, `FEATURE_REPORT_WVALUE`; Phase 1's `build_command()` constructs the 65-byte buffer |
| HID-03 | Driver enforces mandatory 100ms inter-command delay to prevent yc3121 firmware crash/stall | Dedicated thread loop sleeps `DEFAULT_DELAY_MS` between commands; natural serialization prevents overlap |
| HID-04 | Driver handles Linux hidraw stale read issue via retry-and-match loop (echo byte verification) | FlowControlTransport pattern: send, delay, read, check `response[0] == cmd_byte`, retry up to `QUERY_RETRIES` times |
| HID-05 | Driver validates all write indices against key matrix bounds before sending to prevent firmware OOB corruption | DeviceDefinition's `key_count` (108 for M5W) and `layer` (4) fields provide bounds; validate before SET_KEYMATRIX |
| HID-06 | udev rules enable non-root HID access for yc3121 keyboards on Linux | udev rule for SUBSYSTEM=="usb", ATTRS{idVendor}=="3141" with MODE="0660", GROUP="plugdev", TAG+="uaccess"; separate rule for IF2 unbind |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusb | 0.9.4 | libusb Rust bindings for USB control transfers | Only viable option for MonsGeek -- hidapi fails due to firmware STALL bug. Reference project uses same approach. |
| thiserror | 2.0.18 | Derive Error trait for transport error types | Already used in monsgeek-protocol. Project standard. |
| log | 0.4.29 | Logging facade | Reference project uses this. Lightweight, no runtime dependency. Phase 3 can add tracing subscriber later. |
| crossbeam-channel | 0.5.15 | MPSC channel for transport thread communication | Bounded channels with select, better than std::sync::mpsc for backpressure and multi-producer use |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| monsgeek-protocol | workspace | Protocol constants, checksums, device registry, timing | Always -- Phase 2 depends on Phase 1 |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| crossbeam-channel | std::sync::mpsc | std mpsc lacks bounded channels and select; crossbeam is the de facto standard |
| log | tracing | tracing is heavier; log is sufficient for transport layer; Phase 3 can bridge log->tracing |

**Installation:**
```toml
[dependencies]
monsgeek-protocol = { path = "../monsgeek-protocol" }
rusb = "0.9"
thiserror = "2.0"
log = "0.4"
crossbeam-channel = "0.5"

[features]
hardware = []  # Gated integration tests against real hardware
```

## Architecture Patterns

### Recommended Project Structure
```
crates/monsgeek-transport/
  src/
    lib.rs            # Public API: TransportHandle, DeviceInfo, TransportError
    error.rs          # TransportError enum (thiserror)
    usb.rs            # UsbSession: rusb device open, IF2 claim, control transfers
    thread.rs         # Transport thread: command loop, throttling, hot-plug
    flow_control.rs   # FlowControlTransport: retry, echo matching, stale-read handling
    discovery.rs      # Device enumeration via rusb::devices() + DeviceRegistry
    bounds.rs         # Key matrix bounds validation (HID-05)
  tests/
    hardware.rs       # #[cfg(feature = "hardware")] integration tests
  deploy/
    99-monsgeek.rules # udev rules for non-root access and IF2 unbind
```

### Pattern 1: Dedicated Transport Thread with Channel
**What:** A single OS thread owns the `rusb::DeviceHandle` and processes commands from a bounded channel. Callers send `(Command, oneshot::Sender<Result>)` tuples and await responses.
**When to use:** Always -- this is the core architecture decision from CONTEXT.md.
**Why:** Natural serialization eliminates race conditions. 100ms throttle is just `thread::sleep` in the loop. No Send/Sync issues with DeviceHandle. Clean shutdown via channel close.
**Example:**
```rust
// Source: CONTEXT.md decision + monsgeek-hid-driver/src/vendor.rs pattern
use crossbeam_channel::{bounded, Receiver, Sender};
use std::time::{Duration, Instant};

pub struct CommandRequest {
    pub cmd: u8,
    pub data: Vec<u8>,
    pub checksum: monsgeek_protocol::ChecksumType,
    pub response_tx: crossbeam_channel::Sender<Result<Vec<u8>, TransportError>>,
}

fn transport_thread(
    rx: Receiver<CommandRequest>,
    session: UsbSession,
) {
    let mut last_command = Instant::now() - Duration::from_millis(200);

    while let Ok(req) = rx.recv() {
        // Enforce minimum inter-command delay
        let elapsed = last_command.elapsed();
        let min_delay = Duration::from_millis(monsgeek_protocol::timing::DEFAULT_DELAY_MS);
        if elapsed < min_delay {
            std::thread::sleep(min_delay - elapsed);
        }

        let result = execute_command(&session, &req);
        last_command = Instant::now();
        let _ = req.response_tx.send(result);
    }
}
```

### Pattern 2: Layered Transport (Raw I/O + Flow Control)
**What:** Separate raw USB I/O (`UsbSession`) from flow control logic (`FlowControlTransport`). The raw layer does exactly one send or read. The flow control layer adds retries, echo matching, and stale-read handling.
**When to use:** Follows akko reference's proven separation. Keeps USB-specific code isolated from protocol logic.
**Example:**
```rust
// Source: monsgeek-akko-linux flow_control.rs pattern, adapted for rusb
impl FlowControlTransport {
    /// Send command and wait for echoed response (validates cmd byte match).
    pub fn query_command(
        &self,
        cmd_byte: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<Vec<u8>, TransportError> {
        for attempt in 0..timing::QUERY_RETRIES {
            // Build and send command
            let frame = build_command(cmd_byte, data, checksum);
            self.session.vendor_set_report(&frame)?;

            // Wait for firmware to process
            std::thread::sleep(Duration::from_millis(timing::DEFAULT_DELAY_MS));

            // Read response and check echo byte
            let mut response = [0u8; 64];
            self.session.vendor_get_report(&mut response)?;

            if response[0] == cmd_byte {
                return Ok(response.to_vec());
            }
            log::debug!(
                "Echo mismatch attempt {}: expected 0x{:02X}, got 0x{:02X}",
                attempt, cmd_byte, response[0]
            );
        }
        Err(TransportError::EchoMismatch { expected: cmd_byte })
    }
}
```

### Pattern 3: USB Control Transfer for HID Feature Reports
**What:** Use `rusb::DeviceHandle::write_control()` / `read_control()` with HID class request constants to send/receive 64-byte feature reports on IF2.
**When to use:** All HID communication. This bypasses the kernel HID subsystem entirely.
**Example:**
```rust
// Source: monsgeek-hid-driver/src/device.rs (verified against real M5W)
const HID_SET_REPORT: u8 = 0x09;
const HID_GET_REPORT: u8 = 0x01;
const FEATURE_REPORT_WVALUE: u16 = 0x0300;  // Report Type = Feature (3), Report ID = 0
const REQUEST_TYPE_OUT: u8 = 0x21;  // Host-to-device, class, interface
const REQUEST_TYPE_IN: u8 = 0xA1;   // Device-to-host, class, interface
const IF2: u16 = 2;
const USB_TIMEOUT: Duration = Duration::from_secs(1);

// SET_REPORT (send command to keyboard)
handle.write_control(
    REQUEST_TYPE_OUT, HID_SET_REPORT, FEATURE_REPORT_WVALUE,
    IF2, &frame_64_bytes, USB_TIMEOUT
)?;

// GET_REPORT (read response from keyboard)
let mut buf = [0u8; 64];
handle.read_control(
    REQUEST_TYPE_IN, HID_GET_REPORT, FEATURE_REPORT_WVALUE,
    IF2, &mut buf, USB_TIMEOUT
)?;
```

### Pattern 4: Hot-Plug via rusb HotplugBuilder
**What:** Register a hotplug callback filtered by VID to detect keyboard connect/disconnect.
**When to use:** Device lifecycle management in the transport thread.
**Critical safety note:** The Hotplug callback can only safely call methods taking `Device<T>` (not `DeviceHandle`). Device opening must happen outside the callback.
**Example:**
```rust
// Source: rusb/examples/hotplug.rs + docs.rs/rusb/0.9.4
use rusb::{Context, Device, HotplugBuilder, UsbContext};

struct DeviceWatcher {
    event_tx: Sender<HotplugEvent>,
}

impl<T: UsbContext> rusb::Hotplug<T> for DeviceWatcher {
    fn device_arrived(&mut self, device: Device<T>) {
        if let Ok(desc) = device.device_descriptor() {
            if desc.vendor_id() == 0x3141 {
                let _ = self.event_tx.send(HotplugEvent::Arrived {
                    vid: desc.vendor_id(),
                    pid: desc.product_id(),
                    bus: device.bus_number(),
                    address: device.address(),
                });
            }
        }
    }

    fn device_left(&mut self, device: Device<T>) {
        let _ = self.event_tx.send(HotplugEvent::Left {
            bus: device.bus_number(),
            address: device.address(),
        });
    }
}

// Registration (in transport thread)
if rusb::has_hotplug() {
    let context = Context::new()?;
    let _reg = HotplugBuilder::new()
        .vendor_id(0x3141)
        .enumerate(true)  // Get initial devices too
        .register(&context, Box::new(DeviceWatcher { event_tx }))?;

    // Event loop
    loop {
        context.handle_events(Some(Duration::from_millis(100)))?;
    }
}
```

### Anti-Patterns to Avoid
- **Using hidapi for MonsGeek keyboards:** The M5W firmware STALLs on HID descriptor reads for IF1/IF2. hidapi's hidraw backend fails completely; its libusb backend returns "hid_error not implemented". This is a MonsGeek firmware bug, not a hidapi limitation.
- **Sending commands without inter-command delay:** The yc3121 firmware's EP0 ring buffer cannot handle overlapping requests. A second SET_REPORT before the first is consumed causes the USB peripheral to NAK indefinitely.
- **Assuming GET_REPORT returns fresh data after SET_REPORT:** The M5W firmware does NOT update the GET_REPORT response buffer for SET commands. GET_REPORT returns the result of the last GET command. SET-then-GET requires sending the matching GET command after SET, not just reading the buffer.
- **Using DeviceHandle methods inside Hotplug callbacks:** rusb explicitly documents that synchronous API functions and blocking descriptor retrieval are NOT safe inside hotplug callbacks. Only `Device<T>` methods are safe.
- **Forgetting to detach kernel drivers before claiming interfaces:** If usbhid is bound to any interface, `claim_interface` will fail with `EBUSY`. Must check `kernel_driver_active()` and `detach_kernel_driver()` first.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| USB control transfers | Custom USB protocol implementation | rusb `write_control`/`read_control` | libusb handles all USB protocol details, timeouts, error recovery |
| HID Feature Report framing | Manual USB descriptor parsing | Constants from reference project (`FEATURE_REPORT_WVALUE=0x0300`, `REQUEST_TYPE_OUT=0x21`, `REQUEST_TYPE_IN=0xA1`) | These are HID class standard values, verified working on M5W |
| Command checksums | Custom checksum in transport | `monsgeek_protocol::checksum::build_command()` | Already implemented and tested in Phase 1 |
| Device identification | Custom VID/PID lookup | `monsgeek_protocol::DeviceRegistry` | Already implemented in Phase 1 with JSON extensibility |
| Timing constants | Hardcoded delay values | `monsgeek_protocol::timing::*` | Centralized constants from Phase 1 |
| Multi-producer channel | Custom synchronization | `crossbeam-channel` bounded channel | Battle-tested, correct backpressure semantics |

**Key insight:** Phase 1 already built all protocol-level abstractions. Phase 2 should consume them, not duplicate them. The transport layer is purely about USB I/O, timing, and reliability -- the protocol knowledge lives in `monsgeek-protocol`.

## Common Pitfalls

### Pitfall 1: Kernel Driver Conflict on IF2
**What goes wrong:** When the M5W is plugged in, `usbhid` probes all three interfaces (IF0, IF1, IF2). IF1 and IF2 have firmware that STALLs on HID report descriptor reads, causing 25-second timeouts followed by EPROTO, which resets the entire USB port and disconnects the keyboard.
**Why it happens:** The yc3121 firmware does not implement HID report descriptors for IF1/IF2. The kernel's usbhid driver assumes all HID interfaces have valid descriptors.
**How to avoid:** Dual-path approach per CONTEXT.md: (1) udev rule unbinds usbhid from IF2 after probe: `ACTION=="add", SUBSYSTEM=="usb", DRIVER=="usbhid", ATTRS{idVendor}=="3141", ATTRS{idProduct}=="4005", ATTR{bInterfaceNumber}=="02", RUN+="/bin/sh -c 'echo -n %k > /sys/bus/usb/drivers/usbhid/unbind'"`. (2) If the STALL causes full port reset before unbind fires, use `options usbhid quirks=0x3141:0x4005:0x0010` in modprobe.d.
**Warning signs:** Device disappears from `lsusb` shortly after being plugged in. `dmesg` shows STALL/timeout errors on IF1/IF2. `rusb::devices()` finds the device intermittently.

### Pitfall 2: 65-Byte vs 64-Byte Buffer Convention
**What goes wrong:** Sending wrong-sized buffers to `write_control` or `read_control`.
**Why it happens:** The HID spec requires Report ID as byte 0 when using hidraw/hidapi (65 bytes: [report_id, payload...]). But rusb control transfers send Report ID via wValue (the `FEATURE_REPORT_WVALUE=0x0300`), so the data payload is exactly 64 bytes with NO report ID prefix.
**How to avoid:** Phase 1's `build_command()` returns a 65-byte Vec with `buf[0]=0` (report ID). For rusb control transfers, send `&buf[1..]` (64 bytes) to `write_control`. For `read_control`, use a 64-byte buffer.
**Warning signs:** USB timeout on SET_REPORT. Garbled responses where first byte looks like report ID 0x00.

### Pitfall 3: Stale GET_REPORT Response
**What goes wrong:** After a SET command, the next GET_REPORT returns data from the previous GET command, not the SET result.
**Why it happens:** The M5W firmware's GET_REPORT response buffer is only updated when a GET command is processed. SET commands do NOT update this buffer. This is documented in the reference implementations.
**How to avoid:** After SET, send the corresponding GET command and read the response. Use echo byte matching (`response[0] == cmd_byte`) to verify you got the right response. Retry up to `QUERY_RETRIES` (5) times.
**Warning signs:** SET_DEBOUNCE appears to work but GET_DEBOUNCE returns the old value. Or GET_DEBOUNCE returns a response with echo byte from a different command.

### Pitfall 4: USB Reset Changes Device Address
**What goes wrong:** After calling `handle.reset()`, the device's USB address may change, making the existing `Device` / `DeviceHandle` invalid.
**Why it happens:** USB bus re-enumeration after reset may assign a new address.
**How to avoid:** The monsgeek-hid-driver pattern: after reset, drop the handle, wait 500ms for re-enumeration, then re-find the device by VID/PID. See `UsbSession::open()` in the reference.
**Warning signs:** `rusb::Error::NoDevice` after reset.

### Pitfall 5: HotplugBuilder Safety Constraints
**What goes wrong:** Calling synchronous USB functions (descriptor reads, control transfers) inside the `device_arrived` / `device_left` callbacks causes deadlocks or undefined behavior.
**Why it happens:** rusb's hotplug callbacks run inside libusb's event handling context. The documentation explicitly states that only `Device<T>` methods are safe; `DeviceHandle` methods (including all control transfers) are NOT safe in this context.
**How to avoid:** Use hotplug callbacks only to send notification events (VID, PID, bus, address) through a channel. The transport thread receives these events and performs device opening/closing outside the callback context.
**Warning signs:** Application hangs when keyboard is plugged in. Deadlock in libusb event loop.

### Pitfall 6: SET_DEBOUNCE Command Byte Confusion
**What goes wrong:** Sending the wrong command byte for SET_DEBOUNCE causes the keyboard to enter sleep mode and appear bricked.
**Why it happens:** The M5W is YiChip family (yc3121). YiChip uses `0x11` for SET_DEBOUNCE, but RY5088 uses `0x06` for SET_DEBOUNCE. RY5088's `0x11` is SET_SLEEPTIME. The monsgeek-hid-driver reference project originally mapped SET_DEBOUNCE to `0x11` for RY5088 keyboards, which worked correctly on that family. But applying the same value to a YiChip keyboard would need to use the YiChip command table.
**How to avoid:** Use `ProtocolFamily::detect()` from Phase 1 to determine the correct command table, then use `commands().set_debounce` for the family-specific byte. For M5W (YiChip): SET_DEBOUNCE=0x11, GET_DEBOUNCE=0x91.
**Warning signs:** Keyboard goes to sleep immediately after debounce command. Need to verify correct mapping on real hardware during integration testing.

## Code Examples

Verified patterns from reference implementations:

### USB Session Opening (from monsgeek-hid-driver/src/device.rs)
```rust
// Source: references/monsgeek-hid-driver/src/device.rs
// Verified working on M5W hardware

pub fn open(vid: u16, pid: u16) -> Result<UsbSession, TransportError> {
    let device = find_device(vid, pid)?;

    // Optional: Reset to clear kernel probe corruption on IF2
    // Only needed if udev unbind path is not working

    let handle = device.open()?;

    // Detach kernel drivers and claim interfaces
    for iface in [0u8, 1, 2] {
        match handle.kernel_driver_active(iface) {
            Ok(true) => { handle.detach_kernel_driver(iface)?; }
            Ok(false) | Err(_) => {}
        }
        handle.claim_interface(iface)?;
    }

    Ok(UsbSession { handle })
}
```

### Feature Report I/O (from monsgeek-hid-driver/src/device.rs)
```rust
// Source: references/monsgeek-hid-driver/src/device.rs
const HID_SET_REPORT: u8 = 0x09;
const HID_GET_REPORT: u8 = 0x01;
const FEATURE_REPORT_WVALUE: u16 = 0x0300;
const REQUEST_TYPE_OUT: u8 = 0x21;
const REQUEST_TYPE_IN: u8 = 0xA1;
const IF2: u16 = 2;
const USB_TIMEOUT: Duration = Duration::from_secs(1);

/// Send a 64-byte command to IF2 via SET_REPORT.
pub fn vendor_set_report(&self, data: &[u8]) -> Result<(), TransportError> {
    self.handle.write_control(
        REQUEST_TYPE_OUT, HID_SET_REPORT, FEATURE_REPORT_WVALUE,
        IF2, data, USB_TIMEOUT
    )?;
    Ok(())
}

/// Read a 64-byte response from IF2 via GET_REPORT.
pub fn vendor_get_report(&self, buf: &mut [u8; 64]) -> Result<(), TransportError> {
    self.handle.read_control(
        REQUEST_TYPE_IN, HID_GET_REPORT, FEATURE_REPORT_WVALUE,
        IF2, buf, USB_TIMEOUT
    )?;
    Ok(())
}
```

### Query with Echo Matching (from monsgeek-akko-linux flow_control.rs)
```rust
// Source: references/monsgeek-akko-linux/.../flow_control.rs (adapted for rusb)
pub fn query_command(
    session: &UsbSession,
    cmd_byte: u8,
    data: &[u8],
    checksum: ChecksumType,
) -> Result<Vec<u8>, TransportError> {
    for attempt in 0..monsgeek_protocol::timing::QUERY_RETRIES {
        // Build 65-byte buffer (Phase 1), send 64-byte payload (skip report ID)
        let frame = monsgeek_protocol::build_command(cmd_byte, data, checksum);
        session.vendor_set_report(&frame[1..])?;

        std::thread::sleep(Duration::from_millis(
            monsgeek_protocol::timing::DEFAULT_DELAY_MS
        ));

        let mut response = [0u8; 64];
        session.vendor_get_report(&mut response)?;

        if response[0] == cmd_byte {
            return Ok(response.to_vec());
        }
        log::debug!(
            "Echo mismatch (attempt {}): expected 0x{:02X}, got 0x{:02X}",
            attempt, cmd_byte, response[0]
        );
    }
    Err(TransportError::EchoMismatch { expected: cmd_byte })
}
```

### Udev Rules (from reference project pattern + keyboard community practice)
```
# /etc/udev/rules.d/99-monsgeek.rules
#
# Allow non-root access to MonsGeek keyboards via libusb.
# Also unbind usbhid from IF2 (vendor interface) to prevent STALL.

# Grant non-root USB device access for MonsGeek VID
SUBSYSTEM=="usb", ATTRS{idVendor}=="3141", MODE="0660", GROUP="plugdev", TAG+="uaccess"

# Unbind usbhid from IF2 to prevent firmware STALL on descriptor read
ACTION=="add", SUBSYSTEM=="usb", DRIVER=="usbhid", ATTRS{idVendor}=="3141", ATTRS{idProduct}=="4005", ATTR{bInterfaceNumber}=="02", RUN+="/bin/sh -c 'echo -n %k > /sys/bus/usb/drivers/usbhid/unbind'"
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| hidapi for all keyboards | rusb for MonsGeek (firmware STALL bug) | Discovered during monsgeek-hid-driver development | hidapi cannot access IF2 on MonsGeek keyboards; rusb bypasses HID subsystem entirely |
| Single global delay between commands | Per-command throttle with minimum inter-command interval | Standard in akko reference | More responsive: only waits the remaining time if delay has partially elapsed |
| USB reset on every open | Conditional reset only if kernel probe corrupted IF2 | monsgeek-hid-driver evolution | Faster device opening when udev unbind works; reset is fallback |
| rusb 0.8 hotplug API | rusb 0.9 hotplug API | rusb 0.9.0 release | HotplugBuilder pattern unchanged; Registration type for unregistering |

**Deprecated/outdated:**
- rusb 0.8 and earlier: Use 0.9.4 for latest fixes and consistent API

## Open Questions

1. **IF2 unbind vs HID_QUIRK_IGNORE: which path works?**
   - What we know: The monsgeek-hid-driver uses HID_QUIRK_IGNORE (claims all interfaces, manages IF0 input via uinput). The CONTEXT.md proposes trying udev unbind first (less invasive -- only unbinds IF2, leaves IF0 keyboard working normally).
   - What's unclear: Whether the M5W firmware STALL on IF1/IF2 causes a full USB port reset before the udev unbind rule has a chance to fire. If it does, the unbind path is useless and HID_QUIRK_IGNORE is the only option.
   - Recommendation: Implement both paths. First integration test validates udev unbind approach. If it fails (device disappears from lsusb during probe), fall back to HID_QUIRK_IGNORE with clear documentation.

2. **SET_DEBOUNCE command byte verification on M5W hardware**
   - What we know: The M5W is YiChip family. Phase 1's YICHIP_COMMANDS has `set_debounce: 0x11` and `get_debounce: 0x91`. The monsgeek-hid-driver reference uses `SET_DEBOUNCE=0x06` (RY5088 mapping) because it was originally tested on RY5088 hardware.
   - What's unclear: Whether the M5W actually responds correctly to `0x11` for SET_DEBOUNCE or if it interprets it as something else.
   - Recommendation: Integration test should verify: send `GET_DEBOUNCE` (0x91), read current value, send `SET_DEBOUNCE` (0x11) with new value, send `GET_DEBOUNCE` again, verify value changed. If this fails, try the RY5088 mapping (0x06) as fallback.

3. **Channel backpressure strategy**
   - What we know: The transport thread processes one command at a time with 100ms minimum delay. With bounded channel, senders block when channel is full.
   - What's unclear: What capacity provides the right balance between responsiveness and memory.
   - Recommendation: Bounded channel with capacity 16 (matches akko reference's `REQUEST_QUEUE_SIZE`). This allows ~1.6 seconds of queued commands at 100ms spacing, which is enough for burst scenarios without excessive memory.

4. **Whether to claim IF0 and IF1**
   - What we know: The monsgeek-hid-driver claims all 3 interfaces and manages IF0 keyboard input via uinput. Our driver only needs IF2 for vendor commands.
   - What's unclear: Whether claiming only IF2 (leaving IF0/IF1 to kernel) works with the udev unbind approach.
   - Recommendation: With udev unbind path, only claim IF2 (let kernel manage IF0 for keyboard input). With HID_QUIRK_IGNORE fallback, must claim all 3 interfaces since no kernel driver is bound.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | Cargo.toml `[features] hardware = []` |
| Quick run command | `cargo test -p monsgeek-transport` |
| Full suite command | `cargo test --workspace` |
| Hardware test command | `cargo test -p monsgeek-transport --features hardware` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| HID-01 | Device enumeration (VID 0x3141 detection) | unit + hardware | `cargo test -p monsgeek-transport test_enumerate` | No -- Wave 0 |
| HID-02 | FEA command send/receive (GET_USB_VERSION) | hardware | `cargo test -p monsgeek-transport --features hardware test_get_usb_version` | No -- Wave 0 |
| HID-03 | 100ms inter-command throttle | unit | `cargo test -p monsgeek-transport test_throttle` | No -- Wave 0 |
| HID-04 | Stale-read retry with echo matching | unit + hardware | `cargo test -p monsgeek-transport test_echo_matching` | No -- Wave 0 |
| HID-05 | Key matrix bounds validation | unit | `cargo test -p monsgeek-transport test_bounds_validation` | No -- Wave 0 |
| HID-06 | Udev rules file exists and is syntactically valid | unit (file check) | `cargo test -p monsgeek-transport test_udev_rules` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p monsgeek-transport`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green + hardware tests pass on real M5W

### Wave 0 Gaps
- [ ] `crates/monsgeek-transport/tests/hardware.rs` -- gated integration tests for HID-01, HID-02, HID-04
- [ ] Unit tests for throttle timing (HID-03), bounds validation (HID-05)
- [ ] `deploy/99-monsgeek.rules` -- udev rules file (HID-06)
- [ ] Framework install: already available (cargo test built-in)

## Sources

### Primary (HIGH confidence)
- `references/monsgeek-hid-driver/src/device.rs` -- UsbSession with rusb control transfers, IF claiming, USB reset pattern. Verified working on M5W hardware.
- `references/monsgeek-hid-driver/src/vendor.rs` -- Inter-command delay, EP0 FIFO architecture from firmware disassembly, query/send pattern.
- `references/monsgeek-hid-driver/src/protocol.rs` -- Bit7 checksum, command frame building, echo validation, command constants.
- `references/monsgeek-hid-driver/deploy/monsgeek-hid-usbhid.conf` -- HID_QUIRK_IGNORE modprobe config.
- `references/monsgeek-akko-linux/.../flow_control.rs` -- FlowControlTransport: retry logic, echo matching, throttling pattern.
- `references/monsgeek-akko-linux/.../hid_wired.rs` -- HidWiredTransport: feature report send/read, Mutex-wrapped device.
- `references/monsgeek-akko-linux/.../discovery.rs` -- Device discovery, VID/PID filtering, interface matching.
- `crates/monsgeek-protocol/` -- Phase 1 output: all protocol constants, checksums, device registry, timing constants.
- [rusb docs.rs](https://docs.rs/rusb/0.9.4/) -- DeviceHandle API, HotplugBuilder, safety constraints.
- [rusb hotplug.rs source](https://github.com/a1ien/rusb/blob/master/src/hotplug.rs) -- Hotplug trait definition, HotplugBuilder implementation.

### Secondary (MEDIUM confidence)
- [rusb hotplug example](https://github.com/a1ien/rusb/blob/master/examples/hotplug.rs) -- Working hotplug registration example.
- [Vial udev rules](https://get.vial.today/manual/linux-udev.html) -- Community-standard udev rule patterns for keyboard HID access.
- [crates.io rusb](https://crates.io/crates/rusb) -- Version 0.9.4 confirmed current.

### Tertiary (LOW confidence)
- None -- all findings verified against primary sources.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- rusb 0.9.4 verified on crates.io, all other crates verified. Reference project uses identical stack.
- Architecture: HIGH -- Dedicated thread + channel pattern is decided in CONTEXT.md. Layered transport pattern proven in both reference implementations.
- Pitfalls: HIGH -- All pitfalls documented from real experience in the reference implementations and firmware disassembly.

**Research date:** 2026-03-19
**Valid until:** 2026-04-19 (stable domain -- USB HID specs don't change, rusb 0.9.x is mature)
