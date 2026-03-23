---
phase: 02-fea-protocol-hid-transport
plan: 03
subsystem: transport
tags: [rust, rusb, hid, hardware-validation, recovery, transport-modes, safety-gates]

requires:
  - phase: 02-fea-protocol-hid-transport
    plan: 01
    provides: UsbSession foundation, transport errors, bounds validation
  - phase: 02-fea-protocol-hid-transport
    plan: 02
    provides: flow control, discovery, transport thread, hot-plug thread

provides:
  - hardware-validated `GET_USB_VERSION` parsing with 32-bit firmware device ID handling
  - firmware-ID-aware session matching instead of PID-only identity assumptions
  - `TransportOptions` / `connect_with_options()` with control-only default and explicit userspace-input mode
  - translated `TransportEvent::InputActions` when userspace-input mode is selected
  - native `recover()` API and `recover` example for reset-then-reopen recovery
  - dangerous live-write tests isolated behind explicit compile-time and runtime opt-in
  - corrected Phase 2 planning and validation artifacts based on real M5W behavior

affects: [phase-3, phase-5, phase-7]

tech-stack:
  added: []
  patterns:
    - firmware-id-first-device-identity
    - mode-selected-interface-claiming
    - read-only-default-hardware-validation
    - native-reset-reopen-recovery

key-files:
  created:
    - .planning/phases/02-fea-protocol-hid-transport/02-03-SUMMARY.md
    - crates/monsgeek-transport/examples/recover.rs
  modified:
    - .planning/ROADMAP.md
    - .planning/STATE.md
    - .planning/phases/02-fea-protocol-hid-transport/02-VALIDATION.md
    - crates/monsgeek-protocol/devices/m5w.json
    - crates/monsgeek-protocol/src/device.rs
    - crates/monsgeek-protocol/src/protocol.rs
    - crates/monsgeek-transport/Cargo.toml
    - crates/monsgeek-transport/src/lib.rs
    - crates/monsgeek-transport/src/thread.rs
    - crates/monsgeek-transport/src/usb.rs
    - crates/monsgeek-transport/tests/hardware.rs

key-decisions:
  - "Default transport ownership is control-only: claim IF2 and leave kernel typing on IF0 untouched"
  - "Userspace input is explicit opt-in via TransportOptions rather than an accidental side effect of connect()"
  - "Firmware device ID from GET_USB_VERSION is the canonical device identity; USB PID is transport metadata only"
  - "Live feature writes are not routine transport validation because the M5W can be left in a bad state after failed restore"
  - "Native recovery is part of the transport surface and uses reset-then-reopen plus GET_USB_VERSION verification"

patterns-established:
  - "Control-only by default, userspace-input only when explicitly requested"
  - "Read-only host validation for transport bring-up; mutating hardware tests require explicit dangerous gating"
  - "Recovery must use the repo's native reset/reopen flow rather than ad hoc USB poking"

requirements-completed: [HID-01, HID-02, HID-03, HID-04, HID-05, HID-06]

duration: multi-session
completed: 2026-03-23
---

# Phase 02 Plan 03: Hardware Validation and Safe Transport Closeout Summary

**Phase 2 is closed with real wired-M5W validation, control-only default ownership, explicit userspace-input mode, and a safer validation model that no longer treats live writes as routine transport proof.**

## Performance

- **Duration:** multi-session over 2026-03-19 to 2026-03-23
- **Tasks:** 4 major closeout tasks
- **Files modified:** 16 in the final closeout set

## Accomplishments

- Verified on real hardware that `GET_USB_VERSION` works after reset-then-reopen and that its device ID field is 32-bit little-endian, returning firmware device ID `1308`
- Corrected discovery and connect logic so runtime matching is driven by firmware device ID instead of assuming a single stable PID or any stable bus/address
- Implemented the missing ownership split: `connect()` now defaults to control-only transport, while `connect_with_options()` exposes explicit userspace-input mode with translated `InputActions`
- Added a native recovery path via `monsgeek_transport::recover()` and the `recover` example so transient `PIPE` / stale-session states can be cleared without relying on a reference driver
- Hardened validation so dangerous live writes are no longer part of the default hardware suite; mutating tests now require both the `dangerous-hardware-writes` feature and `MONSGEEK_ENABLE_DANGEROUS_WRITES=1`
- Reconciled the planning stack with the real transport model, corrected USB identity facts, and removed the remaining “Phase 2 is still waiting on ownership split” drift

