# Phase 6: Macros & Device-Specific Advanced Features - Research

**Researched:** 2026-03-27
**Domain:** HID macro programming, magnetic/Hall-effect switch protocol, firmware bounds enforcement
**Confidence:** HIGH

## Summary

Phase 6 completes two distinct protocol areas: (1) macro read/write with full hardware verification on the M5W, and (2) magnetic/Hall-effect switch commands that can only be unit-tested since the M5W lacks magnetic switches. Both areas have well-documented wire formats in the reference implementation and decompiled Electron app. The codebase already contains all command byte constants, the magnetism sub-command module, and a `known_var` CommandSchemaMap entry for `set_macro`.

The primary work is: extend `validate_dangerous_write` for SET_MACRO and SET_FN bounds, audit/fix the CommandSchemaMap macro entry, add magnetic switch command entries to the schema map, gate magnetic commands per-device at the bridge boundary, write hardware tests for macro round-trip, and write unit tests for magnetic wire format correctness.

**Primary recommendation:** Follow the established Phase 4/5 pattern exactly: extend `validate_dangerous_write` with new branches, add CommandSchemaMap entries, write hardware tests that exercise schemas against real firmware, and use read-before-write/restore for all dangerous writes.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Full roundtrip macro verification on real M5W hardware: GET_MACRO read, SET_MACRO write (multi-page), readback, assign to key, confirm execution, restore
- SET_MACRO bounds: macro_index 0-49, chunk_page 0-9
- SET_FN bounds: same key_index bounds as SET_KEYMATRIX, resolved per-device via DeviceDefinition
- Magnetic/Hall-effect: protocol support only, no hardware verification (M5W has noMagneticSwitch: true)
- Full port from reference implementation for magnetic commands
- Unit tests against mock data for magnetic wire format
- MAG-01 through MAG-04 marked as "implemented, pending device validation"
- Gate magnetic switch commands per-device in validate_dangerous_write
- CommandSchemaMap audit: fix existing set_macro entry, add GET_MACRO if missing, add all magnetic switch entries
- Phase not complete until browser macro checkpoint passes on real M5W hardware
- Read-before-write, restore original for macro hardware tests

### Claude's Discretion
- Exact magnetic switch command set organization (separate module vs. inline in existing files)
- Test scaffolding structure for macro hardware tests
- Which macro slot index to use for hardware testing
- Exact calibration state machine internal design
- Error handling for unexpected firmware responses during macro operations

### Deferred Ideas (OUT OF SCOPE)
- Macro execution timing measurement
- Macro text input mode vs. raw key sequence mode
- Per-key RGB streaming (LED_STREAM 0xE8)
- Magnetic switch hardware verification
- Activating CommandSchemaMap at runtime in the bridge send path
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| MACR-01 | User can read existing macros via GET_MACRO | GET_MACRO wire format documented (2-byte query: macro_index + page). GET_MACRO stride bug documented for firmware 4.07 but M5W uses yc3121 -- verify on hardware. CommandSchemaMap already has GET_MACRO as shared entry. |
| MACR-02 | User can program macros via SET_MACRO | SET_MACRO wire format documented (7-byte header + variable payload, Bit7 checksum, multi-page). Reference implementation has full SetMacroCommand with bounds. validate_dangerous_write needs SET_MACRO branch. |
| MAG-01 | Read advanced switch calibration state | GET_CALIBRATION (0xFE) and GET_MULTI_MAGNETISM (0xE5) with CALIBRATION subcmd (0xFE). Response is 2-byte per key LE values. DeviceDefinition.has_magnetism() already implemented. |
| MAG-02 | Calibrate advanced switch behavior | SET_MAGNETISM_CAL (0x1C) and SET_MAGNETISM_MAX_CAL (0x1E) for min/max calibration. Protocol: start(1), wait, stop(0). validate_dangerous_write must gate on has_magnetism(). |
| MAG-03 | Read per-key rapid-trigger style configuration | GET_MULTI_MAGNETISM (0xE5) with RT_PRESS (0x02) and RT_LIFT (0x03) subcmds. 2-byte LE values per key. Paged response format. |
| MAG-04 | Set per-key actuation/reset points | SET_MULTI_MAGNETISM (0x65) and SET_KEY_MAGNETISM_MODE (0x1D). Wire format: 7-byte header + payload. Must be gated per-device. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| monsgeek-protocol | workspace | Command constants, magnetism subcmds, device definition, CommandSchemaMap | Already contains cmd.rs with all macro/magnetic constants and magnetism.rs with subcmd constants |
| monsgeek-transport | workspace | Bounds validation, hardware test infrastructure | Has bounds.rs, hardware.rs test pattern |
| monsgeek-driver | workspace | validate_dangerous_write, bridge service layer | Single insertion point for all write validation |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| serde/serde_json | existing | Device definition deserialization | Already in use for DeviceDefinition |
| tonic | existing | gRPC Status error types in validate_dangerous_write | Already in use |

