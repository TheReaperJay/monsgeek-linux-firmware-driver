---
phase: 2
slug: fea-protocol-hid-transport
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-19
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) |
| **Config file** | Cargo.toml `[features] hardware = []` |
| **Quick run command** | `cargo test -p monsgeek-transport` |
| **Full suite command** | `cargo test --workspace` |
| **Hardware test command** | `cargo test -p monsgeek-transport --features hardware` |
| **Estimated runtime** | ~5 seconds (unit), ~30 seconds (hardware) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p monsgeek-transport`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green + hardware tests pass on real M5W
- **Max feedback latency:** 5 seconds (unit tests)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 02-01-01 | 01 | 1 | HID-01 | unit + hardware | `cargo test -p monsgeek-transport test_enumerate` | ❌ W0 | ⬜ pending |
| 02-01-02 | 01 | 1 | HID-02 | hardware | `cargo test -p monsgeek-transport --features hardware test_get_usb_version` | ❌ W0 | ⬜ pending |
| 02-02-01 | 02 | 1 | HID-03 | unit | `cargo test -p monsgeek-transport test_throttle` | ❌ W0 | ⬜ pending |
| 02-02-02 | 02 | 1 | HID-04 | unit + hardware | `cargo test -p monsgeek-transport test_echo_matching` | ❌ W0 | ⬜ pending |
| 02-03-01 | 03 | 1 | HID-05 | unit | `cargo test -p monsgeek-transport test_bounds_validation` | ❌ W0 | ⬜ pending |
| 02-03-02 | 03 | 1 | HID-06 | unit (file check) | `cargo test -p monsgeek-transport test_udev_rules` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/monsgeek-transport/tests/hardware.rs` — gated integration tests for HID-01, HID-02, HID-04
- [ ] Unit test stubs for throttle timing (HID-03), bounds validation (HID-05)
- [ ] `deploy/99-monsgeek.rules` — udev rules file (HID-06)
- [ ] Framework install: already available (cargo test built-in)

*Wave 0 test stubs should be created as part of first plan execution.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real M5W detection | HID-01 | Requires physical hardware connected | Plug in M5W, run `cargo test -p monsgeek-transport --features hardware test_enumerate` |
| Real HID command round-trip | HID-02 | Requires physical hardware connected | With M5W connected, run `cargo test -p monsgeek-transport --features hardware test_get_usb_version` |
| Udev rule installation | HID-06 | Requires sudo and system udev reload | Copy `deploy/99-monsgeek.rules` to `/etc/udev/rules.d/`, run `sudo udevadm control --reload-rules` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 5s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
