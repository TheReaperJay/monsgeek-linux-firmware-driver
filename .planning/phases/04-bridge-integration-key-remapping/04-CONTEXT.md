# Phase 4: Bridge Integration & Key Remapping - Context

**Gathered:** 2026-03-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can read and modify key mappings and switch between profiles using the MonsGeek web configurator on Linux, with verified safety guards preventing firmware memory corruption from out-of-bounds write commands. The bridge passthrough from Phase 3 already forwards raw HID commands; this phase closes the safety gap for dangerous indexed writes and verifies the full key remapping flow end-to-end on real M5W hardware.

</domain>

<decisions>
## Implementation Decisions

### Write safety enforcement

- Validate SET_KEYMATRIX, SET_FN, and SET_MACRO bounds at `DriverService::send_command_rpc` — the gRPC service boundary
- This is the only point in the call chain that has both the raw command bytes AND access to `DeviceDefinition` (via `self.registry` + connected device's `device_id`)
- NOT in `CommandController` — it holds only `UsbSession + Instant`, has no `DeviceDefinition`, and its role is timing enforcement only
- NOT in `bridge_transport::send_command_with` — it's a generic async-to-sync adapter over `T: BridgeTransport`, pushing device semantics there breaks the trait's purpose
- `ConnectedDevice` should cache the resolved `DeviceDefinition` (or `CommandTable` + bounds) at connection time to avoid repeated `registry.find_by_id()` lookups per RPC
- The command byte for `set_keymatrix` differs between protocol families (YiChip = 0x09, RY5088 = 0x0A) — must resolve via `DeviceDefinition.commands().set_keymatrix`, not hardcoded bytes

### SET_KEYMATRIX wire format (verified from reference)

- 11-byte `#[repr(C)]` struct: `payload[0]` = profile (0-3), `payload[1]` = key_index (0-125), `payload[5]` = layer (0-2)
- Same byte offsets for both protocol families; only the command byte differs
- Existing `bounds::validate_write_request` extracts `key_count` and `layer` from `DeviceDefinition` — reuse it

### Dangerous write commands requiring protection

- **SET_KEYMATRIX** — OOB key_index/layer corrupts flash, can boot-loop MCU
- **SET_FN** — same key_index bounds risk, different wire layout
- **SET_MACRO** — OOB macro_index/page corrupts staging buffer + flash (max macro_index = 49, max chunk_page = 9)
- SET_LEDPARAM, SET_PROFILE, SET_RESET do NOT carry indexed writes into arbitrary memory regions — no bounds protection needed

### Error feedback to web app

- `TransportError::BoundsViolation` propagates through the transport channel
- `send_command_rpc` catches it and returns gRPC error status (INVALID_ARGUMENT) with a descriptive message
- Web app sees a failed response instead of silent success

### Verification methodology

- Feature-gated hardware integration tests on real M5W for GET/SET_KEYMATRIX and GET/SET_PROFILE (same pattern as Phase 2 hardware tests)
- Manual browser checkpoint: open app.monsgeek.com, read a key mapping, remap a key (e.g., Caps Lock to Ctrl), switch profiles, confirm each profile retains independent mappings
- Phase is not complete until browser checkpoint passes on real hardware

### Phase scope

- Safety gap closure + end-to-end verification — no new bridge-level RPC logic expected
- Bridge passthrough from Phase 3 should already handle key remapping commands
- Fix whatever breaks during hardware verification

### Claude's Discretion

- Exact structure of cached device context in `ConnectedDevice` (full `DeviceDefinition` clone vs. extracted bounds struct)
- Whether SET_FN and SET_MACRO bounds validation is implemented in Phase 4 or deferred to their respective phases (5, 6) — SET_KEYMATRIX bounds are mandatory for this phase
- Test scaffolding for hardware integration tests

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Write safety infrastructure
- `crates/monsgeek-transport/src/bounds.rs` — existing `validate_write_request` and `validate_key_index` functions, reusable for SET_KEYMATRIX bounds enforcement
- `crates/monsgeek-transport/src/controller.rs` — CommandController architecture (timing-only chokepoint, no device context — validation does NOT go here)
- `crates/monsgeek-protocol/src/command_schema.rs` — CommandSchemaMap (currently dead code at runtime, defines per-device command vocabulary)

### Bridge service layer (validation insertion point)
- `crates/monsgeek-driver/src/service/mod.rs` — `DriverService::send_command_rpc` is the validated insertion point; `ConnectedDevice` struct needs cached DeviceDefinition
- `crates/monsgeek-driver/src/service/device_registry.rs` — `DeviceRegistration` stores `device_id`, bridge resolves to `DeviceDefinition` via registry
- `crates/monsgeek-driver/src/bridge_transport.rs` — generic async-to-sync adapter, should remain device-agnostic

### Protocol and device context
- `crates/monsgeek-protocol/src/cmd.rs` — shared command constants (SET_KEYMATRIX = 0x0A, GET_KEYMATRIX = 0x8A for RY5088; YiChip uses CommandTable overrides)
- `crates/monsgeek-protocol/src/protocol.rs` — `CommandTable` with `set_keymatrix`, `get_keymatrix`, `set_profile`, `get_profile` fields
- `crates/monsgeek-protocol/src/device.rs` — `DeviceDefinition.commands()` resolves per-device command table with overrides

### Reference wire format
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/command.rs` §1105-1167 — `SetKeyMatrixData` 11-byte struct, bounds constants (MAX_PROFILE=3, MAX_KEY_INDEX=125, MAX_LAYER=2, MAX_MACRO_INDEX=49)

### Transport layer (do not modify for this phase)
- `crates/monsgeek-transport/src/thread.rs` — transport thread has zero device context, receives `(cmd, data, checksum, mode)` tuples only
- `crates/monsgeek-transport/src/lib.rs` — `spawn_transport` receives DeviceDefinition but only passes `device.vid` to hotplug thread

### Prior phase context
- `.planning/phases/03-grpc-web-bridge/03-CONTEXT.md` — Phase 3 bridge decisions this phase builds on
- `.planning/REQUIREMENTS.md` — KEYS-01, KEYS-02, KEYS-03 requirements for this phase

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `bounds::validate_write_request(device, key_index, layer)` — existing bounds validation, tested, exported, zero callsites in execution path
- `DeviceDefinition.commands()` — resolves per-device CommandTable with overrides, already handles YiChip/RY5088 divergence
- `DeviceRegistry::find_by_id(id)` — resolves device_id to full DeviceDefinition
- Phase 2 hardware test pattern — feature-gated `#[cfg(feature = "hardware-test")]` with `#[ignore]` attribute

### Established Patterns
- CommandController is timing-only — no semantic validation (intentional, per commit 81150d3)
- bridge_transport is a generic `T: BridgeTransport` adapter — device-agnostic by design
- DriverService holds `Arc<DeviceRegistry>` and `Arc<Mutex<HashMap<String, ConnectedDevice>>>` — device context available at service layer
- gRPC errors propagate as `tonic::Status` — existing pattern in service handlers

### Integration Points
- `send_command_rpc` in `service/mod.rs` — the single point where validation must be inserted
- `ConnectedDevice` struct — needs DeviceDefinition cached at connection time
- `open_device` / `connect_registration` — where DeviceDefinition is resolved and should be stored

</code_context>

<specifics>
## Specific Ideas

- The reference project's `command.rs` defines exact bounds constants (MAX_KEY_INDEX=125, MAX_LAYER=2, MAX_PROFILE=3, MAX_MACRO_INDEX=49) — these should inform our validation but the actual limits come from DeviceDefinition fields (`key_count`, `layer`) which vary per device
- CommandSchemaMap is currently dead code — this phase does not need to revive it; bounds validation at the service boundary is sufficient
- The comment in controller.rs ("Payload validation belongs in the typed keyboard API layer, not here") is architecturally correct and should not be changed

</specifics>

<deferred>
## Deferred Ideas

- SET_FN bounds validation — may be addressed in Phase 4 or deferred to Phase 5/6 depending on scope
- SET_MACRO bounds validation — same; depends on whether Phase 4 implements a generic dangerous-write guard or only SET_KEYMATRIX
- Reviving CommandSchemaMap for runtime enforcement — not needed for bounds validation, potentially useful for a future typed keyboard API layer
- Profile state tracking in the bridge — not needed for passthrough, could be useful for future CLI

</deferred>

---

*Phase: 04-bridge-integration-key-remapping*
*Context gathered: 2026-03-25*
