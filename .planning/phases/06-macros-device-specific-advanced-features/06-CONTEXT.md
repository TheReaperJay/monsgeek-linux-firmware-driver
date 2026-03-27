# Phase 6: Macros & Device-Specific Advanced Features - Context

**Gathered:** 2026-03-27
**Status:** Ready for planning

<domain>
## Phase Boundary

Macro programming (read, write, assign-to-key, trigger verification) verified end-to-end on real M5W hardware via the web configurator, plus protocol-layer support for device-specific magnetic/Hall-effect switch features (calibration, rapid trigger, per-key actuation points) that cannot be hardware-verified on M5W but are fully modeled from the reference implementation. SET_MACRO and SET_FN bounds validation deferred from Phase 4 are completed here. The bridge remains a raw passthrough — no new RPC logic.

</domain>

<decisions>
## Implementation Decisions

### Macro verification scope
- Full roundtrip on real M5W hardware: GET_MACRO read, SET_MACRO write (multi-page), readback verification, assign macro to a key, confirm macro executes when triggered, then restore
- Covers MACR-01 (read) and MACR-02 (program) completely
- Read-before-write, restore original: save slot contents before test, restore after verification — defensive approach that preserves any user data in the test slot

### Bounds validation (deferred from Phase 4)
- Add SET_MACRO bounds validation to `validate_dangerous_write` in the bridge service layer
  - macro_index: 0-49 (firmware maxMacro=50)
  - chunk_page: 0-9 (firmware page limit)
  - Prevents OOB writes that corrupt firmware macro flash pages
- Add SET_FN bounds validation alongside SET_MACRO
  - Same key_index bounds as SET_KEYMATRIX, resolved per-device via DeviceDefinition
  - Closes the last deferred bounds gap from Phase 4

### Magnetic/Hall-effect strategy
- Protocol support only — no hardware verification (M5W has `noMagneticSwitch: true`)
- Full port from reference implementation: command opcodes, wire format structs (actuation/deactuation bytes, per-key travel data), calibration state machine, progress tracking, per-key travel parsing, response parser
- Unit tests against mock data to verify wire format correctness
- MAG-01 through MAG-04 marked as "implemented, pending device validation" in traceability
- Gate magnetic switch commands per-device in `validate_dangerous_write`: reject SET_MAGNETISM and calibration commands at the bridge service boundary if the connected device has `noMagneticSwitch: true`, with a clear error message

### Browser verification checkpoint
- Macro: open app.monsgeek.com, navigate to macro editor, read existing macro slot, program test macro (key sequence with delays), assign to a key, press key to confirm execution, clear/restore
- Magnetic switch UI: skip — M5W won't render these controls. Not applicable for current hardware target.
- Phase is not complete until browser macro checkpoint passes on real M5W hardware

### CommandSchemaMap audit
- Audit and fix existing macro entries (set_macro uses `known_var` clone — verify against reference wire format: 7-byte header, multi-page payload, Bit7 checksum)
- Add GET_MACRO entry if missing
- Add all magnetic switch command entries (GET/SET_MAGNETISM, calibration commands)
- Hardware tests use CommandSchemaMap to construct commands, validating schema correctness against real firmware (macros) or unit tests (magnetic) — same two-birds-one-stone approach as Phase 5

### Claude's Discretion
- Exact magnetic switch command set organization (separate module vs. inline in existing files)
- Test scaffolding structure for macro hardware tests
- Which macro slot index to use for hardware testing
- Exact calibration state machine internal design
- Error handling for unexpected firmware responses during macro operations

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

**CRITICAL: The decompiled Electron app is the authoritative source for protocol verification. It OVERRIDES the reference akko project when they conflict.**

### Decompiled Electron app (PRIMARY — authoritative for protocol behavior)
- `references/monsgeek-hid-driver/drivers/MonsGeek_v4_setup_500.2.13_WIN2026032/` — Windows Electron app, extracted. Use for macro wire format verification, command byte sequences, magnetic switch protocol details
- `references/monsgeek-hid-driver/drivers/MonsGeek Driver v4500.2.9_MAC20251027/` — macOS Electron app, extracted. Cross-reference with Windows version

### Reference implementation (SECONDARY — use for architectural patterns, but verify protocol details against Electron app)
- `references/monsgeek-akko-linux/iot_driver_linux/src/commands/macros.rs` — Macro CLI commands: get_macro, set_macro, clear_macro, assign_macro, text preview reconstruction
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/command.rs` §1276-1370 — SET_MACRO wire format (7-byte header, multi-page), GET_MACRO (2-byte query), MAX_MACRO_INDEX=49, bounds checking
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/command.rs` §1073-1530 — Magnetism (Hall Effect) commands: GET/SET_MAGNETISM, calibration, per-key travel, actuation/deactuation bytes, response parsing
- `references/monsgeek-akko-linux/docs/bugs/get_macro_stride_bug.txt` — Known macro stride bug documentation
- `references/monsgeek-akko-linux/docs/bugs/oob_hazards.txt` — OOB write hazards documentation
- `references/monsgeek-akko-linux/docs/PROTOCOL.md` — Protocol documentation for macro and magnetic commands

