---
phase: 4
slug: bridge-integration-key-remapping
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-25
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) + feature-gated hardware tests |
| **Config file** | Cargo.toml `[features]` section |
| **Quick run command** | `cargo test --workspace` |
| **Full suite command** | `cargo test --workspace && cargo test -p monsgeek-transport --features hardware -- --ignored --nocapture` |
| **Estimated runtime** | ~15 seconds (unit), ~45 seconds (full with hardware) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green + manual browser checkpoint
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 01 | 1 | KEYS-02 | unit | `cargo test -p monsgeek-driver -- test_set_keymatrix_bounds` | ❌ W0 | ⬜ pending |
| 04-01-02 | 01 | 1 | KEYS-02 | unit | `cargo test -p monsgeek-driver -- test_bounds_violation` | ❌ W0 | ⬜ pending |
| 04-02-01 | 02 | 2 | KEYS-01 | hardware | `cargo test -p monsgeek-transport --features hardware -- --ignored test_get_keymatrix --nocapture` | ❌ W0 | ⬜ pending |
| 04-02-02 | 02 | 2 | KEYS-03 | hardware | `cargo test -p monsgeek-transport --features hardware -- --ignored test_get_set_profile --nocapture` | ❌ W0 | ⬜ pending |
| 04-02-03 | 02 | 2 | KEYS-02 | hardware | `cargo test -p monsgeek-transport --features hardware -- --ignored test_set_keymatrix_roundtrip --nocapture` | ❌ W0 | ⬜ pending |
| 04-03-01 | 03 | 2 | KEYS-02 | manual | Open app.monsgeek.com, remap key, verify | N/A | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/monsgeek-driver/tests/bounds_validation.rs` — unit tests for SET_KEYMATRIX bounds validation at service layer
- [ ] `crates/monsgeek-transport/tests/hardware.rs` — hardware integration tests for GET_KEYMATRIX, SET_KEYMATRIX roundtrip, GET/SET_PROFILE

*Existing test infrastructure (cargo test) covers framework needs. No new framework installation required.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Browser key remapping | KEYS-02 | Requires real browser + real hardware + web configurator | 1. Open app.monsgeek.com 2. Select M5W 3. Select a key 4. Remap to different keycode 5. Verify keypress produces new mapping |
| Profile switching in browser | KEYS-03 | Requires browser UI + firmware state persistence | 1. Remap key on profile 0 2. Switch to profile 1 3. Verify key retains original mapping 4. Switch back to profile 0 5. Verify remap persists |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
