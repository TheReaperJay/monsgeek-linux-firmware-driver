---
phase: 07-cli-service-deployment
plan: 02
subsystem: service-deployment
tags: [systemd, deployment, operations, cli, linux]

requires:
  - phase: 07-cli-service-deployment
    provides: bridge-first CLI from 07-01
provides:
  - Systemd unit packaging for `monsgeek-driver` and `monsgeek-inputd`
  - Install/uninstall automation scripts for deterministic operator flow
  - One-command service verification script (`verify-systemd-services.sh`)
  - Service smoke hook in `tools/test.sh`
  - Service deployment runbook (`docs/systemd.md`)
affects: [phase-closeout, operator-runbook, deployment-verification]

tech-stack:
  added: []
  patterns: [systemd-managed-lifecycle, script-driven-uat, service-smoke-validation]

key-files:
  created:
    - deploy/systemd/monsgeek-driver.service
    - deploy/systemd/monsgeek-inputd.service
    - scripts/install-systemd-services.sh
    - scripts/uninstall-systemd-services.sh
    - scripts/verify-systemd-services.sh
    - docs/systemd.md
    - .planning/phases/07-cli-service-deployment/07-02-SUMMARY.md
  modified:
    - tools/test.sh
    - tools/README.md

key-decisions:
  - "Use direct binary ExecStart paths in units for predictable supervision semantics."
  - "Automate human checkpoint steps into one root-run verification script to reduce manual error."
  - "Keep reboot verification optional (`--with-reboot`) while supporting non-reboot closeout path."

patterns-established:
  - "Service deployment checks are executable scripts, not ad-hoc manual command lists."
  - "CLI smoke validation is part of service lifecycle verification."

requirements-completed: [GRPC-09, CLI-01, CLI-02]

duration: in-session
completed: 2026-03-28
---

# Phase 07 Plan 02 Summary

**Shipped systemd deployment artifacts and automated verification flow for bridge/input daemon services, then validated CLI operation against managed services and restart-on-failure behavior.**

## Performance

- **Tasks completed:** 3/3
- **Files created/modified:** 9
- **Checkpoint outcome:** Approved (non-reboot path)

## Accomplishments

- Added production unit files:
  - `monsgeek-driver.service` with `Restart=on-failure`
  - `monsgeek-inputd.service` with dependency on driver service
- Added deterministic install/uninstall scripts for `/etc/systemd/system` deployment.
- Added service runbook (`docs/systemd.md`) and documented one-command verification flow.
- Extended `tools/test.sh` with `--service-smoke` mode for managed services.
- Added `scripts/verify-systemd-services.sh` to run:
  - install + enabled/active checks
  - CLI smoke (`devices list`, `info`)
  - crash/restart validation (`NRestarts` increment)
  - optional reboot verification mode (`--with-reboot`)

## Verification Evidence

- `systemd-analyze verify deploy/systemd/monsgeek-driver.service deploy/systemd/monsgeek-inputd.service` → pass
- `bash -n` checks passed for:
  - `scripts/install-systemd-services.sh`
  - `scripts/uninstall-systemd-services.sh`
  - `scripts/verify-systemd-services.sh`
  - `tools/test.sh`
- Human checkpoint execution (automated through script) passed:
  - Services reported enabled+active
  - `monsgeek-cli devices list` returned connected M5W
  - `monsgeek-cli info` succeeded against managed bridge
  - `pkill -9 monsgeek-driver` recovered to active with `NRestarts: 0 -> 1`

## Issues Encountered

- Initial verification failed with `ExecMainStatus=203` because `/usr/bin/monsgeek-driver` and related binaries were not installed.
- Resolved by building release binaries and installing:
  - `/usr/bin/monsgeek-driver`
  - `/usr/bin/monsgeek-inputd`
  - `/usr/bin/monsgeek-cli`
- Verification then passed.

## Deviations from Plan

- Added `scripts/verify-systemd-services.sh` (extra automation) to satisfy user preference for single-command verification instead of manual checkpoint execution.
- This reduced operational friction without changing core phase scope.

## Next Phase Readiness

- Phase 07 deliverables are operational on host with systemd-managed services and CLI smoke confirmation.
- Reboot persistence check remains available via `--with-reboot` when desired.
