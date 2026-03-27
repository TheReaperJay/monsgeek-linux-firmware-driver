---
phase: 5
slug: led-control-tuning
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-26
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | `crates/monsgeek-transport/Cargo.toml` (features: `hardware`, `dangerous-hardware-writes`) |
| **Quick run command** | `cargo test -p monsgeek-protocol -- --nocapture` |
| **Full suite command** | `cargo test -p monsgeek-transport --features hardware -- --ignored --nocapture` |
| **Estimated runtime** | ~15 seconds (unit tests), ~60 seconds (hardware integration with device delays) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p monsgeek-protocol -- --nocapture`
- **After every plan wave:** Run `cargo test -p monsgeek-transport --features hardware -- --ignored --nocapture`
- **Before `/gsd:verify-work`:** Full suite including dangerous writes + browser checkpoint must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 05-01-01 | 01 | 1 | Schema | unit | `cargo test -p monsgeek-protocol -- --nocapture` | Partially ✅ | ⬜ pending |
| 05-01-02 | 01 | 1 | Schema | unit | `cargo test -p monsgeek-protocol -- schema --nocapture` | ❌ W0 | ⬜ pending |
| 05-02-01 | 02 | 2 | LED-01 | hardware integration | `cargo test -p monsgeek-transport --features hardware -- --ignored test_get_ledparam --nocapture` | ❌ W0 | ⬜ pending |
| 05-02-02 | 02 | 2 | LED-02 | hardware integration (dangerous) | `MONSGEEK_ENABLE_DANGEROUS_WRITES=1 cargo test -p monsgeek-transport --features "hardware dangerous-hardware-writes" -- --ignored test_set_get_ledparam_round_trip_dangerous --nocapture` | ❌ W0 | ⬜ pending |
| 05-02-03 | 02 | 2 | TUNE-01 | hardware integration (dangerous) | `MONSGEEK_ENABLE_DANGEROUS_WRITES=1 cargo test -p monsgeek-transport --features "hardware dangerous-hardware-writes" -- --ignored test_set_get_debounce_round_trip_dangerous --nocapture` | ✅ (Phase 2) | ⬜ pending |
| 05-02-04 | 02 | 2 | TUNE-02 | hardware integration | `cargo test -p monsgeek-transport --features hardware -- --ignored test_probe_polling_rate --nocapture` | ❌ W0 | ⬜ pending |
| 05-02-05 | 02 | 2 | All | browser checkpoint | Manual: open app.monsgeek.com, change LED/debounce, verify | N/A manual | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `test_get_ledparam` — hardware test for LED-01 (read-only, safe)
- [ ] `test_set_get_ledparam_round_trip_dangerous` — hardware test for LED-02 (dangerous writes)
- [ ] `test_probe_polling_rate` — hardware test for TUNE-02 (read-only probe, may timeout)
- [ ] Schema audit unit tests for newly added shared command entries (SET_REPORT, GET_REPORT)

*Existing `test_set_get_debounce_round_trip_dangerous` covers TUNE-01.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Browser LED control | LED-01, LED-02 | Requires real browser + keyboard visual confirmation | Open app.monsgeek.com, change LED effect mode, adjust brightness/speed/color, confirm keyboard responds |
| Browser debounce adjustment | TUNE-01 | Requires real browser + keyboard | Open app.monsgeek.com, change debounce value, confirm keyboard acknowledges |
| Polling rate probe documentation | TUNE-02 | Result is informational, not pass/fail | Run probe test, document firmware response behavior |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
