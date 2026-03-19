# Phase 2: FEA Protocol & HID Transport - Context

**Gathered:** 2026-03-19
**Status:** Ready for planning

<domain>
## Phase Boundary

Reliable, safe HID communication with yc3121 keyboards that handles all known hardware quirks. Deliverables: device enumeration, HID I/O with safety guards (100ms throttling, stale-read retry, key matrix bounds validation), udev rules for non-root access, and hot-plug detection. This phase fills the `monsgeek-transport` crate that Phase 1 left as an empty shell.

</domain>

<decisions>
## Implementation Decisions

### HID backend
- Use `rusb` (libusb Rust bindings) for USB access — raw control transfers for HID SET_REPORT/GET_REPORT on IF2
- hidapi is NOT viable for MonsGeek keyboards: the M5W firmware STALLs on HID report descriptor reads for IF1/IF2 during kernel probe, which prevents hidapi from accessing IF2 (hidraw backend fails, libusb backend returns "hid_error not implemented")
- hidapi works fine for Akko keyboards (VID 0x3151) because their firmware handles HID probe correctly — this is a MonsGeek firmware bug, not a hidapi limitation
- The akko reference project's hidapi-based approach cannot be reused for MonsGeek hardware

### Kernel driver conflict (IF2 access)
- Dual-path approach: try udev unbind first, fall back to HID_QUIRK_IGNORE
- **Primary path (udev unbind):** udev rule unbinds usbhid from IF2 at USB level after initial probe, while leaving IF0 (keyboard) bound to hid-generic. Format: `ACTION=="add", SUBSYSTEM=="usb", DRIVER=="usbhid", ATTRS{idVendor}=="3141", ATTRS{idProduct}=="4005", ATTR{bInterfaceNumber}=="02", RUN+="/bin/sh -c 'echo -n %k > /sys/bus/usb/drivers/usbhid/unbind'"`
- **Fallback path (HID_QUIRK_IGNORE):** If the IF1/IF2 STALL causes a full USB port reset (killing IF0), use `options usbhid quirks=0x3141:0x4005:0x0010` in modprobe.d to prevent usbhid from binding to ANY interface. This requires managing IF0 input ourselves (via uinput, as monsgeek-hid-driver does).
- HID_QUIRK_IGNORE is device-wide (VID:PID), not per-interface — no selective option exists in the Linux kernel
- Hardware testing during Phase 2 will determine which path works

### Async model
- Dedicated OS thread owns the rusb handle and processes commands from a channel
- All HID I/O is serialized through this single thread — natural fit for 100ms inter-command throttling (just sleep in the loop)
- Async consumers (Phase 3 gRPC bridge) send commands via the channel and await responses
- Sync consumers (Phase 7 CLI) can use the same channel with blocking receive
- rusb stays completely off the tokio runtime — no spawn_blocking needed

### Hot-plug detection
- Built in Phase 2 as part of the dedicated transport thread
- Transport thread manages full device lifecycle: detect connection, open device, process commands, detect disconnection, clean up
- rusb's `HotplugBuilder` with `device_arrived`/`device_left` callbacks for USB-level detection (VID 0x3141)
- Phase 3's `watchDevList` RPC exposes this existing infrastructure via gRPC channel — no retrofit needed

