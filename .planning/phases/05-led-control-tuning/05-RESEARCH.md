# Phase 5: LED Control & Tuning - Research

**Researched:** 2026-03-26
**Domain:** HID protocol commands for LED, debounce, and polling rate on YiChip (M5W)
**Confidence:** HIGH

## Summary

Phase 5 is a verification and schema-audit phase, not a feature-build phase. The bridge is already a fully generic raw passthrough -- LED, debounce, and polling commands flow through it identically to keymatrix commands. The work is: (1) audit and correct CommandSchemaMap entries for LED/debounce/polling commands, (2) write hardware integration tests that exercise these commands against real M5W firmware using the established read-write-readback-restore pattern, (3) probe GET_REPORT on the M5W to determine polling rate support, and (4) pass a manual browser verification checkpoint.

No bridge-level code changes are expected. The protocol crate's `command_schema.rs` needs audit corrections. The transport crate gets new hardware tests. The entire phase operates within existing patterns established in Phases 2 and 4.

**Primary recommendation:** Structure the phase as schema audit first, hardware tests second, browser checkpoint last. Schema audit informs test construction; tests validate the audit.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Verify GET_LEDPARAM and SET_LEDPARAM roundtrip only: read current mode, change effect, adjust brightness/speed/color, confirm keyboard responds
- Per-key RGB streaming (LED_STREAM 0xE8, 7-page protocol) and UserPicture mode are out of scope for this phase
- Side LED commands (SET_SLEDPARAM/GET_SLEDPARAM) are irrelevant -- M5W has `hasSideLight: false`
- Phase 5 verifies firmware debounce read/write works -- not a tuning/measurement phase
- Confirm GET_DEBOUNCE reads a value, SET_DEBOUNCE changes it, keyboard acknowledges the new value
- Whether firmware-side debounce tuning actually fixes ghosting is secondary -- Phase 5.1 userspace daemon is the real fix
- No systematic debounce value sweep or double-letter frequency measurement in this phase
- Read-write-readback-restore: read current value, write test value, readback to verify change took effect, restore original value
- M5W has NO `reportRate` field in any device definition -- the configurator would not show polling rate controls for this device
- Phase 5 probes GET_REPORT (0x83) on real M5W hardware to determine if the firmware responds
- If firmware responds: document supported rates and the working command path
- If firmware ignores or rejects: document that M5W doesn't support user-configurable polling rate; the success criterion is satisfied by proving the command path works
- Do not block phase completion on polling rate results
- Browser verification checkpoint: Open app.monsgeek.com, change LED effect mode, adjust brightness/speed/color, adjust debounce value, confirm keyboard responds to each
- Polling rate is excluded from browser checkpoint since the configurator doesn't show these controls for M5W
- Audit and complete all LED/debounce/polling entries in CommandSchemaMap for correctness against the reference implementation
- CommandSchemaMap is currently dead code at runtime but validates protocol definitions
- Hardware integration tests construct commands via CommandSchemaMap, validating schemas by exercising them against real firmware

### Claude's Discretion
- Exact LED parameter parsing in test assertions (which byte offsets to check in response)
- Whether to test multiple LED modes or just one mode change
- Error handling for unexpected firmware responses
- Test scaffolding structure

