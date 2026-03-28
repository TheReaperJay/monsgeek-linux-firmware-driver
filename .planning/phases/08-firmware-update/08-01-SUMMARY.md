---
phase: 08-firmware-update
plan: 01
subsystem: firmware-update
tags: [firmware, cli, preflight, safety-gates, m5w]

requires: []
provides:
  - Shared `monsgeek-firmware` crate with reusable engine + preflight contracts
  - CLI firmware command surface (`version`, `validate`, `flash`) with strict gate enforcement
  - Regression tests for interactive phrase checks, non-interactive dual-flag checks, and metadata override behavior
affects: [bridge-ota, firmware-runbook, phase-08-wave-2]

tech-stack:
  added: [monsgeek-firmware crate]
  patterns: [shared-firmware-domain-layer, preflight-first-flash-flow, dual-flag-risk-ack]

key-files:
  created:
    - crates/monsgeek-firmware/Cargo.toml
    - crates/monsgeek-firmware/src/lib.rs
    - crates/monsgeek-firmware/src/manifest.rs
    - crates/monsgeek-firmware/src/progress.rs
    - crates/monsgeek-firmware/src/engine.rs
    - crates/monsgeek-firmware/src/preflight.rs
    - crates/monsgeek-cli/Cargo.toml
    - crates/monsgeek-cli/src/lib.rs
    - crates/monsgeek-cli/src/main.rs
    - crates/monsgeek-cli/src/commands.rs
    - crates/monsgeek-cli/src/format.rs
    - crates/monsgeek-cli/tests/firmware_cli.rs
    - .planning/phases/08-firmware-update/08-01-SUMMARY.md
  modified: []

key-decisions:
  - "Version query uses GET_USB_VERSION as required path and best-effort GET_REV probe."
  - "Shared preflight checks are executed before any flash path; flash currently exits after successful preflight until transport integration in 08-02."
  - "Validate command runs the same shared preflight without confirmation-gate failures by using non-interactive safe validation mode."

patterns-established:
  - "Firmware safety gates are centralized in monsgeek-firmware and consumed by CLI adapters."
  - "Non-interactive flashing requires explicit risk acknowledgment separate from --yes."

requirements-completed: [FW-01, FW-03, FW-04]

duration: in-session
completed: 2026-03-28
---

# Phase 08 Plan 01 Summary

**Delivered a reusable firmware-update domain crate and CLI firmware command surface that enforces metadata, confirmation, and non-interactive risk gates before any bootloader path.**

## Performance

- **Duration:** in-session
- **Started:** 2026-03-28T09:18:00Z
- **Completed:** 2026-03-28T10:00:04Z
- **Tasks:** 3
- **Files modified:** 12

## Accomplishments

- Created `monsgeek-firmware` with transfer markers (`0xBA 0xC0`, `0xBA 0xC2`), progress phases, and chunk/checksum helpers (including 0xFF padding behavior).
- Implemented strict preflight policy gates in shared code: compatibility validation, metadata mismatch hard-blocks, typed phrase requirement (`FLASH M5W`), dual non-interactive confirmation flags, and backup-failure override behavior.
- Added CLI `firmware version|validate|flash` commands and regression tests for safety-gate behavior; `flash` now performs shared preflight and intentionally aborts with `not implemented yet` until bridge transport integration (08-02).

## Task Commits

1. **Task 1: Scaffold shared firmware engine crate and canonical domain types** - `bd63994` (feat)
2. **Task 2: Implement preflight validation and safety policy gates** - `cc1052f` (feat)
3. **Task 3: Add CLI firmware commands and tests for version/validate/flash safety behavior** - `f5bf9b4` (feat)

## Files Created/Modified

- `crates/monsgeek-firmware/Cargo.toml` - new shared firmware workspace crate dependencies
- `crates/monsgeek-firmware/src/lib.rs` - exported firmware engine + preflight API
- `crates/monsgeek-firmware/src/manifest.rs` - firmware manifest model + compatibility field validation
- `crates/monsgeek-firmware/src/progress.rs` - explicit phase-based firmware progress model
- `crates/monsgeek-firmware/src/engine.rs` - engine traits, transfer markers, padded checksum helper
- `crates/monsgeek-firmware/src/preflight.rs` - policy enforcement and preflight regression tests
- `crates/monsgeek-cli/Cargo.toml` - added firmware/shared transport dependencies
- `crates/monsgeek-cli/src/lib.rs` - firmware CLI subcommand definitions
- `crates/monsgeek-cli/src/main.rs` - firmware command surface import marker
- `crates/monsgeek-cli/src/commands.rs` - firmware command execution + shared preflight integration
- `crates/monsgeek-cli/src/format.rs` - command output detail rendering for firmware operations
- `crates/monsgeek-cli/tests/firmware_cli.rs` - FW safety-gate regression tests

## Decisions Made

- Used one shared preflight decision path (`PreflightDecision`) for both validate and flash command entrypoints.
- Implemented `firmware version` to send `GET_USB_VERSION` first and treat `GET_REV` as optional best-effort for compatibility with devices that do not expose revision.
- Returned an explicit not-implemented error after successful flash preflight so no bootloader transfer is attempted before Wave 2 bridge integration.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- Network access to crates.io was unavailable in this environment; verification used `--offline` cargo mode.
- Full `cargo test -p monsgeek-driver` includes gRPC socket bind tests that fail under sandbox permissions (`Operation not permitted`) but unrelated driver unit/integration tests passed.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Wave 2 can now reuse the shared firmware engine/preflight APIs from `monsgeek-firmware` for bridge OTA wiring.
- CLI safety-gate tests are in place to detect regressions while bridge transfer/retry/post-verify logic is added.

## Self-Check: PASSED

- `key-files.created` spot-check: `crates/monsgeek-firmware/src/engine.rs` and `crates/monsgeek-cli/tests/firmware_cli.rs` exist.
- Commits present for `08-01`: `bd63994`, `cc1052f`, `f5bf9b4`.
- No `## Self-Check: FAILED` marker.

---
*Phase: 08-firmware-update*
*Completed: 2026-03-28*
