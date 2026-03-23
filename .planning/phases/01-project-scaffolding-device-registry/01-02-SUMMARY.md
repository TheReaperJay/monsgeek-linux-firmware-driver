---
phase: 01-project-scaffolding-device-registry
plan: 02
subsystem: protocol
tags: [rust, hid, checksum, protocol-detection, constants, ble]

requires:
  - phase: 01-project-scaffolding-device-registry (plan 01)
    provides: Cargo workspace, DeviceDefinition, ProtocolError types

provides:
  - FEA command constants (cmd.rs) for all SET/GET/dongle operations
  - Magnetism sub-command constants for Hall Effect parameter addressing
  - ChecksumType enum with Bit7/Bit8/None variants
  - calculate_checksum, apply_checksum, build_command, build_ble_command functions
  - ProtocolFamily enum with detect() for YiChip vs RY5088 identification
  - CommandTable with divergent command byte mappings per family
  - Timing constants (wired HID delays, dongle polling)
  - HID constants (report sizes, usage pages, interface numbers)
  - BLE constants (vendor report ID, markers, report size)
  - RGB/LED constants (page sizes, matrix size)
  - Precision version thresholds for firmware capability detection

affects: [phase-2, phase-3, phase-4, phase-5, phase-6, phase-7]

tech-stack:
  added: []
  patterns: [checksum-excludes-report-id, protocol-family-detect-by-name-then-pid, constant-module-per-domain]

key-files:
  created:
    - crates/monsgeek-protocol/src/cmd.rs
    - crates/monsgeek-protocol/src/magnetism.rs
    - crates/monsgeek-protocol/src/checksum.rs
    - crates/monsgeek-protocol/src/protocol.rs
    - crates/monsgeek-protocol/src/timing.rs
    - crates/monsgeek-protocol/src/hid.rs
    - crates/monsgeek-protocol/src/ble.rs
    - crates/monsgeek-protocol/src/rgb.rs
    - crates/monsgeek-protocol/src/precision.rs
  modified:
    - crates/monsgeek-protocol/src/lib.rs

key-decisions:
  - "ChecksumType uses serde Serialize/Deserialize for future config persistence"
  - "Protocol family detection prioritizes device name prefix over PID heuristic to handle cross-family PID collisions"

patterns-established:
  - "build_command applies checksum to buf[1..] (excluding report ID byte 0) matching reference implementation"
  - "build_ble_command applies checksum to buf[2..] (excluding report ID and marker bytes)"
  - "Constant modules are flat with inline tests, not nested in a parent constants module"

requirements-completed: [REG-01, REG-02]

duration: 4min
completed: 2026-03-19
---

# Phase 01 Plan 02: FEA Protocol Constants & Checksum Algorithms Summary

> Historical correction (2026-03-23): this summary remains useful for protocol structure, but any M5W-specific VID/PID assumptions inherited from early planning should be treated as superseded by the corrected registry and planning documents.

**FEA command opcodes, Bit7/Bit8 checksum algorithms, USB/BLE command builders, and YiChip vs RY5088 protocol family detection with 62 unit tests**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-19T09:52:18Z
- **Completed:** 2026-03-19T09:56:45Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- All 32 SET commands, 21 GET commands, 11 dongle commands, and 1 response status transcribed byte-for-byte from reference implementation
- Checksum algorithms (Bit7, Bit8, None) verified against known test vectors: 0x8F command produces checksum 0x70, multi-byte payload sums correctly
- build_command produces 65-byte USB buffers with report ID at byte 0, command at byte 1, checksum correctly applied to buf[1..] (not buf[0..])
- build_ble_command produces 66-byte BLE buffers with vendor report ID 0x06, marker 0x55, and checksum applied to buf[2..]
- ProtocolFamily::detect correctly identifies YiChip by 4 name prefixes (yc500_, yc300_, yc3121_, yc3123_), RY5088 by 2 prefixes (ry5088_, ry1086_), falls back to PID heuristic (0x40xx = YiChip), defaults to RY5088
- RY5088 and YiChip command tables encode all 14 divergent command byte assignments with Option<u8> for family-exclusive commands

## Task Commits

Each task was committed atomically:

1. **Task 1: FEA command constants, magnetism sub-commands, and constant modules** - `1dc5818` (feat)
2. **Task 2: Checksum algorithms, protocol families, and build_command** - `c78afec` (feat)

## Files Created/Modified
- `crates/monsgeek-protocol/src/cmd.rs` - 32 SET + 21 GET + 11 dongle command constants with name() lookup
- `crates/monsgeek-protocol/src/magnetism.rs` - 13 magnetism sub-command constants with name() lookup
- `crates/monsgeek-protocol/src/checksum.rs` - ChecksumType enum, calculate/apply_checksum, build_command, build_ble_command
- `crates/monsgeek-protocol/src/protocol.rs` - ProtocolFamily enum, detect(), CommandTable, RY5088/YICHIP static tables
- `crates/monsgeek-protocol/src/timing.rs` - HID timing constants and dongle polling sub-module
- `crates/monsgeek-protocol/src/hid.rs` - Report sizes, usage pages, interface numbers, is_vendor_usage_page()
- `crates/monsgeek-protocol/src/ble.rs` - BLE vendor report ID, markers, report size, delay
- `crates/monsgeek-protocol/src/rgb.rs` - LED data sizes, page counts, matrix size
- `crates/monsgeek-protocol/src/precision.rs` - Firmware version thresholds for precision levels
- `crates/monsgeek-protocol/src/lib.rs` - Module declarations and re-exports for all 9 new modules

## Decisions Made
- ChecksumType derives serde Serialize/Deserialize for future device config persistence (required by plan)
- Protocol family detection checks name before PID to correctly handle ry1086_ devices that have 0x40xx PIDs
- Constant modules are flat (e.g., `crate::cmd::SET_LEDPARAM`) rather than nested under a parent constants module, matching the reference implementation structure

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Protocol constants and checksum algorithms are the foundation for Phase 2 (HID transport): command opcodes to construct messages, checksum logic to validate them, timing constants for inter-command delays
- ProtocolFamily::detect will be called during device connection to select the correct CommandTable
- build_command and build_ble_command are ready for use by transport layer implementations
- 73 total tests pass across the workspace (15 pre-existing + 58 new)

## Self-Check: PASSED

All 10 created/modified files verified present. Both commit hashes (1dc5818, c78afec) verified in git log.

---
*Phase: 01-project-scaffolding-device-registry*
*Completed: 2026-03-19*
