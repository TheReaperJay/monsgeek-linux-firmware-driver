---
phase: 03-grpc-web-bridge
plan: 01
subsystem: api
tags: [grpc-web, tonic, prost, cors, localhost-bridge]

requires:
  - phase: 02-fea-protocol-hid-transport
    provides: control-only transport ownership, firmware-id-first discovery, hotplug event model
provides:
  - full DriverGrpc wire contract generation in local crate
  - browser-capable gRPC-Web server bootstrap on localhost:3814
  - full service skeleton with compatibility-safe stubs for non-MVP RPCs
affects: [03-02, 03-03, grpc-bridge, configurator-compatibility]

tech-stack:
  added: [tonic, tonic-web, prost, tower-http, tokio, tokio-stream, tonic-build, futures, tracing]
  patterns: [generated-proto-contract, full-service-registration, compatibility-safe-stubs]

key-files:
  created:
    - crates/monsgeek-driver/build.rs
    - crates/monsgeek-driver/proto/driver.proto
    - crates/monsgeek-driver/src/pb.rs
    - crates/monsgeek-driver/src/lib.rs
    - crates/monsgeek-driver/src/service/mod.rs
  modified:
    - crates/monsgeek-driver/Cargo.toml
    - crates/monsgeek-driver/src/main.rs

key-decisions:
  - "Kept vendor proto contract verbatim, including quirks like watchVender naming, to avoid wire drift."
  - "Registered full DriverGrpc surface up front and used compatibility-safe stubs rather than partial service exposure."
  - "Implemented HTTP/1 + tonic-web + permissive CORS at bootstrap to satisfy browser gRPC-Web requirements."

patterns-established:
  - "Bridge contract is generated from proto, not hand-modeled types."
  - "Non-MVP RPCs remain present with explicit not-supported behavior."

requirements-completed: [GRPC-01, GRPC-07, GRPC-08]

duration: 30 min
completed: 2026-03-23
---

# Phase 03 Plan 01: gRPC-Web Bridge Summary

**Local bridge runtime now exposes the full generated DriverGrpc contract with browser-compatible gRPC-Web server wiring on `127.0.0.1:3814`.**

## Performance

- **Duration:** 30 min
- **Started:** 2026-03-23T22:14:00+07:00
- **Completed:** 2026-03-23T22:44:00+07:00
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments
- Added local proto generation pipeline and imported the vendor-compatible `DriverGrpc` schema.
- Implemented full service skeleton with all RPC handlers present and compatibility-safe stubs.
- Replaced placeholder binary with Tokio gRPC-Web bootstrap (`tonic_web`, HTTP/1, CORS, localhost bind).

## Task Commits

Each task was committed atomically:

1. **Task 1: Add proto generation and runtime dependencies for full DriverGrpc service** - `8aa343f` (feat)
2. **Task 2: Implement full DriverGrpc skeleton with compatibility-safe stubs** - `03b5090` (feat)
3. **Task 3: Wire server bootstrap for browser gRPC-Web on localhost:3814** - `c7fa1c9` (feat)

## Files Created/Modified
- `crates/monsgeek-driver/build.rs` - Tonic/prost build hook for proto generation.
- `crates/monsgeek-driver/proto/driver.proto` - Local canonical bridge contract.
- `crates/monsgeek-driver/src/pb.rs` - Generated proto module inclusion.
- `crates/monsgeek-driver/src/service/mod.rs` - Full DriverGrpc skeleton and stub behavior.
- `crates/monsgeek-driver/src/main.rs` - Browser-capable gRPC-Web server bootstrap.

## Decisions Made
- Preserved wire-level naming quirks from vendor proto to avoid compatibility regressions.
- Chose full service registration now rather than incremental endpoint exposure.
- Added `--help` short-circuit in binary startup so verification commands can run non-blocking.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- Initial service skeleton used a few incorrect generated-field names (`MicrophoneMuteStatus`, `PlayEffectResponse`, `WeatherRes`). Fixed by reading generated prost output and updating initializers.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plan `03-02` can now implement real bridge semantics (device stream + split send/read + DB/version behavior).
- No blockers remain from `03-01`.

---
*Phase: 03-grpc-web-bridge*
*Completed: 2026-03-23*
