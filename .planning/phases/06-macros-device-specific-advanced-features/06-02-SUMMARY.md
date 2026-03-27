---
phase: 06-macros-device-specific-advanced-features
plan: 02
subsystem: driver-transport-runtime
tags: [grpc, discovery, runtime-pid, device-registry, uat, hardware-stability]

requires:
  - phase: 06-macros-device-specific-advanced-features
    provides: bounds gating and command policy from 06-01
provides:
  - Runtime PID alias support via central device registry JSON (`runtimePids`)
  - Non-destructive discovery path (probe no longer resets devices during discovery failures)
  - Deterministic device path resolution in watch flow (`watchDevList` no longer initializes empty on M5W runtime PID 0x4011)
  - Fast rejection for empty `devicePath` requests (`InvalidArgument` instead of ambiguous not-found flow)
  - One-command verification harness (`tools/test.sh`) with layer and macro stress modes
affects: [06-UAT, web-bridge-stability, firmware-command-path, hotplug-discovery]

tech-stack:
  added: []
  patterns: [registry-driven-runtime-aliases, non-destructive-probing, fail-fast-rpc-validation, scripted-hardware-uat]

key-files:
  created:
    - .planning/phases/06-macros-device-specific-advanced-features/06-02-SUMMARY.md
    - tools/test.sh
  modified:
    - crates/monsgeek-protocol/devices/m5w.json
    - crates/monsgeek-protocol/src/device.rs
    - crates/monsgeek-protocol/src/registry.rs
    - crates/monsgeek-transport/src/discovery.rs
    - crates/monsgeek-driver/src/service/mod.rs
    - .planning/phases/06-macros-device-specific-advanced-features/06-UAT.md

key-decisions:
  - "Do not hardcode runtime PID behavior in transport/driver logic; model runtime aliasing in device JSON (`runtimePids`) and resolve through registry APIs."
  - "Discovery must be non-destructive: probe failures should not trigger reset loops during device list initialization."
  - "Treat blank RPC paths as contract violations (`InvalidArgument`) to surface caller errors immediately."
  - "Codify UAT as executable script (`tools/test.sh`) to remove manual command sequencing drift."

patterns-established:
  - "Registry as single source of truth for runtime compatibility mapping (canonical PID + alias PIDs)."
  - "Probe fallback from firmware ID query to unique registry runtime alias match."
  - "Stress-first validation gate before phase verification closure."

requirements-completed: [MACR-01, MACR-02, MAG-01, MAG-02, MAG-03, MAG-04]

duration: 1h
completed: 2026-03-27
---

# Phase 06 Plan 02: Runtime Path Stability and Firmware Command Reliability Summary

**Fixed the runtime device-path collapse that caused empty `watchDevList` init and unstable firmware command sessions, then validated layer/macro command stability under stress.**

## Performance

- **Duration:** 1h
- **Tasks:** 3 (root-cause, remediation, hardware UAT automation)
- **Files modified/created:** 7+ core files + 1 test harness script

## Root Cause

- M5W was frequently enumerating on runtime PID `0x4011` while registry canonical PID was `0x4015`.
- Discovery path was filtering/handling candidates in a way that allowed `watch_dev_list` to initialize with `0 device(s)`, yielding blank `DEVPATH`.
- Discovery probe behavior could enter destructive recovery behavior during list initialization, increasing instability in the firmware-facing path.

## Accomplishments

- Added `runtimePids` to device registry schema and M5W profile (`0x4011` alias) so runtime PID handling is registry-driven, not hardcoded.
- Added runtime-aware registry query APIs (`find_by_runtime_vid_pid`, `supports_runtime_vid_pid`).
- Updated discovery to:
  - honor runtime PID aliases from registry,
  - avoid reset-based destructive recovery during discovery query failures,
  - fallback to unique runtime alias mapping when firmware-ID query fails.
- Added fail-fast RPC guard for empty `devicePath` (`device path is empty` -> `InvalidArgument`).
- Added/extended tests for empty-path handling and runtime alias resolution.
- Built `tools/test.sh` one-command harness with:
  - smoke path validation,
  - `--layer-stress`,
  - `--macro-stress`,
  - configurable iteration/timeout knobs.

## Validation Evidence

- `watchDevList` now returns initialized device list on runtime PID path:
  - `watch_dev_list: sending Init with 1 device(s)`
  - path resolved: `3151-4011-ffff-0002-1@id1308-b003-a006-n1`
- Layer stress:
  - `bash tools/test.sh --layer-stress --iterations 200 --read-timeout 1`
  - Result: `200/200` iterations passed.
- Macro stress:
  - `bash tools/test.sh --macro-stress --macro-iterations 200 --macro-read-timeout 1`
  - Result: `200/200` iterations passed.
- Combined run:
  - `bash tools/test.sh --layer-stress --macro-stress`
  - Result: both stress suites passed in one session.

## Files Created/Modified

- `crates/monsgeek-protocol/devices/m5w.json`  
  Added `runtimePids: [16401]` for central runtime alias modeling.

- `crates/monsgeek-protocol/src/device.rs`  
  Added runtime PID support field + helper (`supports_runtime_pid`) and tests.

- `crates/monsgeek-protocol/src/registry.rs`  
  Added runtime-aware lookup APIs and tests.

- `crates/monsgeek-transport/src/discovery.rs`  
  Implemented runtime-alias-aware candidate resolution and non-destructive probe behavior.

- `crates/monsgeek-driver/src/service/mod.rs`  
  Added empty `devicePath` rejection and tests.

- `tools/test.sh`  
  Added deterministic one-command UAT/stress harness for gRPC firmware path validation.

- `.planning/phases/06-macros-device-specific-advanced-features/06-UAT.md`  
  Updated to final `5/5` pass with no open gaps.

## Decisions & Deviations

- **Decision:** keep registry as source of truth for compatibility (`runtimePids`) instead of introducing transport-side special cases.
- **Decision:** prioritize reliability in discovery (non-destructive probe) over aggressive recovery during listing.
- **Deviation from initial checkpoint path:** Added scripted stress harness to avoid repeated manual command drift and copy/paste errors.

## Next Phase Readiness

- Phase 06 verification is now grounded in repeatable stress evidence, not single-step manual checks.
- Driver/transport path for layer and macro command flow is stable under current M5W runtime conditions.
- Ready to proceed to next phase planning/execution with this verification baseline captured.