### Hardware validation
- Gated integration test suite: `cargo test --features hardware`
- Tests run against real M5W hardware: GET_USB_VERSION, SET/GET_DEBOUNCE round-trip, hot-plug detection, stale-read retry verification
- Feature gate keeps hardware tests out of normal `cargo test` runs (CI doesn't need a keyboard)
- First test validates the udev unbind approach — determines whether we need the HID_QUIRK_IGNORE fallback
- Provides regression protection for all subsequent phases

### Claude's Discretion
- Transport thread channel design (bounded vs unbounded, backpressure strategy)
- Error type design for transport errors
- Echo byte matching implementation details
- Stale-read retry timing and strategy
- Key matrix bounds validation approach (HID-05)
- Integration test structure and organization

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### MonsGeek IF2 access and kernel driver conflict
- `references/monsgeek-hid-driver/src/device.rs` — UsbSession: rusb device opening, IF0/IF1/IF2 claiming, USB reset, HID_SET_PROTOCOL, control transfer API for SET_REPORT/GET_REPORT on IF2
- `references/monsgeek-hid-driver/deploy/monsgeek-hid-usbhid.conf` — HID_QUIRK_IGNORE modprobe config, documents why kernel driver conflict exists
- `references/monsgeek-hid-driver/.planning/PITFALLS.md` — Stale GET_FEATURE responses, feature report size (65 vs 64 bytes), hid-generic conflicts, checksum off-by-one, partial hidraw reads
- `references/monsgeek-hid-driver/src/vendor.rs` — Vendor protocol: SET_DEBOUNCE via rusb control transfers, 100ms inter-command delay, firmware EP0 FIFO architecture documentation
- `references/monsgeek-hid-driver/src/protocol.rs` — Bit7 checksum, 64-byte frame building, response echo validation, corrected command constants (SET_DEBOUNCE=0x06, not 0x11)

### Transport patterns (akko reference — hidapi-based, for architectural patterns only)
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/hid_wired.rs` — HidWiredTransport: feature report send/read, Mutex-wrapped HidDevice, event subsystem
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/flow_control.rs` — FlowControlTransport: retry logic, echo matching, throttling, stale-read handling
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/discovery.rs` — Device discovery: interface matching by usage page (0xFFFF/0xFF00), VID/PID filtering, bus type detection
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/lib.rs` — Transport trait definition, method signatures, type aliases

### Protocol constants (already built in Phase 1)
- `crates/monsgeek-protocol/src/checksum.rs` — ChecksumType, calculate_checksum, apply_checksum, build_command, build_ble_command
- `crates/monsgeek-protocol/src/protocol.rs` — ProtocolFamily detection, CommandTable (RY5088/YiChip), family-specific command bytes
- `crates/monsgeek-protocol/src/timing.rs` — DEFAULT_DELAY_MS (100ms), QUERY_RETRIES (5), SEND_RETRIES (3), dongle timing
- `crates/monsgeek-protocol/src/hid.rs` — REPORT_SIZE (65), INPUT_REPORT_SIZE (64), usage pages, interface numbers, is_vendor_usage_page

### Project requirements
- `.planning/REQUIREMENTS.md` — HID-01 through HID-06 (device detection, FEA commands, 100ms delay, stale-read retry, bounds validation, udev rules)
- `.planning/ROADMAP.md` §Phase 2 — Success criteria, dependency on Phase 1

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `monsgeek-protocol::checksum` — build_command() constructs 65-byte HID feature reports with checksums. Phase 2 consumes this for USB control transfers.
- `monsgeek-protocol::protocol` — ProtocolFamily::detect() and CommandTable provide family-specific command bytes. Phase 2 uses these to select correct opcodes.
- `monsgeek-protocol::timing` — DEFAULT_DELAY_MS, QUERY_RETRIES, SEND_RETRIES constants for throttling and retry logic.
- `monsgeek-protocol::hid` — REPORT_SIZE (65), INTERFACE_FEATURE (2), usage page constants for device matching.
- `monsgeek-protocol::registry` — DeviceRegistry for loading device definitions by VID/PID and device ID.

### Established Patterns
- Phase 1 used pure-data, no-I/O design for protocol crate — Phase 2's transport crate is the first crate with OS dependencies (rusb, udev)
- Device definitions are JSON-driven — transport should use DeviceRegistry for VID/PID matching during enumeration
- Reference project separates raw transport (HidWiredTransport) from flow control (FlowControlTransport) — consider similar layering

### Integration Points
- `monsgeek-transport` depends on `monsgeek-protocol` for command building, checksum logic, device registry, timing constants
- Phase 3 will depend on `monsgeek-transport` for the transport thread's command channel
- Phase 3's `watchDevList` RPC will consume hot-plug events from the transport thread
- Phase 7 CLI will use the same command channel with blocking receive

</code_context>

<specifics>
## Specific Ideas

- The monsgeek-hid-driver reference found that SET_DEBOUNCE was mapped to 0x11 (actually SET_SLEEPTIME) in some sources, causing the keyboard to enter sleep mode and appear bricked. Our protocol crate has both mappings (RY5088 uses 0x06, YiChip uses 0x11) — the M5W is YiChip family (yc3121), so verify the correct command byte on real hardware during integration testing.
- The monsgeek-hid-driver documents the firmware's EP0 FIFO architecture from disassembly: after SET_REPORT completes, the firmware's USB interrupt handler copies 64 bytes into a ring buffer, and the application-layer dispatcher drains it. A second SET_REPORT before the first is consumed causes the USB peripheral to NAK indefinitely (host sees timeout). This is why 100ms delay is mandatory.
- The 65-byte buffer convention: on Linux, Report ID 0 (no numbered reports) requires hidraw to prepend 0x00 as byte 0. rusb control transfers use the same convention — first byte is report ID, 64 bytes of payload follow.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 02-fea-protocol-hid-transport*
*Context gathered: 2026-03-19*