No new external dependencies required. All work is within existing workspace crates.

## Architecture Patterns

### Extension Points (No New Modules Needed for Macros)

The existing architecture handles macro commands through the same patterns as keymatrix. The key extension points are:

```
crates/monsgeek-driver/src/service/mod.rs
  validate_dangerous_write()       # Add SET_MACRO + SET_FN branches

crates/monsgeek-protocol/src/command_schema.rs
  register_shared_commands()       # Audit macro entries, add magnetic entries

crates/monsgeek-transport/tests/hardware.rs
  # Add macro GET/SET round-trip tests
```

### Pattern 1: validate_dangerous_write Extension
**What:** Add SET_MACRO and SET_FN branches to the existing validation function.
**When to use:** Always -- these are the deferred Phase 4 bounds.
**Key details:**
- SET_MACRO: extract macro_index from msg[1], chunk_page from msg[2]. Reject macro_index > 49, chunk_page > 9.
- SET_FN: extract fn_sys from msg[1], profile from msg[2], key_index from msg[3]. Reuse `bounds::validate_write_request` for key_index/layer bounds.
- Magnetic commands (SET_MAGNETISM_CAL, SET_MAGNETISM_MAX_CAL, SET_KEY_MAGNETISM_MODE, SET_MULTI_MAGNETISM): reject with clear error if `definition.has_magnetism()` returns false.

### Pattern 2: Hardware Test Read-Write-Readback-Restore
**What:** The established Phase 4/5 dangerous-write test pattern.
**When to use:** For macro SET/GET round-trip on real M5W.
**Structure:**
1. Acquire HW_LOCK mutex
2. Connect to M5W, load device definition
3. Read original state (GET_MACRO for test slot)
4. Write test data (SET_MACRO with known payload)
5. Read back (GET_MACRO) and verify match
6. Restore original state (SET_MACRO with saved data)
7. Verify restoration
8. Handle errors in test_result/restore_result closures (restore always runs)

### Pattern 3: CommandSchemaMap Audit
**What:** Verify and fix existing entries, add missing ones.
**When to use:** Every phase that touches new command types.
**For Phase 6:**
- Audit `set_macro` entry (currently `known_var` clone -- verify it matches the 7-byte header + variable payload format)
- Verify GET_MACRO is already in shared commands (it is, at line 333 of command_schema.rs)
- Add SET_FN as a device-specific Known entry if not already present (SET_FN 0x10 is currently registered as Shared)
- Magnetic commands already registered as Shared in `register_shared_commands`: SET_MAGNETISM_REPORT (FixedSize 1), SET_MAGNETISM_CAL, SET_KEY_MAGNETISM_MODE, SET_MAGNETISM_MAX_CAL, SET_MULTI_MAGNETISM (all VariableWithMax), GET_KEY_MAGNETISM_MODE, GET_MULTI_MAGNETISM (VariableWithMax)

### Anti-Patterns to Avoid
- **Adding command-specific routing to the bridge:** The bridge is a raw passthrough. Validation happens in `validate_dangerous_write`, not in RPC handlers.
- **Hardcoding macro_index bounds:** Use constants (MAX_MACRO_INDEX=49, MAX_CHUNK_PAGE=9) that match the reference implementation, not magic numbers.
- **Testing macros without restore:** Any macro slot written during testing must be restored to its original contents.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Macro wire format | Custom serialization | Reference SetMacroHeader struct (7-byte repr(C)) | Bit-level layout must match firmware exactly |
| Key index bounds | Inline bounds checks | `bounds::validate_write_request()` | Already handles missing device fields defensively |
| Per-device capability gating | Command-level if/else | `DeviceDefinition.has_magnetism()` | Already implements the correct logic for magnetism/noMagneticSwitch fields |
| Magnetism subcmd constants | New constant definitions | `monsgeek_protocol::magnetism::*` | Already defined with all 13 subcmd values |
| Command byte resolution | Hardcoded 0x0B for set_macro | `device.commands().set_macro` | YiChip override is 0x08 on some devices; CommandTable resolves per-device |

