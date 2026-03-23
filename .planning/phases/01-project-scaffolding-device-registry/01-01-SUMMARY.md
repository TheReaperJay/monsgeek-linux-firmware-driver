---
phase: 01-project-scaffolding-device-registry
plan: 01
subsystem: protocol
tags: [rust, serde, workspace, device-registry, json]

requires:
  - phase: none
    provides: greenfield project

provides:
  - Cargo workspace with three crates (monsgeek-protocol, monsgeek-transport, monsgeek-driver)
  - DeviceDefinition struct with serde camelCase JSON deserialization
  - DeviceRegistry with directory scanning and multi-index lookup (by ID, by VID/PID)
  - M5W device JSON definition (corrected later to wired VID 0x3151, PID 0x4015, ID 1308)
  - Error types (ProtocolError, RegistryError) with thiserror
  - 15 unit tests covering device deserialization, magnetism logic, registry operations

affects: [01-02, phase-2, phase-3, phase-4, phase-5, phase-6, phase-7]

tech-stack:
  added: [serde 1.0, serde_json 1.0, thiserror 2.0, glob 0.3]
  patterns: [one-json-per-device registry, multi-index HashMap lookup, serde rename_all camelCase]

key-files:
  created:
    - Cargo.toml
    - .gitignore
    - crates/monsgeek-protocol/Cargo.toml
    - crates/monsgeek-protocol/src/lib.rs
    - crates/monsgeek-protocol/src/error.rs
    - crates/monsgeek-protocol/src/device.rs
    - crates/monsgeek-protocol/src/registry.rs
    - crates/monsgeek-protocol/devices/m5w.json
    - crates/monsgeek-transport/Cargo.toml
    - crates/monsgeek-transport/src/lib.rs
    - crates/monsgeek-driver/Cargo.toml
    - crates/monsgeek-driver/src/main.rs
  modified: []

key-decisions:
  - "Used Rust edition 2024 for all crates (latest stable on rustc 1.93.1)"
  - "firmware/ and references/ directories excluded via .gitignore (data extraction sources, not project deliverables)"

patterns-established:
  - "One JSON file per device in crates/monsgeek-protocol/devices/ for extensibility"
  - "DeviceRegistry multi-index: HashMap<i32, DeviceDefinition> + HashMap<(u16,u16), Vec<i32>>"
  - "serde rename_all camelCase on all device-related structs for JSON interop"
  - "thiserror 2.0 for error types with domain-specific variants"

requirements-completed: [REG-01, REG-02]

duration: 4min
completed: 2026-03-19
---

# Phase 01 Plan 01: Workspace Scaffolding & Device Registry Summary

> Historical correction (2026-03-23): this summary captured an early, incorrect M5W USB identity extraction. The verified wired M5W USB identity is `0x3151:0x4015`, not `0x3141:0x4005`. Treat the registry/device-architecture work here as valid, but the specific M5W USB constants as superseded.

**Rust workspace with three crates, JSON-driven DeviceRegistry with M5W definition (later corrected to wired VID 0x3151, PID 0x4015, ID 1308), and 15 unit tests**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-19T09:45:32Z
- **Completed:** 2026-03-19T09:49:28Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments
- Cargo workspace compiles with three crates: monsgeek-protocol (lib), monsgeek-transport (lib shell), monsgeek-driver (binary)
- M5W device definition loads from JSON with the intended device metadata; the wired USB VID/PID values were corrected later during real hardware validation to `0x3151:0x4015`
- DeviceRegistry scans devices/ directory for *.json files and indexes by device ID and VID/PID
- Adding a new JSON file to devices/ is verified (via test_registry_extensible) to work without code changes
- has_magnetism() correctly handles all three cases: magnetism flag, no_magnetic_switch flag, default

## Task Commits

Each task was committed atomically:

1. **Task 1: Workspace scaffolding, device types, and M5W JSON** - `968ae2c` (feat)
2. **Task 2: Device registry with directory scanning and multi-index lookup** - `7e81840` (test)

## Files Created/Modified
- `Cargo.toml` - Workspace root with members = ["crates/*"]
- `.gitignore` - Rust gitignore excluding firmware/ and references/
- `crates/monsgeek-protocol/Cargo.toml` - Protocol crate with serde, serde_json, thiserror, glob
- `crates/monsgeek-protocol/src/lib.rs` - Crate root with pub mod and re-exports
- `crates/monsgeek-protocol/src/error.rs` - ProtocolError and RegistryError enums
- `crates/monsgeek-protocol/src/device.rs` - DeviceDefinition, FnSysLayer, TravelSetting, RangeConfig structs with 7 unit tests
- `crates/monsgeek-protocol/src/registry.rs` - DeviceRegistry with directory scanning and 8 unit tests
- `crates/monsgeek-protocol/devices/m5w.json` - M5W device definition
- `crates/monsgeek-transport/Cargo.toml` - Transport crate (empty shell for Phase 2)
- `crates/monsgeek-transport/src/lib.rs` - Empty shell with doc comment
- `crates/monsgeek-driver/Cargo.toml` - Driver binary crate
- `crates/monsgeek-driver/src/main.rs` - Minimal binary printing version

## Decisions Made
- Used Rust edition 2024 for all crates (latest stable on rustc 1.93.1)
- Excluded firmware/ and references/ from git via .gitignore (large binary data extraction sources, not project deliverables)
- Implemented DeviceRegistry in Task 1 alongside the types (needed for workspace to compile), added dedicated registry tests in Task 2

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Workspace foundation complete for plan 01-02 (FEA protocol constants, checksum algorithms, protocol family detection)
- monsgeek-protocol crate is ready to receive additional modules (cmd.rs, checksum.rs, protocol.rs, timing.rs, etc.)
- DeviceDefinition and DeviceRegistry types are available via pub use exports for downstream crates

## Self-Check: PASSED

All 12 created files verified present. Both commit hashes (968ae2c, 7e81840) verified in git log.

---
*Phase: 01-project-scaffolding-device-registry*
*Completed: 2026-03-19*
