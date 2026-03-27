---
phase: 1
slug: project-scaffolding-device-registry
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-19
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test framework (cargo test) |
| **Config file** | None — Cargo.toml `[dev-dependencies]` section |
| **Quick run command** | `cargo test -p monsgeek-protocol` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p monsgeek-protocol`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | REG-01 | unit | `cargo test -p monsgeek-protocol -- test_m5w_device_definition` | ❌ W0 | ⬜ pending |
| 01-01-02 | 01 | 1 | REG-01 | unit | `cargo test -p monsgeek-protocol -- test_m5w_identity` | ❌ W0 | ⬜ pending |
| 01-01-03 | 01 | 1 | REG-02 | unit | `cargo test -p monsgeek-protocol -- test_registry_extensible` | ❌ W0 | ⬜ pending |
| 01-01-04 | 01 | 1 | REG-02 | unit | `cargo test -p monsgeek-protocol -- test_add_device_json` | ❌ W0 | ⬜ pending |
| 01-02-01 | 02 | 1 | SC-4 | unit | `cargo test -p monsgeek-protocol -- test_command_constants` | ❌ W0 | ⬜ pending |
| 01-02-02 | 02 | 1 | SC-4 | unit | `cargo test -p monsgeek-protocol -- test_checksum_bit7` | ❌ W0 | ⬜ pending |
| 01-02-03 | 02 | 1 | SC-4 | unit | `cargo test -p monsgeek-protocol -- test_build_command` | ❌ W0 | ⬜ pending |
| 01-02-04 | 02 | 1 | SC-4 | unit | `cargo test -p monsgeek-protocol -- test_protocol_family_detect` | ❌ W0 | ⬜ pending |
| 01-02-05 | 02 | 1 | SC-4 | unit | `cargo test -p monsgeek-protocol -- test_protocol_family_pid` | ❌ W0 | ⬜ pending |
| 01-00-01 | 00 | 0 | SC-1 | build | `cargo build --workspace` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `Cargo.toml` — workspace root with three member crates
- [ ] `crates/monsgeek-protocol/Cargo.toml` — protocol crate with serde, serde_json, thiserror, glob
- [ ] `crates/monsgeek-transport/Cargo.toml` — empty shell crate
- [ ] `crates/monsgeek-driver/Cargo.toml` — minimal binary crate
- [ ] `crates/monsgeek-protocol/devices/m5w.json` — M5W device definition
- [ ] `.gitignore` — Rust standard gitignore

*Entire project is greenfield — Wave 0 IS the scaffolding.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| M5W JSON field accuracy | REG-01 | Data extracted from JS bundle requires human verification against known device specs | Compare m5w.json values against MonsGeek product page and reference app |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