## Common Pitfalls

### Pitfall 1: GET_MACRO Stride Bug
**What goes wrong:** GET_MACRO uses 512-byte stride while SET_MACRO uses 256-byte stride. Reading macro index N returns data from slot N*2.
**Why it happens:** Firmware bug in GET_MACRO handler (left shift by 9 instead of 8). Documented in `references/monsgeek-akko-linux/docs/bugs/get_macro_stride_bug.txt`.
**How to avoid:** This bug was documented for firmware 4.07 (device ID 2949, AT32F405). The M5W uses yc3121, which may or may not have the same bug. The hardware test must verify: write to slot 0, read slot 0, confirm match. If testing slot 1+, be aware readback may return wrong data. Use slot 0 for the primary round-trip test to avoid this issue.
**Warning signs:** GET_MACRO returns all zeros or unexpected data for non-zero slot indices.

### Pitfall 2: YiChip SET_MACRO Command Byte Override
**What goes wrong:** Using the shared constant `cmd::SET_MACRO = 0x0B` directly instead of `device.commands().set_macro`.
**Why it happens:** The M5W has `commandOverrides.setMacro: 11` (0x0B) which happens to match the shared constant. But other YiChip devices use 0x08 for SET_MACRO (the YiChip baseline).
**How to avoid:** Always use `device.commands().set_macro` for the command byte. The CommandSchemaMap already handles this via device-specific Known entries.
**Warning signs:** Commands fail on non-M5W YiChip devices.

### Pitfall 3: SET_MACRO Flash Region Overflow
**What goes wrong:** Sending macro_index >= 56 corrupts flash in other configuration regions (userpic, magnetism calibration).
**Why it happens:** Firmware computes flash address as `FLASH_MACROS_BASE + (macro_id >> 3) * 0x800` with no bounds check.
**How to avoid:** Enforce macro_index <= 49 in `validate_dangerous_write`. This matches the web app's maxMacro=50 and keeps within the 7 flash pages allocated for macros.
**Warning signs:** Keyboard settings (LED patterns, calibration) become corrupted after macro operations.

### Pitfall 4: Chunk Page Overflow
**What goes wrong:** chunk_index >= 10 writes past the 514-byte staging buffer into adjacent RAM (LED animation state).
**Why it happens:** Staging buffer holds ~9 chunks of 56 bytes. No firmware bounds check on chunk_index.
**How to avoid:** Enforce chunk_page <= 9 in `validate_dangerous_write`. Applies to SET_MACRO, SET_KEYMATRIX, and SET_FN.
**Warning signs:** LED animations glitch or keyboard becomes unresponsive after large macro writes.

### Pitfall 5: Magnetic Commands on Non-Magnetic Devices
**What goes wrong:** Sending magnetism commands to M5W (which has noMagneticSwitch: true) produces undefined firmware behavior.
**Why it happens:** No firmware-side capability gating -- the firmware processes any valid command byte regardless of hardware support.
**How to avoid:** Gate at bridge service boundary: `validate_dangerous_write` must reject SET_MAGNETISM_*, SET_KEY_MAGNETISM_MODE, SET_MULTI_MAGNETISM when `!definition.has_magnetism()`.
**Warning signs:** Firmware returns garbage data or no response for magnetism commands.

### Pitfall 6: Multi-Page Macro Transfer Timing
**What goes wrong:** Sending macro pages too fast causes firmware to miss chunks.
**Why it happens:** The 100ms inter-command delay is already enforced by the transport thread, but multi-page macro writes involve many sequential commands.
**How to avoid:** The existing transport throttling (100ms per command) handles this. A 10-page macro requires ~1 second minimum. Don't try to bypass throttling for macro writes.
**Warning signs:** Macro playback is corrupted or truncated.

## Code Examples

### validate_dangerous_write SET_MACRO Branch
```rust
// Pattern for SET_MACRO bounds validation in validate_dangerous_write
if cmd == commands.set_macro {
    if msg.len() < 3 {
        return Err(Status::invalid_argument(
            "SET_MACRO payload too short: need at least 3 bytes",
        ));
    }
    let macro_index = msg[1];
    let chunk_page = msg[2];

    if macro_index > 49 {
        return Err(Status::invalid_argument(format!(
            "SET_MACRO macro_index {} exceeds max 49", macro_index
        )));
    }
    if chunk_page > 9 {
        return Err(Status::invalid_argument(format!(
            "SET_MACRO chunk_page {} exceeds max 9", chunk_page
        )));
    }
}
```

