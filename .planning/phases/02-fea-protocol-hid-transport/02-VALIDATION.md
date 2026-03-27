---
phase: 2
slug: fea-protocol-hid-transport
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-19
corrected: 2026-03-23
---

# Phase 2 — Validation Strategy

Validation for Phase 2 is now split between normal Rust test coverage and explicit host-side hardware verification on the wired M5W.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` |
| **Hardware feature** | `hardware` |
| **Quick run command** | `cargo test -p monsgeek-transport --lib` |
| **Compile hardware tests** | `cargo test -p monsgeek-transport --features hardware --tests --no-run` |
| **Hardware test command** | `cargo test -p monsgeek-transport --features hardware -- --ignored --nocapture` |
| **Dangerous write tests** | `MONSGEEK_ENABLE_DANGEROUS_WRITES=1 cargo test -p monsgeek-transport --features "hardware dangerous-hardware-writes" --test hardware test_set_get_debounce_round_trip_dangerous -- --ignored --nocapture` |
| **Primary host checks** | `test_get_usb_version`, `test_enumerate_m5w`, `stall_recovery` |

## What Has Been Verified

| Behavior | Status | Evidence |
|----------|--------|----------|
| Transport crate compiles and unit tests pass | ✅ | `cargo test -p monsgeek-transport --lib` |
| Hardware tests compile | ✅ | `cargo test -p monsgeek-transport --features hardware --tests --no-run` |
| `GET_USB_VERSION` works on real M5W | ✅ | host-side hardware test passed |
| `GET_USB_VERSION` device ID is 32-bit and equals `1308` | ✅ | parsed from live response |
| Firmware-ID-aware enumeration finds M5W dynamically | ✅ | `test_enumerate_m5w` passed on host |
| Reset-then-reopen clears transient `PIPE` state | ✅ | `stall_recovery` passed on host |
| Short-lived session cleanup can restore typing | ✅ | IF0 reattach fix validated on host |
| Native recovery entry point exists outside the test harness | ✅ | `monsgeek_transport::recover()` plus `cargo run -p monsgeek-transport --example recover -- 1308` |
| Dangerous live-write validation is isolated from routine transport checks | ✅ | compile-time `dangerous-hardware-writes` gate plus `MONSGEEK_ENABLE_DANGEROUS_WRITES=1` runtime opt-in |

## Remaining Validation Before Phase 2 Closeout

| Behavior | Requirement / Goal | Status |
|----------|---------------------|--------|
| Long-lived control-mode session preserves normal typing | Phase 2 closeout quality gate | ✅ complete |
| Final Phase 2 summary written | planning closeout | ✅ complete |

## Residual Follow-Up After Phase 2

| Behavior | Why It Remains Follow-Up |
|----------|---------------------------|
| Hot-plug arrival/leave behavior through the `udev` path | implemented and non-blocking for the wired-M5W bridge MVP, but should continue to be observed during Phase 3 integration |
| Dangerous feature-write validation | intentionally excluded from routine transport validation until each write path has a proven-safe restore strategy |

## Manual / Host-Only Verifications

| Behavior | Why Host-Only | Command / Method |
|----------|---------------|------------------|
| Real `GET_USB_VERSION` | needs real USB hardware | `cargo test -p monsgeek-transport --features hardware --test hardware test_get_usb_version -- --ignored --nocapture` |
| Dynamic enumeration | needs real USB hardware | `cargo test -p monsgeek-transport --features hardware --test hardware test_enumerate_m5w -- --ignored --nocapture` |
| Reset recovery | needs real USB hardware | `cargo test -p monsgeek-transport --features hardware --test stall_recovery -- --nocapture` |
| Native recovery command | needs real USB hardware | `cargo run -p monsgeek-transport --example recover -- 1308` |
| Interface ownership / typing preservation | needs live kernel / USB state | inspect with `lsusb -t` before and after host-side tests |
| Dangerous write validation | mutates live keyboard state | `MONSGEEK_ENABLE_DANGEROUS_WRITES=1 cargo test -p monsgeek-transport --features "hardware dangerous-hardware-writes" --test hardware test_set_get_debounce_round_trip_dangerous -- --ignored --nocapture` |

## Acceptance Standard For Phase 2

Phase 2 should only be treated as complete when:

- the transport layer is hardware-verified on the wired M5W
- the current planning documents reflect the corrected hardware facts
- long-lived control mode no longer steals keyboard input unintentionally
- routine transport validation is read-only by default, with live writes requiring explicit dangerous opt-in
- the remaining operational behavior is documented clearly enough for Phase 3 to build on it safely

## Validation Sign-Off

- [x] Unit and compile-time validation exist
- [x] Real host-side transport validation exists
- [x] Long-lived transport ownership model finalized
- [x] Phase 2 summary completed
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** complete for Phase 2 closeout
