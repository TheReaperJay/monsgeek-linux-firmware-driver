---
phase: 6
slug: macros-device-specific-advanced-features
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-27
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml workspace + per-crate feature flags |
| **Quick run command** | `cargo test -p monsgeek-protocol -p monsgeek-driver` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~15 seconds (unit), ~30 seconds (with hardware) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p monsgeek-protocol -p monsgeek-driver`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green + browser macro checkpoint on real M5W
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 06-01-01 | 01 | 1 | MACR-02 | unit | `cargo test -p monsgeek-driver validate_dangerous_write_set_macro` | ❌ W0 | ⬜ pending |
| 06-01-02 | 01 | 1 | N/A | unit | `cargo test -p monsgeek-driver validate_dangerous_write_set_fn` | ❌ W0 | ⬜ pending |
| 06-01-03 | 01 | 1 | N/A | unit | `cargo test -p monsgeek-driver validate_dangerous_write_magnetic` | ❌ W0 | ⬜ pending |
| 06-02-01 | 02 | 1 | MACR-01 | hardware | `cargo test -p monsgeek-transport --features hardware -- --ignored test_get_macro --nocapture` | ❌ W0 | ⬜ pending |
| 06-02-02 | 02 | 1 | MACR-02 | hardware (dangerous) | `MONSGEEK_ENABLE_DANGEROUS_WRITES=1 cargo test -p monsgeek-transport --features "hardware dangerous-hardware-writes" -- --ignored test_set_get_macro --nocapture` | ❌ W0 | ⬜ pending |
| 06-02-03 | 02 | 1 | MAG-01 | unit | `cargo test -p monsgeek-transport test_magnetism_calibration_parse` | ❌ W0 | ⬜ pending |
| 06-02-04 | 02 | 1 | MAG-02 | unit | `cargo test -p monsgeek-transport test_magnetism_calibration_commands` | ❌ W0 | ⬜ pending |
| 06-02-05 | 02 | 1 | MAG-03 | unit | `cargo test -p monsgeek-transport test_magnetism_rt_read` | ❌ W0 | ⬜ pending |
| 06-02-06 | 02 | 1 | MAG-04 | unit | `cargo test -p monsgeek-transport test_magnetism_set_actuation` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/monsgeek-driver/src/service/mod.rs` — SET_MACRO, SET_FN, magnetic command bounds validation unit tests in test module
- [ ] `crates/monsgeek-transport/tests/hardware.rs` — macro GET/SET round-trip hardware test stubs (MACR-01, MACR-02)
- [ ] Magnetic wire format unit tests — location TBD (transport or protocol crate)

*Existing test infrastructure (cargo test, feature flags) covers framework needs.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Browser macro checkpoint | MACR-02 | Requires web configurator UI + real keyboard interaction | 1. Open web configurator 2. Navigate to macro editor 3. Program new macro in slot 0 4. Trigger macro key 5. Verify correct key sequence playback |
| Magnetic switch hardware validation | MAG-01..04 | M5W has noMagneticSwitch: true; need magnetic-capable device | Deferred until magnetic-capable device available. Unit tests verify wire format correctness. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
