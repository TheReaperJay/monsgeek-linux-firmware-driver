---
phase: 2
slug: fea-protocol-hid-transport
status: in_progress
nyquist_compliant: false
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

## Remaining Validation Before Phase 2 Closeout

| Behavior | Requirement / Goal | Status |
|----------|---------------------|--------|
| Long-lived control-mode session preserves normal typing | Phase 2 closeout quality gate | ⬜ pending |
| Hot-plug behavior remains stable through `udev` event path | HID-01 operational hardening | ⬜ pending |
| Debounce / SET-GET round-trip verification on live hardware | HID-04 / feature-roundtrip quality gate | ⬜ pending |
| Final Phase 2 summary written | planning closeout | ⬜ pending |

## Manual / Host-Only Verifications

| Behavior | Why Host-Only | Command / Method |
|----------|---------------|------------------|
| Real `GET_USB_VERSION` | needs real USB hardware | `cargo test -p monsgeek-transport --features hardware --test hardware test_get_usb_version -- --ignored --nocapture` |
| Dynamic enumeration | needs real USB hardware | `cargo test -p monsgeek-transport --features hardware --test hardware test_enumerate_m5w -- --ignored --nocapture` |
| Reset recovery | needs real USB hardware | `cargo test -p monsgeek-transport --features hardware --test stall_recovery -- --nocapture` |
| Interface ownership / typing preservation | needs live kernel / USB state | inspect with `lsusb -t` before and after host-side tests |

## Acceptance Standard For Phase 2

Phase 2 should only be treated as complete when:

- the transport layer is hardware-verified on the wired M5W
- the current planning documents reflect the corrected hardware facts
- long-lived control mode no longer steals keyboard input unintentionally
- the remaining operational behavior is documented clearly enough for Phase 3 to build on it safely

## Validation Sign-Off

- [x] Unit and compile-time validation exist
- [x] Real host-side transport validation exists
- [ ] Long-lived transport ownership model finalized
- [ ] Phase 2 summary completed
- [ ] `nyquist_compliant: true` ready to set on closeout

**Approval:** pending closeout of transport-mode ownership and summary
