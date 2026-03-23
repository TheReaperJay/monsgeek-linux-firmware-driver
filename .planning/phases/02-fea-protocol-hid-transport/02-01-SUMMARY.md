---
phase: 02-fea-protocol-hid-transport
plan: 01
subsystem: transport
tags: [rust, rusb, usb, hid, control-transfers, udev, bounds-validation]

requires:
  - phase: 01-project-scaffolding-device-registry
    provides: DeviceDefinition struct, Cargo workspace, thiserror patterns

provides:
  - TransportError enum with 8 variants covering all transport-layer error cases
  - From<rusb::Error> impl mapping all rusb errors uniformly to Usb(String)
  - UsbSession struct with open/open_at, vendor_set_report (64-byte), vendor_get_report (returns [u8; 64] by value)
  - HID class constants verified against reference (REQUEST_TYPE_OUT=0x21, HID_SET_REPORT=0x09, FEATURE_REPORT_WVALUE=0x0300)
  - validate_key_index and validate_write_request for key matrix bounds checking
  - deploy/99-monsgeek.rules udev rules for non-root access (the earlier IF2-unbind assumption was later superseded)
  - 21 unit tests covering error types, HID constants, bounds validation, udev file smoke test

affects: [02-02, 02-03, phase-3, phase-4, phase-5, phase-7]

tech-stack:
  added: [rusb 0.9, log 0.4, crossbeam-channel 0.5]
  patterns: [from-rusb-error-uniform-to-usb-string, timeout-variant-reserved-for-flow-control, vendor-get-report-returns-by-value, assert-64-byte-set-report]

key-files:
  created:
    - crates/monsgeek-transport/src/error.rs
    - crates/monsgeek-transport/src/usb.rs
    - crates/monsgeek-transport/src/bounds.rs
    - crates/monsgeek-transport/deploy/99-monsgeek.rules
  modified:
    - crates/monsgeek-transport/Cargo.toml
    - crates/monsgeek-transport/src/lib.rs
    - Cargo.lock

key-decisions:
  - "From<rusb::Error> maps ALL variants to Usb(String) -- TransportError::Timeout is reserved for flow_control layer which has command byte context"
  - "vendor_get_report returns [u8; 64] by value (not via mutable buffer parameter) for cleaner flow_control API"
  - "UsbSession detaches kernel drivers from IF0/IF1/IF2 but only claims IF2 (vendor interface)"
  - "vendor_set_report asserts data.len() == 64 at runtime (panic on programming error, not Result)"

patterns-established:
  - "From<rusb::Error> is a uniform mapping -- domain-specific error variants are constructed by higher layers with richer context"
  - "64-byte payload convention: build_command returns 65 bytes, callers pass &frame[1..] to vendor_set_report"
  - "DeviceDefinition bounds checking before any key matrix USB write"

requirements-completed: [HID-02, HID-05, HID-06]

duration: 5min
completed: 2026-03-19
---

# Phase 02 Plan 01: Transport Foundation Summary

> Historical correction (2026-03-23): this summary predates live hardware validation. The earlier M5W USB IDs and IF2-unbind-centric assumptions were superseded by verified M5W behavior: wired `0x3151:0x4015`, 32-bit `GET_USB_VERSION` device identity, and `HID_QUIRK_IGNORE` plus explicit `IF0` handoff as the current host setup.

**TransportError enum, rusb-based UsbSession with HID control transfers on IF2, key matrix bounds validation, and udev rules for non-root keyboard access**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-19T11:57:33Z
- **Completed:** 2026-03-19T12:03:10Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- TransportError enum covers all 8 error cases needed across Phase 2: Usb, Timeout, EchoMismatch, DeviceNotFound, BoundsViolation, KernelDriverActive, Disconnected, ChannelClosed
- UsbSession wraps rusb::DeviceHandle with vendor_set_report (64-byte assert) and vendor_get_report (returns [u8; 64] by value), ready for flow_control layer to build upon
- Key matrix bounds validation prevents firmware OOB corruption: validate_key_index checks raw bounds, validate_write_request extracts bounds from DeviceDefinition
- Udev rules grant non-root USB access; the earlier IF2-unbind-first workaround was later superseded by the hardware-validated host setup
- 94 total tests pass across workspace (73 Phase 1 + 21 new transport tests)

## Task Commits

Each task was committed atomically:

1. **Task 1: Transport error types, USB session with control transfers** - `d4e5620` (feat)
2. **Task 2: Key matrix bounds validation and udev rules** - `5d63c42` (feat)

## Files Created/Modified
- `crates/monsgeek-transport/Cargo.toml` - Added rusb 0.9, thiserror 2.0, log 0.4, crossbeam-channel 0.5, hardware feature gate
- `crates/monsgeek-transport/src/lib.rs` - Module declarations and re-exports for error, usb, bounds
- `crates/monsgeek-transport/src/error.rs` - TransportError enum with 8 variants, From<rusb::Error> impl, 8 unit tests
- `crates/monsgeek-transport/src/usb.rs` - UsbSession struct, open/open_at, vendor_set_report/vendor_get_report, HID constants, 4 unit tests
- `crates/monsgeek-transport/src/bounds.rs` - validate_key_index, validate_write_request against DeviceDefinition, 9 unit tests
- `crates/monsgeek-transport/deploy/99-monsgeek.rules` - Udev rules for non-root access and IF2 unbind
- `Cargo.lock` - Updated with new dependencies

## Decisions Made
- From<rusb::Error> maps ALL variants uniformly to Usb(String) -- the Timeout variant with command byte context is exclusively for the flow_control layer (Plan 02) to construct
- vendor_get_report returns [u8; 64] by value rather than taking a mutable buffer parameter, matching the API contract Plan 02 depends on
- UsbSession detaches kernel drivers from all 3 interfaces but only claims IF2, leaving IF0 keyboard functionality to the kernel
- vendor_set_report uses assert (panic) rather than Result for wrong-size data -- this is a programming error, not a runtime condition
- Removed redundant rusb::Context field from UsbSession since DeviceHandle<Context> already stores a clone of the context internally

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Error types and UsbSession are ready for Plan 02 (flow_control.rs) to build retry logic, echo matching, and throttling on top of
- validate_key_index/validate_write_request are ready for Plan 03 (transport thread) to call before key matrix writes
- Udev rules are ready for installation and hardware testing
- crossbeam-channel dependency is in Cargo.toml for Plan 03's transport thread channel

## Self-Check: PASSED

All 7 created/modified files verified present. Both commit hashes (d4e5620, 5d63c42) verified in git log.

---
*Phase: 02-fea-protocol-hid-transport*
*Completed: 2026-03-19*