### validate_dangerous_write SET_FN Branch
```rust
// SET_FN wire layout: [cmd, fn_sys, profile, key_index, pad, pad, pad, checksum, ...]
if cmd == cmd::SET_FN {
    if msg.len() < 4 {
        return Err(Status::invalid_argument(
            "SET_FN payload too short: need at least 4 bytes",
        ));
    }
    let profile = msg[2];
    let key_index = msg[3] as u16;

    if profile > MAX_PROFILE {
        return Err(Status::invalid_argument(format!(
            "SET_FN profile {} exceeds max {}", profile, MAX_PROFILE
        )));
    }
    monsgeek_transport::bounds::validate_write_request(definition, key_index, 0)
        .map_err(|e| Status::invalid_argument(e.to_string()))?;
}
```

### Magnetic Command Gating
```rust
// Gate magnetic commands at bridge boundary
let magnetic_cmds = [
    cmd::SET_MAGNETISM_REPORT,
    cmd::SET_MAGNETISM_CAL,
    cmd::SET_KEY_MAGNETISM_MODE,
    cmd::SET_MAGNETISM_MAX_CAL,
    cmd::SET_MULTI_MAGNETISM,
];
if magnetic_cmds.contains(&cmd) && !definition.has_magnetism() {
    return Err(Status::failed_precondition(format!(
        "{} rejected: device {} does not support magnetic switches",
        cmd::name(cmd), definition.display_name
    )));
}
```

### Macro Hardware Test Structure
```rust
#[cfg(feature = "dangerous-hardware-writes")]
#[test]
#[ignore]
fn test_set_get_macro_round_trip_dangerous() {
    require_dangerous_write_opt_in();
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    let commands = m5w.commands();
    let test_slot: u8 = 0; // Use slot 0 to avoid GET_MACRO stride bug

    // 1. Read original macro in slot 0
    let original = handle
        .send_query(cmd::GET_MACRO, &[test_slot, 0], ChecksumType::Bit7)
        .expect("GET_MACRO failed");

    // 2. Write test macro, 3. Read back, 4. Verify, 5. Restore
    // ... (following established closure pattern from debounce test)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| No SET_MACRO bounds checking | validate_dangerous_write with macro_index + chunk_page limits | Phase 6 | Prevents flash corruption via OOB macro writes |
| No SET_FN bounds checking | validate_dangerous_write reusing bounds::validate_write_request | Phase 6 | Closes last deferred bounds gap from Phase 4 |
| Magnetic commands always pass through | Per-device capability gating at bridge boundary | Phase 6 | Prevents undefined firmware behavior on non-magnetic devices |

**Key firmware differences to track:**
- PROTOCOL.md documents macro_id 0-15 as flash path limit for AT32F405 firmware 4.07
- Reference implementation uses MAX_MACRO_INDEX=49 for web app compatibility (50 macro slots)
- The M5W uses yc3121 firmware where the flash layout supports 50 macros in 7 pages (14KB)
- Use 0-49 range as specified in CONTEXT.md decisions

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) |
| Config file | Cargo.toml workspace + per-crate feature flags |
| Quick run command | `cargo test -p monsgeek-protocol -p monsgeek-driver` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| MACR-01 | Read existing macros via GET_MACRO | hardware | `cargo test -p monsgeek-transport --features hardware -- --ignored test_get_macro --nocapture` | Wave 0 |
| MACR-02 | Program macros via SET_MACRO | hardware (dangerous) | `MONSGEEK_ENABLE_DANGEROUS_WRITES=1 cargo test -p monsgeek-transport --features "hardware dangerous-hardware-writes" -- --ignored test_set_get_macro --nocapture` | Wave 0 |
| MACR-02 | SET_MACRO bounds validation | unit | `cargo test -p monsgeek-driver validate_dangerous_write_set_macro` | Wave 0 |
| MAG-01 | Read calibration state (mock) | unit | `cargo test -p monsgeek-transport test_magnetism_calibration_parse` | Wave 0 |
| MAG-02 | Calibration commands (mock) | unit | `cargo test -p monsgeek-transport test_magnetism_calibration_commands` | Wave 0 |
| MAG-03 | Read per-key RT config (mock) | unit | `cargo test -p monsgeek-transport test_magnetism_rt_read` | Wave 0 |
| MAG-04 | Set per-key actuation (mock) | unit | `cargo test -p monsgeek-transport test_magnetism_set_actuation` | Wave 0 |
| N/A | SET_FN bounds validation | unit | `cargo test -p monsgeek-driver validate_dangerous_write_set_fn` | Wave 0 |
| N/A | Magnetic command gating (non-magnetic device) | unit | `cargo test -p monsgeek-driver validate_dangerous_write_magnetic` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p monsgeek-protocol -p monsgeek-driver`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green + browser macro checkpoint on real M5W hardware

