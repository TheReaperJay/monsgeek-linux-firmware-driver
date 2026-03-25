# Phase 4: Bridge Integration & Key Remapping - Research

**Researched:** 2026-03-25
**Domain:** Write-safety enforcement at gRPC service boundary + end-to-end key remapping verification
**Confidence:** HIGH

## Summary

Phase 4 closes the safety gap between the raw gRPC bridge passthrough (Phase 3) and the firmware's complete absence of bounds checking on indexed write commands. The bridge already forwards all HID commands transparently; the web configurator already generates correctly-formed key remapping packets. The missing piece is write-safety validation at the `DriverService::send_command_rpc` boundary to prevent out-of-bounds memory writes that can corrupt firmware flash and boot-loop the MCU.

The implementation is narrowly scoped: (1) cache `DeviceDefinition` in `ConnectedDevice` at connection time, (2) intercept SET_KEYMATRIX commands in `send_command_rpc` and validate profile/key_index/layer against device bounds, (3) return gRPC INVALID_ARGUMENT on bounds violations, (4) add unit tests with mock transport for the validation path, (5) add feature-gated hardware integration tests for GET/SET_KEYMATRIX and GET/SET_PROFILE round trips, and (6) pass the manual browser checkpoint on real M5W hardware.

**Primary recommendation:** Add device context to `ConnectedDevice`, implement bounds validation in `send_command_rpc` by matching on the resolved command table's `set_keymatrix` byte, and verify with both mock unit tests and hardware integration tests. The existing `bounds::validate_write_request` function does the actual bounds math -- wire it in at the service layer.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Validate SET_KEYMATRIX, SET_FN, and SET_MACRO bounds at `DriverService::send_command_rpc` -- the gRPC service boundary
- This is the only point in the call chain that has both the raw command bytes AND access to `DeviceDefinition` (via `self.registry` + connected device's `device_id`)
- NOT in `CommandController` -- it holds only `UsbSession + Instant`, has no `DeviceDefinition`, and its role is timing enforcement only
- NOT in `bridge_transport::send_command_with` -- it's a generic async-to-sync adapter over `T: BridgeTransport`, pushing device semantics there breaks the trait's purpose
- `ConnectedDevice` should cache the resolved `DeviceDefinition` (or `CommandTable` + bounds) at connection time to avoid repeated `registry.find_by_id()` lookups per RPC
- The command byte for `set_keymatrix` differs between protocol families (YiChip = 0x09, RY5088 = 0x0A) -- must resolve via `DeviceDefinition.commands().set_keymatrix`, not hardcoded bytes
- SET_KEYMATRIX wire format: 11-byte `#[repr(C)]` struct, `payload[0]` = profile (0-3), `payload[1]` = key_index (0-125), `payload[5]` = layer (0-2)
- Same byte offsets for both protocol families; only the command byte differs
- Existing `bounds::validate_write_request` extracts `key_count` and `layer` from `DeviceDefinition` -- reuse it
- Dangerous write commands requiring protection: SET_KEYMATRIX (OOB key_index/layer corrupts flash), SET_FN (same key_index bounds risk), SET_MACRO (OOB macro_index/page corrupts staging buffer + flash)
- SET_LEDPARAM, SET_PROFILE, SET_RESET do NOT carry indexed writes into arbitrary memory regions -- no bounds protection needed
- Error feedback: `TransportError::BoundsViolation` propagates through transport channel; `send_command_rpc` catches it and returns gRPC INVALID_ARGUMENT with descriptive message
- Feature-gated hardware integration tests on real M5W for GET/SET_KEYMATRIX and GET/SET_PROFILE
- Manual browser checkpoint required: open app.monsgeek.com, read key mapping, remap a key, switch profiles
- Phase is not complete until browser checkpoint passes on real hardware

### Claude's Discretion
- Exact structure of cached device context in `ConnectedDevice` (full `DeviceDefinition` clone vs. extracted bounds struct)
- Whether SET_FN and SET_MACRO bounds validation is implemented in Phase 4 or deferred to their respective phases (5, 6) -- SET_KEYMATRIX bounds are mandatory for this phase
- Test scaffolding for hardware integration tests

### Deferred Ideas (OUT OF SCOPE)
- SET_FN bounds validation -- may be addressed in Phase 4 or deferred to Phase 5/6 depending on scope
- SET_MACRO bounds validation -- same; depends on whether Phase 4 implements a generic dangerous-write guard or only SET_KEYMATRIX
- Reviving CommandSchemaMap for runtime enforcement -- not needed for bounds validation
- Profile state tracking in the bridge -- not needed for passthrough
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| KEYS-01 | User can read the current key mapping for any profile via GET_KEYMATRIX | Bridge passthrough already forwards GET_KEYMATRIX; no validation needed for read commands. Hardware test verifies round-trip. |
| KEYS-02 | User can remap any key on any supported layer via SET_KEYMATRIX | Requires bounds validation at `send_command_rpc` before forwarding. Wire format research identifies exact byte offsets for profile/key_index/layer extraction. |
| KEYS-03 | User can switch between the keyboard's supported profiles via SET_PROFILE / GET_PROFILE | SET_PROFILE does not carry indexed writes -- no bounds protection needed. Hardware test verifies profile switching persists independent mappings. |
</phase_requirements>

## Standard Stack

### Core (already in project)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tonic | 0.12.x | gRPC server, Status codes | Already in use for bridge |
| tonic-web | 0.12.x | gRPC-Web translation layer | Already in use for browser bridge |
| crossbeam-channel | 0.5.x | Transport thread command channel | Already in use for transport |
| monsgeek-protocol | workspace | DeviceDefinition, CommandTable, bounds constants | Project crate |
| monsgeek-transport | workspace | bounds::validate_write_request, TransportHandle | Project crate |

### Supporting (already in project)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio | 1.x | Async runtime for gRPC service | Already in use |
| tracing | 0.1.x | Structured logging | Already in use for bridge diagnostics |
| thiserror | 2.x | TransportError derive | Already in use |

No new dependencies required. This phase wires existing infrastructure together.

## Architecture Patterns

### Current Architecture (before Phase 4)

```
Browser (app.monsgeek.com)
  |
  | gRPC-Web (SendMsg / ReadMsg)
  v
DriverService::send_command_rpc()
  |
  | (no validation -- raw passthrough)
  v
bridge_transport::send_command()
  |
  v
TransportHandle::send_fire_and_forget()
  |
  v
Transport Thread -> CommandController -> UsbSession
```

### Target Architecture (after Phase 4)

```
Browser (app.monsgeek.com)
  |
  | gRPC-Web (SendMsg / ReadMsg)
  v
DriverService::send_command_rpc()
  |
  | 1. Extract cmd byte from msg[0]
  | 2. Match against ConnectedDevice.definition.commands().set_keymatrix
  | 3. If match: extract profile/key_index/layer from payload, validate bounds
  | 4. On violation: return Status::invalid_argument(...)
  |
  v (if valid or non-dangerous command)
bridge_transport::send_command()
  |
  v
TransportHandle::send_fire_and_forget()
  |
  v
Transport Thread -> CommandController -> UsbSession
```

### Pattern 1: ConnectedDevice with Cached DeviceDefinition

**What:** Cache the full `DeviceDefinition` (or a focused bounds struct) inside `ConnectedDevice` at connection time.

**Recommendation:** Clone the full `DeviceDefinition`. It's small (~300 bytes), cloning happens once per device connection (not per RPC), and it provides access to everything needed: `commands()` for command-byte matching, `key_count`/`layer` for bounds validation, `fn_sys_layer` for future SET_FN validation.

**Current state:**
```rust
#[derive(Clone)]
struct ConnectedDevice {
    registration: DeviceRegistration,
    handle: TransportHandle,
}
```

**Target state:**
```rust
#[derive(Clone)]
struct ConnectedDevice {
    registration: DeviceRegistration,
    handle: TransportHandle,
    definition: DeviceDefinition,
}
```

**Integration points:** `connect_registration` already resolves `DeviceDefinition` via `self.registry.find_by_id()` and clones it. The `spawn_transport_event_loop` hot-plug path also resolves and clones it. Both paths already have the definition available -- just store it in the struct.

### Pattern 2: Command-Byte Matching at Service Boundary

**What:** In `send_command_rpc`, after resolving the device handle, match the command byte against the device's resolved `CommandTable` to identify dangerous writes.

**Why command-byte matching, not hardcoded bytes:** The SET_KEYMATRIX command byte is 0x09 on YiChip and 0x0A on RY5088. Hardcoding either would fail for the other family. The `DeviceDefinition.commands().set_keymatrix` resolves the correct byte per device.

**Pattern:**
```rust
async fn send_command_rpc(
    &self,
    path: &str,
    msg: Vec<u8>,
    checksum: ChecksumType,
) -> Result<(), Status> {
    self.open_device(path).await?;
    let (handle, definition) = self.get_device_for_path(path)?;

    if !msg.is_empty() {
        let cmd = msg[0];
        let commands = definition.commands();

        if cmd == commands.set_keymatrix && msg.len() > 6 {
            let profile = msg[1];
            let key_index = msg[2];
            let layer = msg[6];
            self.validate_keymatrix_bounds(&definition, profile, key_index, layer)?;
        }
    }

    bridge_transport::send_command(handle, msg, checksum)
        .await
        .map_err(Status::internal)
}
```

### Pattern 3: SET_KEYMATRIX Wire Format Extraction

**What:** Extract bounded fields from the raw 64-byte HID buffer sent by the web app.

**Wire format (verified from reference `SetKeyMatrixData`):**
```
msg[0]  = command byte (SET_KEYMATRIX: 0x09 YiChip, 0x0A RY5088)
msg[1]  = profile (0-3)
msg[2]  = key_index (0-125 for M5W, up to key_count-1 per device)
msg[3]  = pad0
msg[4]  = pad1
msg[5]  = enabled
msg[6]  = layer (0-2 for M5W base layers, up to layer-1 per device)
msg[7]  = checksum placeholder (overwritten by transport)
msg[8]  = config_type
msg[9]  = b1
msg[10] = b2
msg[11] = b3
msg[12..64] = zero padding
```

**Critical note:** The web app sends the full 64-byte buffer with the command byte at position 0. In `send_command_rpc`, `msg` is the raw bytes from the gRPC message. The bridge's `send_command_with` splits `msg[0]` as cmd and `msg[1..]` as payload. So when matching against `set_keymatrix`, we compare `msg[0]`, and the profile/key_index/layer are at `msg[1]`, `msg[2]`, `msg[6]` respectively.

**Confirmed by code:** In `bridge_transport::send_command_with`:
```rust
let cmd = data[0];
let payload = data[1..].to_vec();
```

So `msg[0]` = cmd byte, `msg[1]` = first payload byte = profile, `msg[2]` = key_index, `msg[6]` = layer. This is correct because the `SetKeyMatrixData` 11-byte struct starts with profile at offset 0 of the payload.

### Pattern 4: Profile Bounds Validation

**What:** Validate profile index (0-3) against a fixed maximum, not per-device config.

**Evidence:** The reference project uses `MAX_PROFILE = 3` as a hardcoded constant. The M5W has 4 profiles (0-3). The `DeviceDefinition` does not have a `profile_count` field. This is a firmware-wide constant, not per-device.

**Recommendation:** Validate `profile <= 3` using a constant. Unlike key_count/layer which are per-device fields in `DeviceDefinition`, profile count is firmware-fixed at 4 (indices 0-3) across all known FEA keyboards.

### Anti-Patterns to Avoid
- **Validating in CommandController:** It has zero device context by design. Pushing device semantics into the timing-only chokepoint violates its single responsibility.
- **Validating in bridge_transport:** It's a generic `T: BridgeTransport` adapter. Adding device awareness breaks the trait contract.
- **Hardcoding command bytes:** SET_KEYMATRIX is 0x09 on YiChip, 0x0A on RY5088. Always resolve via `DeviceDefinition.commands()`.
- **Blocking on CommandSchemaMap:** It's dead code at runtime. Bounds validation at the service boundary is sufficient. Don't revive it for this phase.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Key index/layer bounds checking | Custom validation logic | `bounds::validate_write_request(&definition, key_index, layer)` | Already handles missing key_count/layer defensively, tested, covers edge cases |
| Command byte resolution | Hardcoded `0x09` / `0x0A` | `definition.commands().set_keymatrix` | Per-device overrides handled automatically |
| Protocol family detection | Manual chip family string matching | `DeviceDefinition.protocol_family()` / `DeviceDefinition.commands()` | Already resolves via name prefix + PID heuristic + overrides |
| gRPC error propagation | Custom error types | `tonic::Status::invalid_argument(msg)` | Standard gRPC pattern, web app sees failed response |

## Common Pitfalls

### Pitfall 1: Off-by-One in Wire Format Offsets
**What goes wrong:** The `msg` Vec in `send_command_rpc` includes the command byte at index 0, but the `SetKeyMatrixData` struct describes payload-only offsets. Confusing the two shifts all field extractions by 1.
**Why it happens:** The bridge's `send_command_with` does `let cmd = data[0]; let payload = data[1..].to_vec();` -- so `msg[1]` in the service layer corresponds to `payload[0]` (profile) in the struct.
**How to avoid:** Document the offset mapping explicitly. `msg[0]` = cmd, `msg[1]` = profile (struct offset 0), `msg[2]` = key_index (struct offset 1), `msg[6]` = layer (struct offset 5).
**Warning signs:** Bounds validation rejects valid commands or accepts invalid ones.

### Pitfall 2: Missing Device Definition for Hot-Plugged Devices
**What goes wrong:** A device connected via hot-plug might not have its `DeviceDefinition` cached if the hot-plug path in `spawn_transport_event_loop` doesn't store it.
**Why it happens:** The hot-plug path constructs `ConnectedDevice` independently from `connect_registration`. Both paths must populate the `definition` field.
**How to avoid:** Verify both `connect_registration` (initial scan) and the hot-plug handler in `spawn_transport_event_loop` (lines ~279-329) both store the definition.
**Warning signs:** Hot-plugged devices bypass bounds validation.

### Pitfall 3: Short Command Buffer
**What goes wrong:** Attempting to extract layer from `msg[6]` on a buffer shorter than 7 bytes causes a panic.
**Why it happens:** The web app should always send 64-byte buffers, but defensive code must handle malformed input.
**How to avoid:** Check `msg.len() > 6` before extracting SET_KEYMATRIX fields. If the buffer is too short, it's malformed -- reject it.
**Warning signs:** Panics in the service layer on unusual input.

### Pitfall 4: Hardware Test Leaves Keyboard in Modified State
**What goes wrong:** A SET_KEYMATRIX hardware test remaps a key but crashes before restoring it. The keyboard is now in a modified state.
**Why it happens:** No cleanup / restore pattern in the test.
**How to avoid:** Follow the Phase 2 debounce test pattern: read original value, write test value, verify, restore original, verify restore. Use a closure pattern so restore runs even on test failure.
**Warning signs:** Keyboard behaves differently after running tests.

### Pitfall 5: Profile Validation Using DeviceDefinition.layer
**What goes wrong:** Confusing the `layer` field (number of key mapping layers, e.g., 4 for M5W) with profile count. Both happen to be 4 for M5W, masking the bug.
**Why it happens:** `DeviceDefinition` has `layer: Option<u8>` for key layers but no `profile_count` field. Profile count is firmware-fixed at 4 (MAX_PROFILE=3).
**How to avoid:** Use a constant `MAX_PROFILE: u8 = 3` for profile validation, separate from `device.layer` for layer validation.

## Code Examples

### Example 1: ConnectedDevice with DeviceDefinition

```rust
// Source: crates/monsgeek-driver/src/service/mod.rs
#[derive(Clone)]
struct ConnectedDevice {
    registration: DeviceRegistration,
    handle: TransportHandle,
    definition: DeviceDefinition,
}
```

### Example 2: Populating Definition at Connection Time

```rust
// In connect_registration, definition is already resolved:
fn connect_registration(&self, registration: DeviceRegistration, emit_add: bool) {
    let Some(definition) = self.registry.find_by_id(registration.device_id).cloned() else {
        // ... warn and return
        return;
    };
    // ... open transport ...
    let connected = ConnectedDevice {
        registration: registration.clone(),
        handle,
        definition,  // <-- already cloned above
    };
    // ...
}
```

### Example 3: Getting Device + Handle from Path

```rust
// New helper that returns both handle and definition
fn get_device_for_path(&self, path: &str) -> Result<(TransportHandle, DeviceDefinition), Status> {
    let devices = self.devices.lock().expect("devices map poisoned");
    // ... same lookup logic as get_handle_for_path but returns both ...
}
```

### Example 4: SET_KEYMATRIX Bounds Validation

```rust
// Source: verified wire format from reference SetKeyMatrixData
const MAX_PROFILE: u8 = 3;

fn validate_set_keymatrix(
    definition: &DeviceDefinition,
    msg: &[u8],
) -> Result<(), Status> {
    if msg.len() < 7 {
        return Err(Status::invalid_argument("SET_KEYMATRIX payload too short"));
    }
    let profile = msg[1];
    let key_index = msg[2] as u16;
    let layer = msg[6];

    if profile > MAX_PROFILE {
        return Err(Status::invalid_argument(
            format!("profile {} exceeds max {}", profile, MAX_PROFILE)
        ));
    }

    bounds::validate_write_request(definition, key_index, layer)
        .map_err(|e| Status::invalid_argument(e.to_string()))
}
```

### Example 5: Hardware Test Pattern (GET_KEYMATRIX)

```rust
// Feature-gated, ignored, follows Phase 2 pattern
#[cfg(feature = "hardware")]
#[test]
#[ignore]
fn test_get_keymatrix_profile_0() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    let commands = m5w.commands();
    // GET_KEYMATRIX query: [profile, magic=0, page=0, magnetism_profile=0]
    let response = handle
        .send_query(commands.get_keymatrix, &[0, 0, 0, 0], ChecksumType::Bit7)
        .expect("GET_KEYMATRIX query failed");

    assert_eq!(response[0], commands.get_keymatrix,
        "echo byte mismatch for GET_KEYMATRIX");

    handle.shutdown();
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Hardcoded command bytes | Per-device CommandTable via `DeviceDefinition.commands()` | Phase 2 | Must use resolved command table, not constants |
| Validation in transport layer | Validation at gRPC service boundary | Phase 4 decision | DriverService is the only layer with both raw bytes and device context |
| Full typed command structs | Raw passthrough with service-layer bounds checking | Phase 3/4 design | Bridge passes raw bytes; service layer only validates dangerous writes |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) + feature-gated hardware tests |
| Config file | Cargo.toml `[features]` section |
| Quick run command | `cargo test --workspace` |
| Full suite command | `cargo test --workspace && cargo test -p monsgeek-transport --features hardware -- --ignored --nocapture` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| KEYS-01 | Read key mapping via GET_KEYMATRIX | hardware-integration | `cargo test -p monsgeek-transport --features hardware -- --ignored test_get_keymatrix --nocapture` | Wave 0 |
| KEYS-02 | Remap key via SET_KEYMATRIX with bounds validation | unit + hardware-integration | `cargo test -p monsgeek-driver -- test_set_keymatrix_bounds` | Wave 0 |
| KEYS-02 | Bounds violation rejected at service layer | unit | `cargo test -p monsgeek-driver -- test_bounds_violation_returns_error` | Wave 0 |
| KEYS-03 | Switch profiles via SET_PROFILE/GET_PROFILE | hardware-integration | `cargo test -p monsgeek-transport --features hardware -- --ignored test_get_set_profile --nocapture` | Wave 0 |
| KEYS-02 | Manual browser verification | manual-only | Open app.monsgeek.com, remap Caps Lock -> Ctrl, verify | N/A |

### Sampling Rate
- **Per task commit:** `cargo test --workspace`
- **Per wave merge:** `cargo test --workspace` (hardware tests require manual opt-in)
- **Phase gate:** Full suite green + manual browser checkpoint before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] Unit tests for SET_KEYMATRIX bounds validation in `send_command_rpc` (mock transport)
- [ ] Unit test for bounds violation returning gRPC INVALID_ARGUMENT
- [ ] Hardware test for GET_KEYMATRIX profile read
- [ ] Hardware test for GET_PROFILE / SET_PROFILE round trip (dangerous, feature-gated)
- [ ] Hardware test for SET_KEYMATRIX round trip with restore (dangerous, feature-gated)

## Open Questions

1. **Should SET_FN and SET_MACRO validation be included in Phase 4?**
   - What we know: Context marks these as Claude's discretion. SET_KEYMATRIX bounds are mandatory. SET_FN uses the same key_index bounds. SET_MACRO uses different fields (macro_index, page).
   - What's unclear: Whether the web configurator's key remapping flow actually sends SET_FN during normal key remapping, or only when editing Fn-layer mappings.
   - Recommendation: Implement the generic dangerous-write guard pattern with SET_KEYMATRIX mandatory. Add SET_FN if the code structure makes it trivial (it shares key_index bounds, just different byte offsets: `msg[3]` = key_index for SET_FN). Defer SET_MACRO to Phase 6 since macros are out of scope for KEYS-01/02/03.

2. **GET_KEYMATRIX query wire format for page-based reads**
   - What we know: Reference `GetKeyMatrixData` is 4 bytes: `[profile, magic, page, magnetism_profile]`. The `magic` value is unclear but the web app sends it correctly.
   - What's unclear: What `magic` byte value the web app uses (likely 0). The web app generates these queries -- we just pass them through.
   - Recommendation: No action needed. GET_KEYMATRIX is a read command -- no bounds validation required. Hardware tests should verify the echo byte and basic response structure.

3. **Exact profile count per device**
   - What we know: Reference uses `MAX_PROFILE = 3` (4 profiles: 0-3). No per-device profile count field in `DeviceDefinition`.
   - What's unclear: Whether any device supports fewer than 4 profiles.
   - Recommendation: Use `MAX_PROFILE = 3` as a firmware-wide constant for now. If a device with fewer profiles is added later, add a `profile_count` field to `DeviceDefinition`.

## Sources

### Primary (HIGH confidence)
- `crates/monsgeek-transport/src/bounds.rs` -- existing `validate_write_request` and `validate_key_index` functions, verified via code read
- `crates/monsgeek-driver/src/service/mod.rs` -- `DriverService`, `ConnectedDevice`, `send_command_rpc`, `connect_registration` verified via code read
- `crates/monsgeek-driver/src/bridge_transport.rs` -- `send_command_with` splits `data[0]` as cmd and `data[1..]` as payload, verified via code read
- `crates/monsgeek-protocol/src/device.rs` -- `DeviceDefinition.commands()` resolves per-device CommandTable, verified via code read
- `crates/monsgeek-protocol/src/protocol.rs` -- `CommandTable` with `set_keymatrix`, `get_keymatrix`, `set_profile`, `get_profile` fields, verified via code read
- `references/monsgeek-akko-linux/.../command.rs` lines 1105-1167 -- `SetKeyMatrixData` 11-byte struct, `MAX_PROFILE=3`, `MAX_KEY_INDEX=125`, `MAX_LAYER=2`, verified via code read
- `references/monsgeek-akko-linux/.../command.rs` lines 1190-1254 -- `SetFnData` wire format with fn_sys at offset 0, profile at offset 1, key_index at offset 2, verified via code read
- `references/monsgeek-akko-linux/.../command.rs` -- `MAX_MACRO_INDEX=49`, `MAX_CHUNK_PAGE=9`, `CHUNK_PAYLOAD_SIZE=56`, verified via code read
- `crates/monsgeek-protocol/devices/m5w.json` -- M5W key_count=108, layer=4, verified via code read

### Secondary (MEDIUM confidence)
- `crates/monsgeek-transport/src/error.rs` -- `TransportError::BoundsViolation` variant exists and has Display impl, verified via code read
- Phase 2 hardware test patterns in `crates/monsgeek-transport/tests/hardware.rs` -- established pattern for feature-gated dangerous-write tests with restore

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in use, no new dependencies
- Architecture: HIGH -- validation insertion point, wire format, and bounds functions all verified via direct code reads
- Pitfalls: HIGH -- wire format offsets verified against reference struct, both connection paths identified
- Code examples: HIGH -- based on actual codebase state, not hypothetical

**Research date:** 2026-03-25
**Valid until:** 2026-04-25 (stable domain -- wire format and service architecture unlikely to change)
