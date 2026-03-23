---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: executing
stopped_at: "Planning stack corrected after real hardware validation; Phase 2 remains open for transport-mode cleanup"
last_updated: "2026-03-23T00:00:00Z"
progress:
  total_phases: 8
  completed_phases: 1
  total_plans: 5
  completed_plans: 4
---

# Project State

## Project Reference

See: [.planning/PROJECT.md](./PROJECT.md), [.planning/ROADMAP.md](./ROADMAP.md), and [.planning/REQUIREMENTS.md](./REQUIREMENTS.md)

**Core value:** The MonsGeek configurator must work on Linux without requiring a Windows machine.
**Current focus:** Phase 02 — finish the HID transport correctly after real hardware validation.

## Current Position

Phase: `02` (`fea-protocol-hid-transport`) — EXECUTING  
Plan: `03` of `03`

Phase 2 is no longer blocked on “can we talk to the keyboard at all?” That part is proven on the wired M5W. The remaining work is to finish the long-lived transport ownership model cleanly before moving to the bridge phase.

## What Is Verified

- Wired M5W USB identity is `0x3151:0x4015`
- M5W dongle PID is `0x4011`
- `GET_USB_VERSION` works on real hardware after reset-then-reopen
- `GET_USB_VERSION` device ID is a 32-bit little-endian field and returns `1308`
- The transport thread’s 100ms throttling model is correct for this firmware
- `udev` is the reliable hot-plug mechanism in this environment
- Transport cleanup must return `IF0` to the kernel unless full userspace-input mode is intentional

## Current Engineering Reality

Implemented:

- Protocol crate foundation and JSON-driven registry
- `rusb`-based USB session with control transfers on `IF2`
- Echo-matched query/send flow control
- Device discovery and firmware-ID-aware probing
- Transport thread and `udev` hot-plug monitoring
- Hardware tests for live `GET_USB_VERSION` and enumeration
- IF0 handoff fix so short-lived sessions no longer leave the keyboard dead

Still open before the bridge phase:

- Split transport into clear ownership modes:
  - control mode: `IF2` only, preserve kernel typing
  - userspace-input mode: intentionally own `IF0`
- Close out Phase 2 hardware-validation plan and write its summary
- Keep discovery/transport modeling general enough for follow-on MonsGeek/Akko profiles and future dongle work

## Recent Decisions

- Roadmap remains eight phases, with the bridge phase as the MVP because configurator compatibility is the first user-visible success
- Firmware device ID, not USB PID, is the canonical model identity
- USB bus/address is runtime transport metadata only
- `rusb` is the correct backend for MonsGeek M5W transport behavior on Linux
- `udev`, not `libusb` arrival callbacks, is the hot-plug source in this environment
- `HID_QUIRK_IGNORE` is the chosen workaround for the broken kernel-probe path on this hardware setup
- The transport must not steal typing accidentally; `IF0` ownership must be explicit

## Pending Todos

- Synchronize the remaining historical planning docs with errata notes
- Finish the Phase 2 transport-mode split
- Then begin bridge planning and implementation for the gRPC-Web layer

## Blockers / Concerns

- Phase 2 should not be declared complete until long-lived control sessions stop interfering with normal keyboard input
- Device-specific advanced features must be treated as per-profile capabilities, not assumed globally across the FEA family
- Dongle support is not yet implemented and must not be implied as already working

## Session Continuity

Last major checkpoint: 2026-03-23  
Stopped at: planning stack correction after hardware verification  
Next recommended action: finish Phase 2 transport-mode cleanup, then start the bridge workstream
