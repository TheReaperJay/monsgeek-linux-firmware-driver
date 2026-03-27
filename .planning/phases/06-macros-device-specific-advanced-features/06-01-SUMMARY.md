---
phase: 06-macros-device-specific-advanced-features
plan: 01
subsystem: protocol
tags: [validation, bounds-checking, macro, magnetic-switches, hid]

requires:
  - phase: 04-key-remapping-core
    provides: validate_dangerous_write function, bounds.rs, CommandSchemaMap
provides:
  - SET_MACRO bounds validation (macro_index <= 49, chunk_page <= 9)
  - SET_FN bounds validation (profile, key_index)
  - Magnetic command gating on non-magnetic devices
  - 14 unit tests covering all new validation branches
affects: [06-02, hardware-tests, magnetic-device-support]

tech-stack:
  added: []
  patterns: [magnetic-command-gating-per-device, failed-precondition-for-capability-checks]

key-files:
  created: []
  modified:
    - crates/monsgeek-driver/src/service/mod.rs
    - crates/monsgeek-protocol/src/command_schema.rs

key-decisions:
  - "CommandSchemaMap set_macro entry uses VariableWithMax(63) which is correct for bridge transport passthrough"
  - "Magnetic gating uses Status::failed_precondition (not invalid_argument) to distinguish capability vs bounds errors"
  - "SET_FN layer=0 for bounds check since SET_FN wire format lacks a layer byte"

patterns-established:
  - "Capability gating: use failed_precondition for device-lacks-feature, invalid_argument for out-of-bounds"
  - "Magnetic command array: local const array of 5 SET commands checked with contains()"

requirements-completed: [MACR-02, MAG-01, MAG-02, MAG-03, MAG-04]

duration: 4min
completed: 2026-03-27
---

# Phase 06 Plan 01: Macro/FN Bounds and Magnetic Command Gating Summary

**SET_MACRO/SET_FN bounds validation and magnetic command gating in validate_dangerous_write with 14 unit tests**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-27T06:44:21Z
- **Completed:** 2026-03-27T06:47:53Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Extended validate_dangerous_write with SET_MACRO bounds (macro_index 0-49, chunk_page 0-9)
- Extended validate_dangerous_write with SET_FN bounds (profile 0-3, key_index per device)
- Added magnetic command gating that rejects 5 SET commands on non-magnetic devices
- Verified CommandSchemaMap entries are already correct (set_macro, GET_MACRO, all magnetic entries present)
- 14 new unit tests covering all boundary conditions, short buffers, and capability checks

## Task Commits

Each task was committed atomically:

1. **Task 1: Audit CommandSchemaMap + add SET_MACRO, SET_FN, and magnetic command gating** - `eddb357` (feat)
2. **Task 2: Unit tests for SET_MACRO, SET_FN, and magnetic command validation** - `b85ea07` (test)

## Files Created/Modified
- `crates/monsgeek-driver/src/service/mod.rs` - Three new validation branches in validate_dangerous_write, 14 new tests, 4 test helpers
- `crates/monsgeek-protocol/src/command_schema.rs` - Audited (no changes needed, entries already correct)

## Decisions Made
- CommandSchemaMap set_macro entry uses `VariableWithMax(63)` -- correct for bridge transport which always delivers 63-byte zero-padded payloads. The 7-byte header structure is validated at the bounds level, not schema level.
- Magnetic command gating uses `Status::failed_precondition` to distinguish "device doesn't support this" from "argument out of range".
- SET_FN passes layer=0 to `validate_write_request` because the SET_FN wire format has no layer byte.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- validate_dangerous_write now covers all dangerous write paths: SET_KEYMATRIX, SET_KEYMATRIX_SIMPLE, SET_MACRO, SET_FN, and magnetic SET commands
- Plan 02 (hardware verification) can proceed with macro round-trip testing on real M5W hardware
- Magnetic commands are unit-tested only (M5W has noMagneticSwitch: true); hardware validation deferred until a magnetic device is available

---
*Phase: 06-macros-device-specific-advanced-features*
*Completed: 2026-03-27*
