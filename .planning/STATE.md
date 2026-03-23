---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: Ready to execute
stopped_at: Completed 03-01-PLAN.md
last_updated: "2026-03-23T17:02:27.532Z"
progress:
  total_phases: 8
  completed_phases: 2
  total_plans: 8
  completed_plans: 6
---

# Project State

## Project Reference

See: [.planning/PROJECT.md](./PROJECT.md), [.planning/ROADMAP.md](./ROADMAP.md), and [.planning/REQUIREMENTS.md](./REQUIREMENTS.md)

**Core value:** The MonsGeek configurator must work on Linux without requiring a Windows machine.
**Current focus:** Phase 03 — grpc-web-bridge

## Current Position

Phase: 03 (grpc-web-bridge) — EXECUTING
Plan: 2 of 3

## What Is Verified

- Wired M5W USB identity is `0x3151:0x4015`
- M5W dongle PID is `0x4011`
- `GET_USB_VERSION` works on real hardware after reset-then-reopen
- `GET_USB_VERSION` device ID is a 32-bit little-endian field and returns `1308`
- `connect()` defaults to control-only mode and no longer claims `IF0`
- userspace-input mode is explicit and emits translated input actions through the transport layer
- The transport thread’s 100ms throttling model is correct for this firmware
- `udev` is the reliable hot-plug mechanism in this environment
- Transport cleanup must return `IF0` to the kernel unless full userspace-input mode is intentional
- Native recovery (`recover()`) restores the wired M5W with reset-then-reopen plus `GET_USB_VERSION` verification

## Current Engineering Reality

Implemented:

- Protocol crate foundation and JSON-driven registry
- `rusb`-based USB session with control transfers on `IF2`
- Echo-matched query/send flow control
- Device discovery and firmware-ID-aware probing
- Transport thread and `udev` hot-plug monitoring
- Hardware tests for live `GET_USB_VERSION` and enumeration
- Control-only default transport ownership plus explicit userspace-input mode
- IF0 handoff fix so sessions no longer leave the keyboard dead when they are done
- Native recovery entry point for reset/reopen verification without relying on the test harness
- Routine hardware validation defaults to read-only checks; dangerous live writes require explicit opt-in

Residual follow-up after Phase 2:

- Keep discovery/transport modeling general enough for follow-on MonsGeek/Akko profiles and future dongle work
- Add first-class dongle transport implementation and live validation
- Keep dangerous feature-write tests narrowly gated until each write path has a proven-safe restore story

## Recent Decisions

- Roadmap remains eight phases, with the bridge phase as the MVP because configurator compatibility is the first user-visible success
- Firmware device ID, not USB PID, is the canonical model identity
- USB bus/address is runtime transport metadata only
- `rusb` is the correct backend for MonsGeek M5W transport behavior on Linux
- `udev`, not `libusb` arrival callbacks, is the hot-plug source in this environment
- `HID_QUIRK_IGNORE` is the chosen workaround for the broken kernel-probe path on this hardware setup
- The transport must not steal typing accidentally; `IF0` ownership must be explicit
- Live feature writes are not part of default transport validation; they require an explicit dangerous gate and a native recovery path
- Phase 3 should build on control-only transport by default and treat userspace-input as a separate runtime mode, not a baseline assumption

## Pending Todos

- Begin Phase 3 planning for the gRPC-Web bridge on `localhost:3814`
- Carry firmware-ID-first identity into the bridge's device enumeration APIs
- Keep dongle support explicitly out of the MVP until the transport path is implemented and validated

## Blockers / Concerns

- No Phase 2 blockers remain for the wired M5W control path
- Device-specific advanced features must be treated as per-profile capabilities, not assumed globally across the FEA family
- Dongle support is not yet implemented and must not be implied as already working
- Dangerous live writes can still wedge hardware if used carelessly; they remain opt-in and should stay out of routine development flows

## Session Continuity

Last major checkpoint: 2026-03-23  
Stopped at: Completed 03-01-PLAN.md
Next recommended action: start Phase 3 planning and execution for the gRPC-Web bridge
