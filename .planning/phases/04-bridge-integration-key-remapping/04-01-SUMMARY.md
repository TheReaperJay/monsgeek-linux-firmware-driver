---
phase: 04-bridge-integration-key-remapping
plan: 01
subsystem: api
tags: [grpc, bounds-validation, device-safety, tdd]

# Dependency graph
requires:
  - phase: 02-transport-session
    provides: TransportHandle, bounds::validate_write_request, TransportError::BoundsViolation
  - phase: 03-bridge-grpc
    provides: DriverService with gRPC send_command_rpc, ConnectedDevice, get_handle_for_path
provides:
  - DeviceDefinition cached in ConnectedDevice at connection time
  - SET_KEYMATRIX bounds validation in gRPC service layer (profile, key_index, layer)
  - validate_dangerous_write function for pre-transport write safety
  - find_connected_device shared lookup returning full ConnectedDevice
  - get_device_for_path returning (TransportHandle, DeviceDefinition) for callers needing both
affects: [04-02, 05-led-tuning]

# Tech tracking
tech-stack:
  added: []
  patterns: [service-layer-bounds-validation, cached-device-definition, tdd-red-green-refactor]

key-files:
  created:
    - crates/monsgeek-driver/tests/bounds_validation.rs
  modified:
    - crates/monsgeek-driver/src/service/mod.rs

key-decisions:
  - "validate_dangerous_write is a pub(crate) free function, not an impl method, for direct testability from unit tests"
  - "Extracted find_connected_device to eliminate lookup duplication between get_handle_for_path and get_device_for_path"
  - "MAX_PROFILE=3 as a module constant, matching firmware hard limit"

patterns-established:
  - "Service-layer validation: dangerous write commands validated at gRPC boundary before reaching transport"
  - "Device definition caching: ConnectedDevice carries DeviceDefinition from connection time through all operations"

requirements-completed: [KEYS-02]

# Metrics
duration: 4min
completed: 2026-03-25
---

# Phase 04 Plan 01: Bounds Validation Summary

**SET_KEYMATRIX bounds validation at gRPC service layer with DeviceDefinition caching, preventing OOB key_index/layer/profile from corrupting firmware flash**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-25T09:52:44Z
- **Completed:** 2026-03-25T09:57:07Z
- **Tasks:** 1 (TDD: 3 commits)
- **Files modified:** 2

## Accomplishments
- ConnectedDevice now caches DeviceDefinition at connection time for both initial scan and hot-plug paths
- send_command_rpc validates SET_KEYMATRIX bounds before forwarding to transport layer
- 7 unit tests covering valid, OOB key_index, OOB layer, OOB profile, short buffer, non-dangerous passthrough, and boundary values
- 1 integration test confirming bridge_transport remains device-agnostic (no validation at transport layer)

## Task Commits

Each task was committed atomically (TDD flow):

1. **Task 1 RED: Failing tests** - `8aed735` (test)
2. **Task 1 GREEN: Implementation** - `dee74bd` (feat)
3. **Task 1 REFACTOR: Extract shared lookup** - `fcad761` (refactor)

## Files Created/Modified
- `crates/monsgeek-driver/src/service/mod.rs` - Added DeviceDefinition to ConnectedDevice, validate_dangerous_write function, find_connected_device shared lookup, get_device_for_path, updated send_command_rpc with validation
- `crates/monsgeek-driver/tests/bounds_validation.rs` - Integration test proving bridge_transport does not validate bounds

## Decisions Made
- validate_dangerous_write is a pub(crate) free function rather than an associated method on DriverService -- it only needs DeviceDefinition and the msg buffer, no self reference, and this makes it directly testable from unit tests
- Extracted find_connected_device to deduplicate the device lookup logic previously copy-pasted between get_handle_for_path and get_device_for_path
- MAX_PROFILE = 3 as a module-level constant, matching the firmware's hard limit of 4 profiles (0-3)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Bounds validation is in place for SET_KEYMATRIX writes
- Ready for Plan 02 which will add the actual key remapping gRPC endpoints
- The validate_dangerous_write function is extensible for additional dangerous commands if needed

---
*Phase: 04-bridge-integration-key-remapping*
*Completed: 2026-03-25*
