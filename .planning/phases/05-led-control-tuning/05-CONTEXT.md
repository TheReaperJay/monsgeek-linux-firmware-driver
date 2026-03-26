# Phase 5: LED Control & Tuning - Context

**Gathered:** 2026-03-26
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can control RGB lighting and tune debounce/polling to address the ghosting/double-letter issue, all via the MonsGeek web configurator on Linux. The bridge is a fully generic raw passthrough — the web configurator constructs payloads and selects checksums itself. Phase 5 verifies these command round-trips work on real M5W hardware and fixes whatever doesn't. Per-key RGB streaming (LED_STREAM 0xE8) and the userspace debounce daemon are out of scope (Phase 5.1).

</domain>

<decisions>
## Implementation Decisions

### LED verification scope
- Verify GET_LEDPARAM and SET_LEDPARAM roundtrip only: read current mode, change effect, adjust brightness/speed/color, confirm keyboard responds
- Per-key RGB streaming (LED_STREAM 0xE8, 7-page protocol) and UserPicture mode are out of scope for this phase
- Side LED commands (SET_SLEDPARAM/GET_SLEDPARAM) are irrelevant — M5W has `hasSideLight: false`

### Ghosting diagnosis approach
- Phase 5 verifies firmware debounce read/write works — not a tuning/measurement phase
- Confirm GET_DEBOUNCE reads a value, SET_DEBOUNCE changes it, keyboard acknowledges the new value
- Whether firmware-side debounce tuning actually fixes ghosting is secondary — Phase 5.1 userspace daemon is the real fix
- No systematic debounce value sweep or double-letter frequency measurement in this phase

### Debounce hardware test pattern
- Read-write-readback-restore: read current value, write test value, readback to verify change took effect, restore original value
- Same safe pattern used in Phase 4 key remapping tests — leaves keyboard in original state

### Polling rate handling
- M5W has NO `reportRate` field in any device definition (Windows app, reference projects, or our JSON) — the configurator would not show polling rate controls for this device
- Phase 5 probes GET_REPORT (0x83) on real M5W hardware to determine if the firmware responds
- If firmware responds: document supported rates and the working command path
- If firmware ignores or rejects: document that M5W doesn't support user-configurable polling rate; the success criterion is satisfied by proving the command path works (limitation is the device, not the driver)
- Do not block phase completion on polling rate results

### Browser verification checkpoint
- Phase is not complete until manual browser checkpoint passes on real M5W hardware
- Open app.monsgeek.com, change LED effect mode, adjust brightness/speed/color, adjust debounce value, confirm keyboard responds to each
- Polling rate is excluded from browser checkpoint since the configurator doesn't show these controls for M5W

### Command schema audit
- Audit and complete all LED/debounce/polling entries in CommandSchemaMap for correctness against the reference implementation
- CommandSchemaMap is currently dead code at runtime (bridge does raw passthrough), but this pays down tech debt and validates protocol definitions
- Hardware integration tests construct commands via CommandSchemaMap, which validates schemas are correct by exercising them against real firmware
- Known schema facts to verify:
  - LED commands use Bit8 checksum
  - Debounce commands use Bit7 checksum
  - YiChip SET_DEBOUNCE uses `Normalized { wire_size: 2, normalizer: PrependProfileZero }` — single byte `[value]` becomes `[0x00, value]`
  - SET_REPORT/GET_REPORT use Bit7 checksum, wire format: `[cmd, 0x00, rate_code, 0x00, 0x00, 0x00, 0x00, checksum]`

### Claude's Discretion
- Exact LED parameter parsing in test assertions (which byte offsets to check in response)
- Whether to test multiple LED modes or just one mode change
- Error handling for unexpected firmware responses
- Test scaffolding structure

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### LED protocol
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/command.rs` — SetLedParams struct (7-byte payload: mode, speed_inverted, brightness, option, r, g, b), LedMode enum, LedParamsResponse parsing
- `references/monsgeek-akko-linux/iot_driver_linux/src/protocol.rs` — LED command construction and checksum type (Bit8)

### Debounce/polling protocol
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/command.rs` — SetDebounce (1-byte payload), SetPollingRate (1-byte payload with rate enum 0-6), response parsing
- `references/monsgeek-akko-linux/iot_driver_linux/src/protocol.rs` — Debounce/polling command construction, rate code mapping (0=8kHz through 6=125Hz)

