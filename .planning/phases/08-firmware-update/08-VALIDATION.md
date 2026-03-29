---
phase: 8
slug: firmware-update
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-28
---

# Phase 8 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test + targeted integration simulations + guarded manual hardware run |
| **Config file** | Cargo workspace manifests (`Cargo.toml`, crate manifests) |
| **Quick run command** | `cargo test -p monsgeek-driver -p monsgeek-cli -p monsgeek-transport` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30-90 seconds (non-hardware path) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p monsgeek-driver -p monsgeek-cli -p monsgeek-transport`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `$gsd-verify-work`:** Full suite green + guarded manual firmware checkpoint on target hardware
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 08-01-01 | 01 | 1 | FW-01 | unit/integration | `cargo test -p monsgeek-transport usb_version` | ❌ W0 | ⬜ pending |
| 08-01-02 | 01 | 1 | FW-04 | unit | `cargo test -p monsgeek-driver firmware_preflight` | ❌ W0 | ⬜ pending |
| 08-01-03 | 01 | 1 | FW-03 | unit/cli | `cargo test -p monsgeek-cli firmware_confirmation` | ❌ W0 | ⬜ pending |
| 08-02-01 | 02 | 2 | FW-02 | integration (simulated) | `cargo test -p monsgeek-driver firmware_transfer_flow` | ❌ W0 | ⬜ pending |
| 08-02-02 | 02 | 2 | FW-02, FW-04 | integration (simulated) | `cargo test -p monsgeek-driver firmware_crc_and_retry` | ❌ W0 | ⬜ pending |
| 08-02-03 | 02 | 2 | FW-01, FW-02 | manual hardware | `scripts/firmware-flash-checkpoint.sh` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Shared firmware engine module scaffold (single source for CLI + bridge update flow)
- [ ] Deterministic mock/sim harness for bootloader transition and transfer phases
- [ ] CLI firmware command test module for confirmation + non-interactive gates
- [ ] Driver OTA stream tests for progress phase mapping and failure propagation
- [ ] Manual hardware runbook script (`scripts/firmware-flash-checkpoint.sh`) with explicit recovery notes

*Existing Rust test infrastructure is sufficient; no new external framework is required.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real bootloader re-enumeration and return to normal mode | FW-02 | Requires physical device USB lifecycle | Run guarded firmware update on M5W, verify bootloader transition, completion, and normal-mode return |
| Post-flash version verification on real hardware | FW-01 | Requires live firmware/device responses | Query `GET_USB_VERSION` and `GET_REV` after update; confirm expected version fields |
| Recovery guidance correctness after forced failure path | FW-02, FW-04 | Needs realistic failure handling on device path | Induce checksum mismatch in controlled test image, confirm error messaging and documented recovery steps |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 90s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