### Wave 0 Gaps
- [ ] Macro hardware test functions in `crates/monsgeek-transport/tests/hardware.rs` -- MACR-01, MACR-02
- [ ] SET_MACRO / SET_FN bounds validation unit tests in `crates/monsgeek-driver/src/service/mod.rs` -- test module
- [ ] Magnetic command gating unit tests in `crates/monsgeek-driver/src/service/mod.rs` -- test module
- [ ] Magnetic wire format unit tests (location: TBD, likely in transport crate or protocol crate)

## Open Questions

1. **GET_MACRO stride bug on M5W/yc3121**
   - What we know: Documented for AT32F405 firmware 4.07 (device ID 2949). GET_MACRO uses 512-byte stride, SET_MACRO uses 256-byte stride.
   - What's unclear: Whether the M5W (yc3121, device ID 1308) has the same bug. The firmware codebases may differ.
   - Recommendation: Use macro slot 0 for the primary round-trip test (unaffected by stride bug). If slot 0 round-trip works, try slot 1 to detect the stride bug. Document the finding either way.

2. **SET_FN wire layout byte offsets**
   - What we know: Reference SetFnData has fn_sys at byte 0, profile at byte 1, key_index at byte 2. But in `validate_dangerous_write`, the msg[] array is the full HID payload including the command byte at msg[0].
   - What's unclear: Need to verify the exact byte positions in the raw bridge message format (msg[0]=cmd, msg[1]=fn_sys, msg[2]=profile, msg[3]=key_index).
   - Recommendation: Cross-reference with the existing SET_KEYMATRIX branch which already maps msg[1]=profile, msg[2]=key_index, msg[6]=layer. Verify SET_FN offsets against the reference SetFnData struct.

3. **Magnetic command CommandSchemaMap coverage**
   - What we know: All magnetic SET/GET commands are already registered as Shared entries in `register_shared_commands`. SET_MAGNETISM_REPORT has FixedSize(1), all others have VariableWithMax.
   - What's unclear: Whether the existing Shared registrations are sufficient or whether some should be promoted to Known with tighter schemas once verified against the Electron app.
   - Recommendation: Keep existing Shared registrations for now. The bridge always sends 63-byte payloads, so VariableWithMax is correct. Tighten later if needed.

## Sources

### Primary (HIGH confidence)
- `crates/monsgeek-protocol/src/cmd.rs` -- all command byte constants verified present
- `crates/monsgeek-protocol/src/magnetism.rs` -- all 13 magnetism subcmd constants verified
- `crates/monsgeek-protocol/src/command_schema.rs` -- existing schema map with macro + magnetic entries verified
- `crates/monsgeek-protocol/src/device.rs` -- DeviceDefinition.has_magnetism() verified
- `crates/monsgeek-driver/src/service/mod.rs` -- validate_dangerous_write structure verified
- `crates/monsgeek-transport/src/bounds.rs` -- validate_write_request verified
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/command.rs` -- SetMacroHeader, SetMacroCommand, GetMacroData, SetMultiMagnetismHeader, SetKeyMagnetismModeData wire formats verified
- `references/monsgeek-akko-linux/docs/PROTOCOL.md` -- chunked write protocol, magnetism subcmds, calibration procedure
- `references/monsgeek-akko-linux/docs/bugs/oob_hazards.txt` -- SET_MACRO flash overflow documentation
- `references/monsgeek-akko-linux/docs/bugs/get_macro_stride_bug.txt` -- GET_MACRO stride mismatch

### Secondary (MEDIUM confidence)
- `references/monsgeek-akko-linux/iot_driver_linux/src/commands/macros.rs` -- CLI macro operations pattern (get, set, clear, assign)

### Tertiary (LOW confidence)
- None. All findings verified against source code and reference documentation.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new dependencies, all extension points verified in existing code
- Architecture: HIGH -- follows established Phase 4/5 patterns exactly
- Pitfalls: HIGH -- firmware bugs documented with root cause analysis from decompilation
- Magnetic commands: MEDIUM -- wire format from reference implementation, cannot hardware-verify on M5W

**Research date:** 2026-03-27
**Valid until:** 2026-04-27 (stable -- firmware protocol unlikely to change)