### Deferred Ideas (OUT OF SCOPE)
- Per-key RGB streaming (LED_STREAM 0xE8) -- complex 7-page protocol, separate verification scope
- UserPicture LED mode -- uses different payload layout (layer<<4 in option byte, fixed RGB)
- Side LED support -- M5W doesn't have side LEDs; verify on a device that does
- Activating CommandSchemaMap at runtime in the bridge send path -- currently dead code, could prevent malformed commands
- Polling rate UI support -- if M5W doesn't support it, consider whether to add a `reportRate` field to device JSON for devices that do
- Systematic debounce tuning / ghosting measurement -- Phase 5.1 territory
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| LED-01 | User can read current LED mode, brightness, speed, and color via GET_LEDPARAM | GET_LEDPARAM (0x87) uses Bit8 checksum; response format: `[echo, mode, speed_inv, brightness, option, r, g, b]`; speed is inverted on wire (0=fast, 4=slow). Schema audit ensures correct registration. Hardware test verifies real firmware response. |
| LED-02 | User can set LED mode, brightness, speed, and color via SET_LEDPARAM | SET_LEDPARAM (0x07) uses Bit8 checksum; payload: `[mode, speed_inv, brightness, option, r, g, b]`; 7 bytes. Hardware test uses write-readback pattern. Bridge passthrough already handles this. |
| TUNE-01 | User can read and set debounce value via GET_DEBOUNCE / SET_DEBOUNCE | M5W (YiChip) overrides: SET=0x11, GET=0x91 with Bit7 checksum. SET uses PrependProfileZero normalization (`[value]` -> `[0x00, value]`). GET response: YiChip format has debounce value at `response[2]` (not `response[1]`). Existing hardware test covers this but behind dangerous-writes flag. |
| TUNE-02 | User can read and set polling rate via GET_REPORT / SET_REPORT where supported | M5W's YICHIP_COMMANDS has `set_report: None, get_report: None` -- the M5W does not expose polling rate commands in its protocol family. GET_REPORT (0x83) can still be probed as a shared command. Rate codes: 0=8kHz, 1=4kHz, 2=2kHz, 3=1kHz, 4=500Hz, 5=250Hz, 6=125Hz. Checksum: Bit7. |
</phase_requirements>

## Architecture Patterns

### No New Architecture Required

Phase 5 operates entirely within established patterns. No new crates, modules, or service endpoints.

### Existing Pattern: Hardware Test Structure
```
crates/monsgeek-transport/tests/hardware.rs
  - #![cfg(feature = "hardware")]
  - All tests are #[ignore]
  - Dangerous writes behind #[cfg(feature = "dangerous-hardware-writes")]
  - HW_LOCK mutex for serial device access
  - load_registry() -> load_m5w() -> connect() pattern
  - handle.send_query() for GET commands
  - handle.send_fire_and_forget() for SET commands
  - Read-write-readback-restore with nested closures for test/restore separation
```

### Existing Pattern: CommandSchemaMap Resolution
```
CommandSchemaMap::for_device(device)
  -> Registers device-specific commands as Known()
  -> Backfills shared commands as Shared()
  -> resolve(cmd_byte) returns CommandResolution
```

### Pattern: LED Command Wire Format
```
SET_LEDPARAM (0x07), Bit8 checksum:
  Payload (7 bytes): [mode, speed_inverted, brightness, option, r, g, b]
  - speed_inverted = 4 - user_speed (0=fast on wire, 4=slow on wire)
  - option: dazzle=7 (DAZZLE_ON), no_dazzle=8 (DAZZLE_OFF)
  - UserPicture mode: option = layer<<4, rgb = (0, 200, 200) -- OUT OF SCOPE

GET_LEDPARAM (0x87), Bit8 checksum:
  Response: [echo=0x87, mode, speed_inv, brightness, option, r, g, b]
  - Parse speed back: user_speed = 4 - speed_inv.min(4)
  - dazzle = (option & 0x0F) == 7
```

### Pattern: YiChip Debounce Wire Format
```
SET_DEBOUNCE (0x11 for YiChip), Bit7 checksum:
  Payload: [0x00, value] (PrependProfileZero normalizer)
  - value range: 0-50 ms

GET_DEBOUNCE (0x91 for YiChip), Bit7 checksum:
  Response: [echo=0x91, profile_byte, debounce_ms, ...]
  - Debounce value is at response[2], NOT response[1]
  - This differs from RY5088 where debounce is at response[1]
```

### Pattern: Polling Rate Wire Format
```
SET_REPORT (0x03), Bit7 checksum:
  Payload: [rate_code]
  - Rate codes: 0=8kHz, 1=4kHz, 2=2kHz, 3=1kHz, 4=500Hz, 5=250Hz, 6=125Hz

GET_REPORT (0x83), Bit7 checksum:
  Response: [echo=0x83, rate_code]
```

