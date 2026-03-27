# Phase 2: FEA Protocol & HID Transport - Research

**Researched:** 2026-03-19  
**Corrected:** 2026-03-23  
**Domain:** Linux USB/HID transport for FEA keyboards, kernel-driver interaction, hot-plug, and hardware validation  
**Confidence:** HIGH after real host-side validation on the wired M5W

## Summary

Phase 2 is responsible for a safe, reusable Linux HID transport layer for FEA keyboards. The first fully verified target is the MonsGeek M5W in wired mode. The correct low-level backend for this hardware is `rusb` using HID class control transfers on `IF2`, not a hidraw-first design. The exact M5W hardware proved several critical facts:

- Wired M5W USB identity is `0x3151:0x4015`
- The 2.4GHz dongle uses PID `0x4011`
- `GET_USB_VERSION` returns the firmware device ID as a 32-bit little-endian field
- The firmware requires 100ms command spacing
- `reset -> reopen -> query` is the reliable recovery path from transient `PIPE` states
- `udev` monitoring is the practical hot-plug path on this Linux setup
- Userspace sessions must not leave `IF0` detached unless userspace input mode is intentional

The reference hierarchy is clear:

- `references/monsgeek-hid-driver/` is the primary source for exact M5W transport behavior
- `references/monsgeek-akko-linux/` is the stronger source for framework architecture, transport layering, profile modeling, and gRPC bridge design

## Locked Conclusions

### 1. Backend choice

- Use `rusb` for MonsGeek wired transport on Linux
- Raw HID feature reports are sent with USB HID class control transfers on `IF2`

### 2. Identity model

- Device identity is not USB PID
- Canonical model identity comes from firmware-reported device ID via `GET_USB_VERSION`
- Bus and address are runtime transport coordinates only

### 3. Recovery model

- Reset-then-reopen is not optional folklore; it was directly verified on the wired M5W
- Echo-matched query handling remains the correct response-validation pattern

### 4. Hot-plug model

- `udev` should be treated as the primary hot-plug event source
- `libusb` hotplug arrival callbacks are not reliable enough here to be the planning truth

### 5. Ownership model

- Claiming IF0/IF1/IF2 can work technically
- But the framework should separate:
  - control mode: preserve kernel typing, own `IF2`
  - userspace-input mode: own `IF0` intentionally

## Phase Requirements Support

| ID | Description | Current Support |
|----|-------------|-----------------|
| HID-01 | Enumerate supported keyboards dynamically | `rusb` enumeration plus registry/profile matching, with firmware-ID probing where needed |
| HID-02 | Send / receive FEA commands over vendor HID interface | `write_control` / `read_control` on `IF2`, 64-byte payloads |
| HID-03 | Enforce 100ms firmware safety delay | transport thread + query flow-control timing |
| HID-04 | Handle stale/mismatched/bad responses | echo matching plus reset/reopen recovery |
| HID-05 | Prevent invalid writes | bounds validation against `DeviceDefinition` |
| HID-06 | Non-root access on Linux | udev rules for USB access |

## Standard Stack

### Core

| Library | Purpose | Why |
|---------|---------|-----|
| `rusb` | USB control transfers, interface claim/release, reset | Correct backend for this hardware path |
| `crossbeam-channel` | transport-thread command queue | good fit for serialized HID I/O |
| `udev` | hot-plug monitoring | verified practical arrival/removal source |
| `log` | structured transport logging | lightweight and sufficient |

### Project Crates

| Crate | Purpose |
|-------|---------|
| `monsgeek-protocol` | protocol constants, checksums, timing, registry/profile data |
| `monsgeek-transport` | USB session, discovery, flow control, transport thread |

## Recommended Architecture Patterns

### Pattern 1: `UsbSession` for raw control transfers

`UsbSession` owns the device handle and exposes:

- `vendor_set_report`
- `vendor_get_report`
- `query_usb_version`
- optional input-path methods when userspace input mode is active

The session is also responsible for:

- reset/reopen recovery
- interface claim/release
- handing `IF0` back to the kernel when appropriate

### Pattern 2: flow control above raw I/O

Raw USB I/O should stay dumb. Retry, echo matching, and timing rules belong in the flow-control layer or transport thread, not buried in every caller.

### Pattern 3: firmware-ID-aware discovery

Use registry/profile data to find candidate transports, then probe with `GET_USB_VERSION` where necessary to resolve the real keyboard model.

### Pattern 4: `udev` hot-plug

The transport stack should treat `udev` as the source of truth for device arrival/removal events on Linux in this project.

### Pattern 5: explicit transport ownership modes

The long-lived transport API should make it explicit whether the session is:

- control-only, preserving kernel typing
- full userspace-input, taking ownership of IF0

## Anti-Patterns To Avoid

- Treating PID as canonical device identity
- Assuming bus/address is stable
- Defaulting to long-lived IF0 ownership when the caller only needs IF2 commands
- Rebinding IF1/IF2 to the kernel on cleanup
- Treating pre-hardware planning assumptions as stronger than the M5W reference or direct host validation
- Copying reference-project constants or field widths without checking the actual target behavior

## Verified M5W Findings To Preserve

- `GET_USB_VERSION` response bytes `[1..4]` are the device ID
- The M5W returned device ID `1308`
- The transport can recover from `PIPE` state via reset-then-reopen
- The keyboard must be returned to `IF0 -> usbhid` after short-lived sessions

## Remaining Unknowns

- Final shape of long-lived control-mode transport
- Exact first-class dongle transport implementation for PID `0x4011`
- Which advanced feature families are truly supported beyond the M5W's standard mechanical capabilities
- Firmware-update transport details and safety validation

## Primary Sources

- `references/monsgeek-hid-driver/src/device.rs`
- `references/monsgeek-hid-driver/src/vendor.rs`
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/discovery.rs`
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/flow_control.rs`
- `references/monsgeek-akko-linux/iot_driver_linux/src/profile/registry.rs`
- live host-side hardware validation performed during Phase 2 execution

---
*Research corrected after hardware validation on 2026-03-23*
