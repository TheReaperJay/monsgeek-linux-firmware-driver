---
phase: 02-fea-protocol-hid-transport
plan: 02
subsystem: transport
tags: [rust, rusb, hid, flow-control, echo-matching, throttling, hot-plug, crossbeam-channel, transport-thread]

requires:
  - phase: 02-fea-protocol-hid-transport
    plan: 01
    provides: TransportError enum, UsbSession with vendor_set_report/vendor_get_report, bounds validation

provides:
  - flow_control::query_command with echo-matched retry (5 attempts, 100ms inter-read delay)
  - flow_control::send_command with fire-and-forget retry (3 attempts)
  - discovery::DeviceInfo struct with VID, PID, device_id, display_name, name, bus, address
  - discovery::enumerate_devices matching USB devices against DeviceRegistry
  - thread::CommandRequest for channel-based command dispatch
  - thread::TransportEvent enum (DeviceArrived, DeviceLeft) for hot-plug lifecycle
  - thread::spawn_transport_thread with 100ms inter-command throttling via Instant tracking
  - thread::spawn_hotplug_thread with hot-plug monitoring later corrected to `udev`
  - TransportHandle (Clone) with send_query, send_fire_and_forget, shutdown
  - connect() factory returning (TransportHandle, Receiver<TransportEvent>)

affects: [02-03, phase-3, phase-7]

tech-stack:
  added: []
  patterns: [echo-byte-matching-retry-loop, dedicated-transport-thread-with-channel, instant-based-throttling, hotplug-source-corrected-later]

key-files:
  created:
    - crates/monsgeek-transport/src/flow_control.rs
    - crates/monsgeek-transport/src/discovery.rs
    - crates/monsgeek-transport/src/thread.rs
  modified:
    - crates/monsgeek-transport/src/lib.rs

key-decisions:
  - "query_command sleeps DEFAULT_DELAY_MS (100ms) between SET_REPORT and GET_REPORT inside the retry loop, giving firmware time to prepare the response"
  - "Transport thread tracks Instant::now() for inter-command throttling rather than sleeping after every command -- only sleeps the remaining delta when commands arrive too fast"
  - "Hot-plug planning was later corrected from libusb callbacks to `udev` monitoring after host-side validation"
  - "DeviceRegistry API uses find_by_vid_pid (returns Vec<&DeviceDefinition>) not get_by_vid_pid -- corrected from plan interface spec"

patterns-established:
  - "Channel-based transport API: callers send CommandRequest via crossbeam channel, transport thread serializes all USB I/O"
  - "Echo matching: response[0] == cmd_byte validates firmware responded to the correct command"
  - "Hot-plug source later corrected to `udev`"

requirements-completed: [HID-01, HID-02, HID-03, HID-04]

duration: 6min
completed: 2026-03-19
---

# Phase 02 Plan 02: Flow Control and Transport Thread Summary

> Historical correction (2026-03-23): this summary predates host-side validation of hot-plug and identity behavior. The current planning truth is `udev` hot-plug, firmware-ID-aware discovery, and corrected M5W wired identity `0x3151:0x4015`.

**Echo-matched query with 5-retry loop, fire-and-forget send with 3-retry, dedicated transport thread enforcing 100ms inter-command throttling, corrected `udev` hot-plug direction, and channel-based TransportHandle API**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-19T12:07:04Z
- **Completed:** 2026-03-19T12:13:29Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Flow control layer implements echo-matched query (5 retries) and fire-and-forget send (3 retries), both skipping report ID byte via `&frame[1..]` per the 65/64-byte convention
- Dedicated transport thread serializes all HID I/O through a single OS thread with 100ms minimum inter-command delay (Instant-based throttling)
- Hot-plug behavior was later corrected to the `udev` model after host-side validation
- TransportHandle provides a Clone-able, channel-based API for Phase 3 (gRPC) and Phase 7 (CLI) consumers
- Device discovery enumerates USB devices against DeviceRegistry, producing DeviceInfo structs
- 116 total tests pass across workspace (73 protocol + 42 transport + 1 doc-test)

## Task Commits

Each task was committed atomically (TDD: RED then GREEN):

1. **Task 1: Flow control layer and device discovery**
   - `e023b27` (test) - Failing tests for flow_control and discovery
   - `d6f8b11` (feat) - Implement flow_control and discovery

2. **Task 2: Transport thread with command channel, throttling, hot-plug, and TransportHandle API**
   - `9785ac1` (test) - Failing tests for transport thread and TransportEvent
   - `806f907` (feat) - Implement transport thread, hot-plug, and TransportHandle

## Files Created/Modified
- `crates/monsgeek-transport/src/flow_control.rs` - Echo-matched query_command (5 retries) and fire-and-forget send_command (3 retries), both using build_command and vendor_set/get_report
- `crates/monsgeek-transport/src/discovery.rs` - DeviceInfo struct and enumerate_devices matching USB devices against DeviceRegistry
- `crates/monsgeek-transport/src/thread.rs` - CommandRequest, TransportEvent, spawn_transport_thread (throttled command loop), spawn_hotplug_thread
- `crates/monsgeek-transport/src/lib.rs` - TransportHandle (send_query, send_fire_and_forget, shutdown), connect() factory, module declarations and re-exports

## Decisions Made
- query_command sleeps 100ms between SET_REPORT and GET_REPORT inside the retry loop to give firmware time to prepare the response -- the transport thread also throttles 100ms between consecutive commands, but query_command needs an additional intra-command delay
- Transport thread uses Instant::now()-based delta throttling rather than unconditional sleep -- more efficient when commands arrive with natural spacing
- Hot-plug approach was later corrected away from libusb callbacks and toward `udev` monitoring
- Corrected plan's interface spec: DeviceRegistry uses `find_by_vid_pid` (returns `Vec<&DeviceDefinition>`) and `find_by_id` (returns `Option<&DeviceDefinition>`), not `get_by_vid_pid`/`get` as written in the plan

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Corrected DeviceRegistry API method names**
- **Found during:** Task 1 (discovery.rs implementation)
- **Issue:** Plan interface spec listed `registry.get_by_vid_pid()` and `registry.get()` but actual API is `find_by_vid_pid()` and `find_by_id()`
- **Fix:** Used correct method names from actual registry.rs source code
- **Files modified:** crates/monsgeek-transport/src/discovery.rs
- **Verification:** `cargo test -p monsgeek-transport` passes
- **Committed in:** d6f8b11

**2. [Historical correction] Hot-plug implementation direction changed after real host validation**
- **Found during:** later Phase 2 hardware validation
- **Issue:** libusb arrival callbacks were not reliable enough on the target Linux host
- **Fix:** planning truth updated to `udev`-based hot-plug monitoring
- **Files modified:** planning docs and transport implementation

---

**Total deviations:** 2 auto-fixed (1 bug, 1 blocking)
**Impact on plan:** Both auto-fixes necessary for correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- TransportHandle is ready for Phase 3 (gRPC bridge) to consume via `connect()` returning `(TransportHandle, Receiver<TransportEvent>)`
- Phase 7 CLI can use the same TransportHandle with blocking `send_query`/`send_fire_and_forget` calls
- Plan 03 (hardware integration tests) can verify flow control, throttling, and echo matching against real M5W hardware
- Hot-plug event channel is ready for Phase 3's `watchDevList` RPC to expose via gRPC streaming

## Self-Check: PASSED

All 4 created/modified files verified present. All 4 commit hashes (e023b27, d6f8b11, 9785ac1, 806f907) verified in git log.

---
*Phase: 02-fea-protocol-hid-transport*
*Completed: 2026-03-19*