### Protocol crate (our code)
- `crates/monsgeek-protocol/src/cmd.rs` — SET_MACRO=0x0B, GET_MACRO=0x8B command constants
- `crates/monsgeek-protocol/src/command_schema.rs` — CommandSchemaMap with existing set_macro entry (audit target)
- `crates/monsgeek-protocol/src/protocol.rs` — CommandTable with set_macro field (YiChip=0x08, RY5088=0x0B)
- `crates/monsgeek-protocol/src/device.rs` — DeviceDefinition with `noMagneticSwitch` field, command override resolution

### Bridge service layer (validation insertion point)
- `crates/monsgeek-driver/src/service/mod.rs` — `validate_dangerous_write` function (currently covers SET_KEYMATRIX only — SET_MACRO and SET_FN to be added)
- `crates/monsgeek-transport/src/bounds.rs` — `validate_write_request` for key_index/layer bounds (reusable for SET_FN)

### Device definition
- `crates/monsgeek-protocol/devices/m5w.json` — M5W device JSON: `noMagneticSwitch: true`, `keyCount: 108`, `layer: 4`, YiChip command overrides including `setMacro: 11`

### Prior phase context
- `.planning/phases/04-bridge-integration-key-remapping/04-CONTEXT.md` — Phase 4 decisions on validate_dangerous_write architecture, SET_MACRO/SET_FN deferral
- `.planning/phases/05-led-control-tuning/05-CONTEXT.md` — Phase 5 CommandSchemaMap audit pattern
- `.planning/REQUIREMENTS.md` — MACR-01, MACR-02, MAG-01 through MAG-04 requirements for this phase

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `validate_dangerous_write` in `service/mod.rs` — existing bounds validation for SET_KEYMATRIX/SET_KEYMATRIX_SIMPLE, extend for SET_MACRO and SET_FN
- `CommandSchemaMap` in `command_schema.rs` — existing set_macro entry (needs audit), used by hardware tests to construct commands
- `DeviceDefinition.commands()` — resolves per-device CommandTable with YiChip overrides (set_macro = 0x08 vs 0x0B)
- `bounds::validate_write_request` — reusable for SET_FN key_index/layer bounds
- Phase 4/5 hardware test pattern — feature-gated `#[cfg(feature = "hardware-test")]` with `#[ignore]`, read-write-readback-restore
- `DeviceDefinition` fields: `noMagneticSwitch`, `keyCount`, `layer` — used for per-device capability gating

### Established Patterns
- Bridge is fully generic raw passthrough — no command-specific routing needed
- `validate_dangerous_write` is the single point for indexed-write safety enforcement
- CommandSchemaMap is dead code at runtime but used by hardware tests to construct and validate commands
- Checksum type is client-controlled (bridge passthrough)
- Hardware tests construct commands via CommandSchemaMap, validating schema correctness by exercising against real firmware

### Integration Points
- `validate_dangerous_write` in `service/mod.rs` — add SET_MACRO and SET_FN branches
- `command_schema.rs` — audit existing macro entry, add magnetic command entries
- `cmd.rs` — add magnetic switch command constants if missing
- Hardware tests in transport crate alongside existing Phase 2/4/5 tests
- `DeviceDefinition` — `noMagneticSwitch` field already exists for capability gating

</code_context>

<specifics>
## Specific Ideas

- The decompiled Electron app is the ground truth for macro wire format — if it disagrees with the reference akko project, the Electron app wins
- SET_MACRO bounds: macro_index 0-49, chunk_page 0-9 (from reference `MAX_MACRO_INDEX` and firmware flash page layout)
- SET_FN bounds follow SET_KEYMATRIX pattern: same key_index limits from DeviceDefinition, resolved per-device
- Macro hardware test should use read-before-write, restore original approach — store original slot contents, write test macro, verify, then restore. More defensive than assuming empty slots.
- YiChip SET_MACRO command byte is 0x08 (not the shared 0x0B) — already handled by CommandTable overrides
- Magnetic switch capability gating at bridge service boundary: reject with clear error when device has `noMagneticSwitch: true`

</specifics>

<deferred>
## Deferred Ideas

- Macro execution timing measurement (how accurately firmware replays delays) — separate verification scope
- Macro text input mode vs. raw key sequence mode — the reference has both but Phase 6 focuses on protocol verification
- Per-key RGB streaming (LED_STREAM 0xE8) — still deferred from Phase 5
- Magnetic switch hardware verification — requires a device with magnetic switches, not available for M5W
- Activating CommandSchemaMap at runtime in the bridge send path — still dead code, could prevent malformed commands in future

</deferred>

---

*Phase: 06-macros-device-specific-advanced-features*
*Context gathered: 2026-03-27*