## Host-Side Verification

The following checks were run against the wired M5W on the host:

- `cargo test -p monsgeek-transport --lib`
- `cargo test -p monsgeek-transport --features hardware --tests --no-run`
- `cargo run -p monsgeek-transport --example recover -- 1308`
- `cargo test -p monsgeek-transport --features hardware --test hardware test_get_usb_version -- --ignored --nocapture`
- `cargo test -p monsgeek-transport --features hardware --test hardware test_enumerate_m5w -- --ignored --nocapture`
- `cargo test -p monsgeek-transport --features hardware --test stall_recovery -- --nocapture`
- `lsusb -t` after recovery and safe tests

Observed results:

- `recover` returned device ID `1308` / `0x0000051C` with firmware version `0x0070`
- `test_get_usb_version`, `test_enumerate_m5w`, and `stall_recovery` all passed on the host
- final USB state showed the wired keyboard with `IF0 -> usbhid` and `IF1/IF2 -> [none]`, which matches the intended control-only model

## Task Commits

1. **Hardware-gated transport validation** - `0b0214b`
2. **Real M5W transport validation and planning correction** - `011452a`
3. **Phase 2 closeout scope correction** - `ad7ea40`

## Files Created/Modified

- `.planning/phases/02-fea-protocol-hid-transport/02-03-SUMMARY.md` - records the actual Phase 2 closeout and hardware evidence
- `.planning/ROADMAP.md` - marks Phase 2 complete and aligns progress with the verified transport model
- `.planning/STATE.md` - moves the project to Phase 3 readiness instead of leaving Phase 2 artificially open
- `.planning/phases/02-fea-protocol-hid-transport/02-VALIDATION.md` - closes validation sign-off and documents the read-only default validation model
- `crates/monsgeek-protocol/devices/m5w.json` - adds M5W-specific command overrides required for correct YiChip-family command bytes
- `crates/monsgeek-protocol/src/device.rs` - exposes device-level command overrides and protocol-family-aware command resolution
- `crates/monsgeek-protocol/src/protocol.rs` - supports protocol-family command mapping needed by M5W overrides
- `crates/monsgeek-transport/Cargo.toml` - adds the `dangerous-hardware-writes` feature gate
- `crates/monsgeek-transport/src/lib.rs` - adds `TransportOptions`, explicit userspace-input selection, and native `recover()`
- `crates/monsgeek-transport/src/thread.rs` - emits `TransportEvent::InputActions` when userspace-input mode is active
- `crates/monsgeek-transport/src/usb.rs` - implements mode-selected interface claiming, 32-bit `UsbVersionInfo`, and cleanup that returns IF0 to the kernel when appropriate
- `crates/monsgeek-transport/tests/hardware.rs` - keeps routine hardware validation read-only and isolates dangerous live writes behind explicit opt-in
- `crates/monsgeek-transport/examples/recover.rs` - native host recovery entry point

## Decisions and Deviations

### Important deviation from the original validation model

The original live write-round-trip validation turned out to be unsafe on the real M5W. A failed restore after `SET_DEBOUNCE` left the keyboard in a bad device state. That changed the phase closeout bar:

- safe transport validation is now read-only by default
- live writes are treated as dangerous feature validation, not basic transport proof
- native recovery is required whenever hardware validation leaves the device in a stale state

This is a substantive correction, not cosmetic cleanup. It removes a class of avoidable hardware regressions from routine development work.

## Next Phase Readiness

Phase 3 can now assume:

- a real wired M5W transport exists on Linux
- transport identity is firmware-ID-first
- control-mode sessions no longer steal typing by default
- userspace-input mode exists when the framework intentionally wants to own IF0
- there is a native recovery path for transient USB firmware stalls

Non-blocking follow-up remains for later work:

- dongle transport still needs first-class implementation and live validation
- dangerous feature-write tests should only expand when each feature has a proven-safe restore path

## Self-Check: PASSED

- Summary matches the actual landed transport behavior
- Host-side verification commands and observed results are recorded
- Phase 2 closeout no longer depends on the unsafe live-write assumption
- Planning artifacts can hand off directly into Phase 3 planning

---
*Phase: 02-fea-protocol-hid-transport*
*Completed: 2026-03-23*
