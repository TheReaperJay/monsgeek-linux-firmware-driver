# Phase 3: gRPC-Web Bridge - Context

**Gathered:** 2026-03-23  
**Status:** Ready for planning

<domain>
## Phase Boundary

Deliver a local gRPC-Web bridge on `127.0.0.1:3814` that makes the MonsGeek web configurator work on Linux against the Phase 2 transport layer. The bridge must expose the vendor-compatible `DriverGrpc` service shape, translate web-app requests into the local transport stack, and preserve normal keyboard typing by building on control-only transport by default.

This phase is about compatibility with the existing configurator, not a custom UI, not firmware flashing, and not inventing a new bridge contract.
</domain>

<decisions>
## Implementation Decisions

### RPC compatibility surface
- **D-01:** Register the full `DriverGrpc` service surface from the vendor-compatible proto, not a partial service.
- **D-02:** Implement the Phase 3 MVP RPCs for real and keep out-of-scope RPCs present but returning deliberate compatibility-safe "not supported yet" responses rather than omitting endpoints.

### Device identity exposed to the web app
- **D-03:** Expose a bridge-owned synthetic compatibility path, not raw bus/address or raw hidraw identity.
- **D-04:** Keep firmware device ID as canonical model identity and treat the bridge path as a session/runtime locator only.
- **D-05:** Allow reconnects to receive a new synthetic path when runtime topology changes; do not promise stable path reuse across reconnects.
- **D-06:** Include a uniqueness suffix in the compatibility path when needed so identical devices can coexist without collisions.

### Device-list stream semantics
- **D-07:** `watchDevList` sends one `Init` snapshot immediately on subscription.
- **D-08:** After `Init`, Phase 3 emits only `Add` and `Remove` events for device-list changes.
- **D-09:** Do not implement `Change` semantics in Phase 3 unless planning/research proves the web app actually requires them.

### Local storage compatibility
- **D-10:** Phase 3 DB RPCs use an in-memory compatibility store for MVP behavior.
- **D-11:** Persistence across bridge restarts is explicitly out of scope for this phase unless later research proves the web app requires it.

### Raw vs higher-level message handling
- **D-12:** Implement both `sendRawFeature` / `readRawFeature` and `sendMsg` / `readMsg` as first-class RPCs.
- **D-13:** Route both RPC families through one shared transport core so behavior, throttling, and device ownership remain consistent.
- **D-14:** It is acceptable to extend lower layers in this repo if the current transport API is too narrow for bridge compatibility; Phase 3 must not distort the bridge contract just to avoid transport-layer edits.

### Ownership and runtime behavior
- **D-15:** Phase 3 uses control-only transport by default so normal kernel typing is preserved while the bridge is running.
- **D-16:** Userspace-input mode is not the default bridge behavior and should not be pulled into Phase 3 unless required for compatibility.

### the agent's Discretion
- Exact internal server/module layout inside the bridge crate
- Exact error text for unsupported RPCs, as long as the RPCs remain present and compatibility-safe
- Exact in-memory DB container type and concurrency primitives
</decisions>

<specifics>
## Specific Ideas

- Follow the working Linux bridge shape in `references/monsgeek-akko-linux/` instead of inventing a novel contract.
- Treat the reference bridge and proto as the compatibility target, then adapt our internal layers to fit that target where needed.
- The current lower-layer code is not an immutable constraint. If `monsgeek-transport` needs API expansion for split send/read bridge behavior, change it instead of warping the bridge design.
- Favor evidence-backed behavior over speculative improvements:
  - synthetic compatibility paths
  - `Init` + `Add`/`Remove` device stream semantics
  - in-memory DB compatibility store for MVP
  - both raw and checksum-aware RPC families exposed
</specifics>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project scope and current state
- `.planning/PROJECT.md` — project vision and non-negotiable direction
- `.planning/REQUIREMENTS.md` — milestone acceptance criteria and Phase 3 MVP expectations
- `.planning/ROADMAP.md` — phase boundary and ordering
- `.planning/STATE.md` — current milestone status after Phase 2 closeout
- `.planning/phases/02-fea-protocol-hid-transport/02-CONTEXT.md` — locked transport assumptions Phase 3 must build on
- `.planning/phases/02-fea-protocol-hid-transport/02-03-SUMMARY.md` — what Phase 2 actually delivered

### Compatibility contract reference
- `references/monsgeek-akko-linux/iot_driver_linux/proto/driver.proto` — full `DriverGrpc` contract and message shapes
- `references/monsgeek-akko-linux/iot_driver_linux/src/main.rs` §390-432 — localhost bind, gRPC-Web enablement, browser-facing server setup
- `references/monsgeek-akko-linux/iot_driver_linux/src/grpc.rs` §432-577 — hot-plug add/remove behavior and synthetic path construction
- `references/monsgeek-akko-linux/iot_driver_linux/src/grpc.rs` §736-898 — device open behavior and split send/read transport usage
- `references/monsgeek-akko-linux/iot_driver_linux/src/grpc.rs` §909-1127 — `watchDevList`, raw/checksum-aware RPCs, DB RPCs, version RPC
- `references/monsgeek-akko-linux/FEATURE_MAP.md` §176-186 — claimed web-app compatibility and implemented gRPC features

### Current local stack
- `crates/monsgeek-transport/src/lib.rs` — current public transport API, control-only default, and likely bridge integration pressure points
- `crates/monsgeek-transport/src/thread.rs` — transport events and serialized command execution model
- `crates/monsgeek-transport/src/usb.rs` — session modes and low-level ownership behavior
- `crates/monsgeek-transport/src/discovery.rs` — firmware-ID-aware enumeration behavior
- `crates/monsgeek-protocol/src/cmd.rs` — command definitions Phase 3 RPCs will forward

### Vendor app evidence
- `firmware/MonsGeek_v4_setup_500.2.13_WIN2026032/extracted/app/resources/app/dist/index.eb7071d5.js` — bundled app behavior, including device selection keyed by `id + "_" + devAddr`
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `monsgeek-transport::connect_with_options()` — already gives the bridge a control-only default session model
- `TransportHandle` — already centralizes serialized command execution and yc3121-safe throttling
- `TransportEvent::{DeviceArrived, DeviceLeft}` — already provides a bridge-friendly source for hot-plug lifecycle updates
- Firmware-ID-aware probing from Phase 2 — already separates canonical device identity from transport PID/runtime location

### Established Patterns
- Canonical identity is firmware device ID, not PID or bus/address
- Transport ownership defaults to preserving normal typing
- `udev`-driven add/remove events are the real Linux hot-plug source
- Recovery and low-level transport safety live below the future bridge, not in the bridge itself

### Integration Points
- Phase 3 bridge code will live above `monsgeek-transport`, likely in `crates/monsgeek-driver`
- `watchDevList` can be built from initial discovery plus `TransportEvent` add/remove signals
- The current `TransportHandle` API is likely too narrow for bridge-level split send/read semantics, so planning should consider extending the transport API rather than bypassing the transport thread
</code_context>

<deferred>
## Deferred Ideas

- File-backed DB persistence across bridge restarts
- `DeviceListChangeType::Change` support for richer online/battery/status updates
- OTA, weather, microphone, LED streaming, and other non-MVP RPC families beyond compatibility-safe stubs
- Stable synthetic device paths across reconnects for identical devices
</deferred>

---

*Phase: 03-grpc-web-bridge*  
*Context gathered: 2026-03-23*
