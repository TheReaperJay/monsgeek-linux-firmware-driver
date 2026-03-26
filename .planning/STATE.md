---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: unknown
stopped_at: Completed 05.1-01-PLAN.md
last_updated: "2026-03-26T15:36:36.853Z"
progress:
  total_phases: 9
  completed_phases: 5
  total_plans: 15
  completed_plans: 13
---

# Project State

## Project Reference

See: [.planning/PROJECT.md](./PROJECT.md), [.planning/ROADMAP.md](./ROADMAP.md), and [.planning/REQUIREMENTS.md](./REQUIREMENTS.md)

**Core value:** The MonsGeek configurator must work on Linux without requiring a Windows machine.
**Current focus:** Phase 05.1 — userspace-input-daemon

## Current Position

Phase: 05.1 (userspace-input-daemon) — EXECUTING
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
- gRPC bridge split send/read semantics are covered by deterministic integration tests via mock transport
- gRPC bridge contract/runtime suite now includes:
  - `grpc_full_service_contract_present`
  - `grpc_server_starts_http1`
  - `grpc_cors_headers_present`
  - `grpc_send_raw_feature_forwards`
  - `grpc_read_raw_feature_returns_data`
  - `grpc_watch_dev_list_init_add_remove`
  - `grpc_get_version_shape`
  - `grpc_db_insert_get_roundtrip`

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
- Phase 3 closeout requires explicit human browser verification before marking Nyquist compliant
- validate_dangerous_write is a pub(crate) free function for direct testability from unit tests
- Extracted find_connected_device to eliminate device lookup duplication between get_handle_for_path and get_device_for_path
- MAX_PROFILE=3 as a module constant matching firmware hard limit of 4 profiles (0-3)
- SET_REPORT and GET_REPORT registered as shared commands for YiChip backfill coverage
- Checksum types documented as comments only; schema map does not enforce checksum type
- GET_LEDPARAM uses Bit7 checksum (not Bit8 as planned); only SET_LEDPARAM uses Bit8
- M5W firmware supports GET_REPORT (rate_code=0 / 8kHz) despite device definition having get_report: None
- SessionMode::InputOnly claims IF0/IF1 only, leaving IF2 free for gRPC bridge coexistence
- Match-on-mode dispatch replaces boolean flags for all interface selection in usb.rs
- evdev 0.13 uses KeyCode (not Key) and InputEvent::new takes raw u16 for event type

## Pending Todos

- Execute Phase 04 Plan 02 (key remapping gRPC endpoints)

## Blockers / Concerns

- No automated blockers remain for Phase 03
- Manual browser checkpoint is still required before Phase 03 can be declared closed
- Device-specific advanced features must be treated as per-profile capabilities, not assumed globally across the FEA family
- Dongle support is not yet implemented and must not be implied as already working
- Dangerous live writes can still wedge hardware if used carelessly; they remain opt-in and should stay out of routine development flows

## Accumulated Context

### Roadmap Evolution

- Phase 5.1 inserted after Phase 5: Userspace Input Daemon (URGENT) — Latency tracing proved kernel HID processing adds only 17us and kernel→userspace delivery is 88us, but Mutter compositor adds 342us p50 with 2.5-18ms jitter at p95. Combined with 6-12ms switch bounce passing through 1ms firmware debounce, the fix requires a userspace daemon that claims IF0, applies software debounce, and injects clean events via uinput. Transport infrastructure (InputProcessor, IF0 claiming, keymap, pump_input) already exists and is tested. Missing piece: uinput virtual device creation and a separate daemon binary.

## Session Continuity

Last major checkpoint: 2026-03-26
Stopped at: Completed 05.1-01-PLAN.md
Next recommended action: Execute 05.1-02-PLAN.md (daemon event loop)