### Anti-Patterns to Avoid
- **Hardcoding command bytes in tests:** Use `device.commands()` to get family-resolved command bytes. The M5W uses 0x11/0x91 for debounce, not the shared 0x06/0x86.
- **Assuming response layout is uniform across families:** YiChip GET_DEBOUNCE has debounce at byte offset 2 (after profile byte); RY5088 has it at offset 1. Tests must use family-aware decoding.
- **Using ChecksumType::Bit7 for LED commands:** LED commands use Bit8. The reference implementation confirms `SetLedParams::CHECKSUM = ChecksumType::Bit8`. Every other command in this phase uses Bit7.

## CommandSchemaMap Audit Findings

These are concrete issues found by reading the current `command_schema.rs` against the reference implementation.

### Finding 1: LED Commands Not Device-Specific

**Current state:** `SET_LEDPARAM` (0x07) and `GET_LEDPARAM` (0x87) are registered only via `register_shared_commands()` as `Shared(VariableWithMax)`.

**Issue:** They should arguably be in the device-specific section for devices with `hasLightLayout: true`, since not all devices support LED control. However, since they use the same command bytes across all families, `Shared` registration is functionally correct for the current bridge passthrough model. No schema bug, just a documentation/completeness note.

**Recommendation:** Leave as-is. The schemas are not enforced at runtime anyway. Adding them as `Known` for LED-capable devices is a future refinement.

### Finding 2: SET_REPORT and GET_REPORT Missing for Shared Registration

**Current state:** `SET_REPORT` (0x03) and `GET_REPORT` (0x83) are only registered as `Known` when the device's `CommandTable` has `set_report: Some(0x03)` / `get_report: Some(0x83)`. For YiChip devices like M5W, both are `None`, so these commands resolve as `Unknown`.

They are also NOT in `register_shared_commands()`, unlike `GET_LEDPARAM` and other shared GET commands.

**Impact:** If someone sends GET_REPORT through the bridge to an M5W, the schema map would flag it as `Unknown`. Since the schema map is dead code at runtime this has no practical effect, but it's incorrect for devices where polling rate might still be probed.

**Recommendation:** Add `GET_REPORT` and `SET_REPORT` to `register_shared_commands()` alongside other shared GET/SET commands. This ensures they resolve as `Shared(VariableWithMax)` even for devices without explicit family-level support.

### Finding 3: Checksum Types Not Tracked in Schema

**Current state:** `CommandSchemaMap` tracks payload shape only, not checksum type. The reference implementation shows:
- SET_LEDPARAM / GET_LEDPARAM: **Bit8** checksum
- SET_DEBOUNCE / GET_DEBOUNCE: **Bit7** checksum
- SET_REPORT / GET_REPORT: **Bit7** checksum

The bridge relies on the web client to specify the correct checksum type. The schema map has no way to validate or enforce this.

**Recommendation:** This is out of scope for Phase 5 (activating CommandSchemaMap at runtime is explicitly deferred). Document the correct checksum assignments in code comments during the audit.

### Finding 4: GET_LEDONOFF (0x85) Collision

**Current state:** Shared `GET_LEDONOFF` (0x85) collides with YiChip `get_profile` (0x85). The collision is correctly handled -- device-specific `Known` wins via `HashMap` insertion order. This is already tested.

**No action needed.** Already verified in existing unit tests.

## Common Pitfalls

### Pitfall 1: YiChip Debounce Response Offset

**What goes wrong:** Reading debounce value from `response[1]` instead of `response[2]` on YiChip devices.
**Why it happens:** RY5088 GET_DEBOUNCE response is `[echo, debounce_ms]`, but YiChip response is `[echo, profile_byte, debounce_ms]` due to PrependProfileZero normalization.
**How to avoid:** The existing `decode_debounce()` helper in `hardware.rs` already handles this correctly -- it checks the command byte to determine offset. Reuse this pattern.
**Warning signs:** Debounce read-back value doesn't match what was written, but the echo byte is correct.

### Pitfall 2: LED Speed Inversion

**What goes wrong:** Writing `speed=3` expecting fast, but getting medium-slow because the wire inverts the value.
**Why it happens:** Protocol uses `speed_wire = 4 - speed_user` where 0=fastest on wire.
**How to avoid:** Tests should verify raw wire values. When setting speed=3 (fast), expect `speed_inv=1` on wire. When reading `speed_inv=1` from response, convert back to `speed_user=3`.
**Warning signs:** LED speed appears to change in the wrong direction.

### Pitfall 3: GET_REPORT on M5W May Timeout or Return Garbage

**What goes wrong:** Sending GET_REPORT (0x83) to M5W expecting a clean response, but the firmware may not recognize this command for YiChip.
**Why it happens:** YICHIP_COMMANDS has `get_report: None`. The firmware may ignore the command entirely, return a stale buffer, or echo it with meaningless data.
**How to avoid:** The polling rate probe test must handle all three cases: (a) valid response with rate code at data[1], (b) no response / timeout, (c) response with echo mismatch or all-zeros. Do not assert correctness -- just document what happens.
**Warning signs:** Test hangs waiting for response, or asserts on data that's actually a stale buffer from a previous command.

### Pitfall 4: Forgetting Bit8 for LED Commands

**What goes wrong:** Using `ChecksumType::Bit7` (the default for most commands) when sending SET_LEDPARAM or GET_LEDPARAM.
**Why it happens:** Every other command in scope for this phase uses Bit7. Easy to copy-paste the wrong checksum type.
**How to avoid:** LED commands are the exception. `Bit8` is confirmed by the reference: `SetLedParams::CHECKSUM = ChecksumType::Bit8`, and all `QueryCommand<CMD>` types default to Bit7, so `QueryLedParams` would use the wrong checksum if used directly.
**Warning signs:** Firmware ignores the command or returns unexpected data because the checksum byte is wrong.

## Code Examples

### Hardware Test: GET_LEDPARAM Read
```rust
// Source: reference command.rs LedParamsResponse parsing + existing hardware.rs patterns
#[test]
#[ignore]
fn test_get_ledparam() {
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    // GET_LEDPARAM uses Bit8 checksum (not Bit7!)
    let response = handle
        .send_query(cmd::GET_LEDPARAM, &[], ChecksumType::Bit8)
        .expect("GET_LEDPARAM query failed");

    assert_eq!(response[0], cmd::GET_LEDPARAM,
        "echo mismatch: expected 0x{:02X}, got 0x{:02X}",
        cmd::GET_LEDPARAM, response[0]);

    // Response layout: [echo, mode, speed_inv, brightness, option, r, g, b]
    let mode = response[1];
    let speed_inv = response[2];
    let brightness = response[3];
    let option = response[4];
    let r = response[5];
    let g = response[6];
    let b = response[7];

    println!("LED params: mode={} speed_inv={} brightness={} option=0x{:02X} rgb=({},{},{})",
        mode, speed_inv, brightness, option, r, g, b);

    // Mode should be a valid LedMode (0-24)
    assert!(mode <= 24, "LED mode {} exceeds known range 0-24", mode);
    // Brightness should be 0-4
    assert!(brightness <= 4, "brightness {} exceeds range 0-4", brightness);
    // Speed inverted should be 0-4
    assert!(speed_inv <= 4, "speed_inv {} exceeds range 0-4", speed_inv);

    handle.shutdown();
}
```

### Hardware Test: SET_LEDPARAM Write-Readback-Restore
```rust
// Source: reference SetLedParams::to_data() + existing debounce round-trip pattern
#[cfg(feature = "dangerous-hardware-writes")]
#[test]
#[ignore]
fn test_set_get_ledparam_round_trip_dangerous() {
    require_dangerous_write_opt_in();
    let _lock = HW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let registry = load_registry();
    let m5w = load_m5w(&registry);
    let (handle, _events) = connect(m5w).expect("failed to connect to M5W");

    // Read current LED state
    let original = handle
        .send_query(cmd::GET_LEDPARAM, &[], ChecksumType::Bit8)
        .expect("GET_LEDPARAM failed");
    let original_payload = original[1..8].to_vec(); // [mode, speed_inv, bright, option, r, g, b]

    // SET_LEDPARAM payload: [mode, speed_inv, brightness, option, r, g, b]
    // Test: set to Breathing (mode=2), speed_inv=2, brightness=3, dazzle_off=8, green
    let test_payload: Vec<u8> = vec![2, 2, 3, 8, 0, 255, 0];

    let test_result = (|| -> Result<(), String> {
        handle
            .send_fire_and_forget(cmd::SET_LEDPARAM, &test_payload, ChecksumType::Bit8)
            .map_err(|e| format!("SET_LEDPARAM failed: {e}"))?;

        let readback = handle
            .send_query(cmd::GET_LEDPARAM, &[], ChecksumType::Bit8)
            .map_err(|e| format!("GET_LEDPARAM after set failed: {e}"))?;

        // Verify mode changed
        if readback[1] != 2 {
            return Err(format!("mode mismatch: set 2, read {}", readback[1]));
        }
        Ok(())
    })();

    // Restore original
    let restore_result = (|| -> Result<(), String> {
        handle
            .send_fire_and_forget(cmd::SET_LEDPARAM, &original_payload, ChecksumType::Bit8)
            .map_err(|e| format!("restore failed: {e}"))?;
        Ok(())
    })();

    handle.shutdown();
    if let Err(err) = restore_result { panic!("{err}"); }
    if let Err(err) = test_result { panic!("{err}"); }
}
```

### Schema Audit: Adding Missing Shared Commands
```rust
// In register_shared_commands(), add SET_REPORT and GET_REPORT:
entries
    .entry(cmd::SET_REPORT)
    .or_insert_with(|| CommandResolution::Shared(var_max.clone()));

// GET_REPORT should be in the shared GET loop:
for &cmd_byte in &[
    cmd::GET_REV,
    cmd::GET_LEDONOFF,
    cmd::GET_LEDPARAM,
    cmd::GET_SLEDPARAM,
    cmd::GET_REPORT,  // <-- add this
    cmd::GET_USB_VERSION,
    // ... rest unchanged
] {
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Command byte resolution | Hardcoded match arms per device | `device.commands()` returning `CommandTable` | YiChip overrides 8 command bytes; table resolution handles this |
| Debounce response decoding | Uniform `response[1]` parsing | Family-aware `decode_debounce()` helper checking command byte | YiChip has profile byte at index 1, pushing debounce to index 2 |
| LED speed conversion | Manual subtraction in tests | Centralized inversion `speed_wire = 4 - speed_user` | Easy to get the direction wrong; one canonical conversion |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) |
| Config file | `crates/monsgeek-transport/Cargo.toml` (features: `hardware`, `dangerous-hardware-writes`) |
| Quick run command | `cargo test -p monsgeek-protocol -- --nocapture` |
| Full suite command | `cargo test -p monsgeek-transport --features hardware -- --ignored --nocapture` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LED-01 | Read LED params via GET_LEDPARAM | hardware integration | `cargo test -p monsgeek-transport --features hardware -- --ignored test_get_ledparam --nocapture` | Wave 0 |
| LED-02 | Set LED params via SET_LEDPARAM | hardware integration (dangerous) | `MONSGEEK_ENABLE_DANGEROUS_WRITES=1 cargo test -p monsgeek-transport --features "hardware dangerous-hardware-writes" -- --ignored test_set_get_ledparam_round_trip_dangerous --nocapture` | Wave 0 |
| TUNE-01 | Read/set debounce | hardware integration (dangerous) | `MONSGEEK_ENABLE_DANGEROUS_WRITES=1 cargo test -p monsgeek-transport --features "hardware dangerous-hardware-writes" -- --ignored test_set_get_debounce_round_trip_dangerous --nocapture` | Exists (Phase 2) |
| TUNE-02 | Read/set polling rate where supported | hardware integration | `cargo test -p monsgeek-transport --features hardware -- --ignored test_probe_polling_rate --nocapture` | Wave 0 |
| Schema | CommandSchemaMap audit correctness | unit test | `cargo test -p monsgeek-protocol -- --nocapture` | Partially exists |

### Sampling Rate
- **Per task commit:** `cargo test -p monsgeek-protocol -- --nocapture`
- **Per wave merge:** `cargo test -p monsgeek-transport --features hardware -- --ignored --nocapture` (read-only tests only)
- **Phase gate:** Full suite including dangerous writes + browser checkpoint before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `test_get_ledparam` -- hardware test for LED-01 (read-only, no dangerous flag needed)
- [ ] `test_set_get_ledparam_round_trip_dangerous` -- hardware test for LED-02 (dangerous writes)
- [ ] `test_probe_polling_rate` -- hardware test for TUNE-02 (read-only probe, may timeout)
- [ ] Schema audit unit tests for newly added shared command entries

*(Existing `test_set_get_debounce_round_trip_dangerous` covers TUNE-01)*

## Open Questions

1. **GET_REPORT response on M5W: timeout vs garbage vs valid?**
   - What we know: YICHIP_COMMANDS has `get_report: None`, meaning the YiChip family officially doesn't support this command. The shared constant GET_REPORT=0x83 exists.
   - What's unclear: Whether the M5W firmware silently ignores it, returns an error frame, or happens to support it despite not being in the family table.
   - Recommendation: The hardware probe test will answer this definitively. Handle all three outcomes without failing the test.

2. **QueryLedParams type alias uses Bit7 (wrong for LED)**
   - What we know: In the reference, `QueryCommand<CMD>` defaults to `ChecksumType::Bit7`. The type alias `QueryLedParams = QueryCommand<{ cmd::GET_LEDPARAM }>` therefore uses Bit7, but LED commands require Bit8.
   - What's unclear: Whether the firmware actually rejects Bit7-checksummed LED queries or silently accepts them.
   - Recommendation: Hardware tests should use explicit `ChecksumType::Bit8` for LED commands. The QueryLedParams alias from the reference is incorrect for our use case.

## Sources

### Primary (HIGH confidence)
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/command.rs` -- SetLedParams, LedParamsResponse, SetDebounce, DebounceResponse, SetPollingRate, PollingRate enum, ChecksumType assignments
- `crates/monsgeek-protocol/src/command_schema.rs` -- CommandSchemaMap implementation, register_shared_commands, device-specific entries
- `crates/monsgeek-protocol/src/protocol.rs` -- YICHIP_COMMANDS (set_report: None, get_report: None), RY5088_COMMANDS (set_report: Some(0x03), get_report: Some(0x83))
- `crates/monsgeek-protocol/devices/m5w.json` -- M5W device definition with commandOverrides
- `crates/monsgeek-transport/tests/hardware.rs` -- Existing hardware test patterns, debounce round-trip, decode_debounce helper

### Secondary (MEDIUM confidence)
- `crates/monsgeek-driver/src/service/mod.rs` -- Bridge passthrough model, proto_checksum_to_protocol mapping (0=Bit7, 1=Bit8, 2=None)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new dependencies, all code is in-tree and read directly
- Architecture: HIGH -- no new architecture, all patterns established in prior phases
- Pitfalls: HIGH -- wire format details confirmed from reference implementation source code
- Schema audit: HIGH -- direct comparison of command_schema.rs against reference command.rs

**Research date:** 2026-03-26
**Valid until:** Indefinite (firmware protocol is stable; all findings are from source code, not external docs)
