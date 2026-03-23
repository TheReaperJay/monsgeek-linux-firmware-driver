# Phase 2: FEA Protocol & HID Transport - Context

**Gathered:** 2026-03-19  
**Corrected:** 2026-03-23  
**Status:** In execution, with hardware validation partially complete

<domain>
## Phase Boundary

Reliable, safe HID communication with FEA keyboards on Linux, first verified against the wired MonsGeek M5W. Deliverables include device discovery, raw HID I/O with safety guards, non-root USB access, hot-plug detection, and the transport abstractions that Phase 3 will consume.

This phase is no longer speculative. Hardware validation established the real constraints and corrected several earlier assumptions.
</domain>

<decisions>
## Implementation Decisions

### HID backend

- Use `rusb` for MonsGeek wired transport
- `hidapi`/hidraw assumptions are not the source of truth for this hardware path
- The primary low-level reference is `references/monsgeek-hid-driver/`
- The primary architectural reference is `references/monsgeek-akko-linux/`

### Identity model

- USB bus/address is runtime transport metadata only
- USB PID is transport identity, not canonical model identity
- The canonical model identifier is the firmware-reported device ID from `GET_USB_VERSION`
- `GET_USB_VERSION` carries a 32-bit little-endian device ID field

### Kernel driver conflict

- On this hardware setup, `HID_QUIRK_IGNORE` is the chosen workaround for the broken IF1/IF2 probe path
- That removes automatic kernel handling for the device and means userspace code must be deliberate about `IF0` ownership
- Short-lived sessions must hand `IF0` back to the kernel on cleanup
- Long-lived sessions need an explicit mode decision:
  - control mode: claim `IF2` only, preserve normal typing through the kernel
  - userspace-input mode: intentionally own `IF0` and translate boot reports ourselves

### Async / ownership model

- A dedicated OS thread owns the active transport session and serializes commands
- This remains the right place to enforce the 100ms firmware safety delay
- `connect()` operates on `&DeviceDefinition`, not raw `(vid, pid)` pairs

### Hot-plug detection

- `udev` monitoring is the practical hot-plug mechanism in this Linux environment
- `libusb` hotplug arrival events were not reliable enough to trust as the primary source

### Hardware validation

- Hardware tests are feature-gated and ignored by default
- Real host-side validation is required for:
  - `GET_USB_VERSION`
  - firmware-ID-aware enumeration
  - transport cleanup / IF0 handoff behavior
  - hot-plug behavior
</decisions>

<canonical_refs>
## Canonical References

### Exact hardware target

- `references/monsgeek-hid-driver/src/device.rs`
- `references/monsgeek-hid-driver/src/vendor.rs`
- `references/monsgeek-hid-driver/deploy/monsgeek-hid-usbhid.conf`

### Framework / architecture reference

- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/discovery.rs`
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/flow_control.rs`
- `references/monsgeek-akko-linux/iot_driver_linux/src/profile/registry.rs`
- `references/monsgeek-akko-linux/iot_driver_linux/src/grpc.rs`

### Project truth sources

- `.planning/PROJECT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/ROADMAP.md`
- `crates/monsgeek-protocol/devices/m5w.json`
- `crates/monsgeek-transport/src/lib.rs`
- `crates/monsgeek-transport/src/usb.rs`
- `crates/monsgeek-transport/src/discovery.rs`
- `crates/monsgeek-transport/src/thread.rs`
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `monsgeek-protocol::checksum` builds report frames
- `monsgeek-protocol::protocol` provides protocol-family command tables
- `monsgeek-protocol::timing` defines transport timing constants
- `monsgeek-protocol::registry` and device JSON files provide the registry/profile layer
- `monsgeek-transport::usb::UsbVersionInfo` parses `GET_USB_VERSION`
- `monsgeek-transport::discovery::probe_devices` and `probe_device` already use firmware ID probing

### Established Patterns

- Channel-based transport thread with 100ms throttling
- Echo-matched query path for response validation
- Reset-then-reopen to recover from `PIPE` / stale device state
- `udev` event monitoring for add/remove

### Integration Points

- Phase 3 bridge will consume `TransportHandle` and `TransportEvent`
- Device picker / watch RPCs depend on discovery + hot-plug
- Later feature phases depend on this phase being stable enough not to interfere with normal typing
</code_context>

<specifics>
## Specific Real-Hardware Findings

- Wired M5W identity is VID `0x3151`, PID `0x4015`
- M5W 2.4GHz dongle identity is PID `0x4011`
- `GET_USB_VERSION` returns device ID `1308` as `0x0000051C`
- The IF0 handoff bug was real: a session that detached `usbhid` and failed to reattach it left the keyboard unable to type
- Raw USB control transfers still work when IF0/IF1/IF2 are claimed, but long-lived ownership of IF0 should be a deliberate mode, not the default
</specifics>

<deferred>
## Deferred Ideas

- Dongle transport implementation
- Bluetooth transport
- Firmware update engine
- Device-specific advanced features beyond the M5W
</deferred>

---

*Phase: 02-fea-protocol-hid-transport*  
*Context corrected after real hardware validation on 2026-03-23*