### Protocol crate (our code)
- `crates/monsgeek-protocol/src/cmd.rs` — Command opcode constants (SET_LEDPARAM=0x07, GET_LEDPARAM=0x87, SET_DEBOUNCE=0x06, GET_DEBOUNCE=0x86, SET_REPORT=0x03, GET_REPORT=0x83)
- `crates/monsgeek-protocol/src/command_schema.rs` — CommandSchemaMap with payload validation rules and normalizers (audit target for this phase)
- `crates/monsgeek-protocol/src/protocol.rs` — ProtocolFamily, CommandTable, YiChip overrides (setDebounce=0x11, getDebounce=0x91)
- `crates/monsgeek-protocol/src/rgb.rs` — RGB constants (TOTAL_RGB_SIZE=378, NUM_PAGES=7, PAGE_SIZE=56, MATRIX_SIZE=126) — out of scope but exists for reference

### Device definition
- `crates/monsgeek-protocol/devices/m5w.json` — M5W device JSON (hasLightLayout:true, hasSideLight:false, commandOverrides for debounce, NO reportRate field)

### Bridge service layer (passthrough path)
- `crates/monsgeek-driver/src/service/mod.rs` — send_command_rpc: fully generic passthrough, only inspects SET_KEYMATRIX and SET_KEYMATRIX_SIMPLE commands
- `crates/monsgeek-driver/src/bridge_transport.rs` — send_fire_and_forget + read_feature_report pattern, checksum type is client-controlled

### Prior phase context
- `.planning/phases/04-bridge-integration-key-remapping/04-CONTEXT.md` — Phase 4 decisions on validation architecture and ConnectedDevice caching
- `.planning/REQUIREMENTS.md` — LED-01, LED-02, TUNE-01, TUNE-02 requirements for this phase

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `CommandSchemaMap` in `command_schema.rs` — payload validation rules per command, includes LED/debounce entries (audit target)
- `DeviceDefinition.commands()` — resolves per-device CommandTable with YiChip overrides for debounce command bytes
- Phase 4 hardware test pattern — feature-gated `#[cfg(feature = "hardware-test")]` with `#[ignore]`, read-write-readback-restore methodology
- `validate_dangerous_write` — only validates keymatrix commands; LED/debounce/polling are not dangerous writes and pass through unchecked

### Established Patterns
- Bridge is fully generic raw passthrough — no command-specific routing for LED or debounce
- Checksum type is client-controlled: sendRawFeature always uses None, sendMsg uses client-specified type (0=Bit7, 1=Bit8, 2=None)
- `normalize_simple_keymatrix` is the only payload transformation in the bridge — only touches SIMPLE keymatrix commands
- ConnectedDevice caches DeviceDefinition at connection time (Phase 4)

### Integration Points
- Hardware tests go in the transport crate alongside existing Phase 2/4 tests
- Schema audit is in the protocol crate (`command_schema.rs`)
- No bridge-level changes expected — passthrough already handles everything

</code_context>

<specifics>
## Specific Ideas

- Hardware tests should use CommandSchemaMap to construct commands, validating schema correctness by exercising them against real firmware (two birds, one stone)
- LED speed is inverted on the wire (0=fast, 4=slow) — bridge doesn't touch this, web app handles it; tests should verify raw wire values match expected protocol
- The M5W's YiChip debounce override (cmd 0x11/0x91 instead of 0x06/0x86) with PrependProfileZero normalization is a key correctness check

</specifics>

<deferred>
## Deferred Ideas

- Per-key RGB streaming (LED_STREAM 0xE8) — complex 7-page protocol, separate verification scope
- UserPicture LED mode — uses different payload layout (layer<<4 in option byte, fixed RGB)
- Side LED support — M5W doesn't have side LEDs; verify on a device that does
- Activating CommandSchemaMap at runtime in the bridge send path — currently dead code, could prevent malformed commands
- Polling rate UI support — if M5W doesn't support it, consider whether to add a `reportRate` field to device JSON for devices that do
- Systematic debounce tuning / ghosting measurement — Phase 5.1 territory

</deferred>

---

*Phase: 05-led-control-tuning*
*Context gathered: 2026-03-26*
