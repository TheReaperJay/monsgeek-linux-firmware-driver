---
phase: 7
slug: cli-service-deployment
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-27
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test + shell smoke checks + systemd-analyze |
| **Config file** | Cargo workspace manifests + unit files under `deploy/systemd/` |
| **Quick run command** | `cargo test -p monsgeek-cli -p monsgeek-driver` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~20-40 seconds (without hardware) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p monsgeek-cli -p monsgeek-driver`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `$gsd-verify-work`:** Full suite green + service smoke check (`systemctl is-active` + CLI command)
- **Max feedback latency:** 40 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 07-01-01 | 01 | 1 | CLI-01 | unit/build | `cargo check -p monsgeek-cli` | ❌ W0 | ⬜ pending |
| 07-01-02 | 01 | 1 | CLI-01, CLI-02 | unit | `cargo test -p monsgeek-cli` | ❌ W0 | ⬜ pending |
| 07-01-03 | 01 | 1 | CLI-01 | integration | `cargo test -p monsgeek-cli cli_smoke` | ❌ W0 | ⬜ pending |
| 07-02-01 | 02 | 2 | GRPC-09 | static validation | `systemd-analyze verify deploy/systemd/monsgeek-driver.service deploy/systemd/monsgeek-inputd.service` | ❌ W0 | ⬜ pending |
| 07-02-02 | 02 | 2 | GRPC-09 | shell/docs | `bash -n scripts/install-systemd-services.sh && bash -n scripts/uninstall-systemd-services.sh` | ❌ W0 | ⬜ pending |
| 07-02-03 | 02 | 2 | GRPC-09, CLI-01 | manual service smoke | `systemctl is-active monsgeek-driver.service monsgeek-inputd.service && monsgeek-cli info` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/monsgeek-cli/` crate scaffold exists with command modules and tests
- [ ] `deploy/systemd/monsgeek-driver.service` created
- [ ] `deploy/systemd/monsgeek-inputd.service` created
- [ ] `scripts/install-systemd-services.sh` + `scripts/uninstall-systemd-services.sh` created
- [ ] `docs/systemd.md` created with verification runbook

*Existing Rust and shell tooling cover framework setup; no new external test framework required.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Boot auto-start for both services | GRPC-09 | Requires real host reboot/systemd state | Enable both services, reboot host, confirm `systemctl is-enabled` and `is-active` for both units |
| Restart-on-failure behavior | GRPC-09 | Requires killing real process under systemd supervision | `pkill -9 monsgeek-driver`, then confirm service returns to active and restart counter increments |
| Real keyboard command behavior via CLI | CLI-01 | Requires connected device hardware | Run `monsgeek-cli info`, `led get`, `debounce get`, `profile get`, then one safe write and verify readback |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 40s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending

