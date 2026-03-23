# Phase 1: Project Scaffolding & Device Registry - Context

> Historical correction (2026-03-23): this context file predates the corrected M5W USB identity. Use the current project and roadmap docs for authoritative M5W constants and framework scope.

**Gathered:** 2026-03-19
**Status:** Ready for planning

<domain>
## Phase Boundary

Establish the Rust workspace and device registry so all subsequent phases build on a shared foundation with correct M5W device constants. Deliverables: compilable workspace with protocol and driver crates, JSON-driven device registry with M5W definition, FEA protocol constants and types with unit tests.

</domain>

<decisions>
## Implementation Decisions

### Workspace & crate structure
- Three crates: `monsgeek-protocol` (lib), `monsgeek-transport` (lib), `monsgeek-driver` (binary)
- `monsgeek-protocol` contains: FEA command constants, checksum logic, protocol types, report framing, device registry loading. Pure data and computation, no I/O, no OS dependencies.
- `monsgeek-transport` is an empty shell in Phase 1 — HID I/O, device discovery, udev, timing guards come in Phase 2. It depends on `monsgeek-protocol`.
- `monsgeek-driver` is an empty binary crate in Phase 1 — gRPC bridge and CLI come in later phases. Depends on both other crates.
- The roadmap's `monsgeek-keyboard` crate is deferred — feature-specific logic (LED types, magnetism settings, sync) doesn't exist until Phases 4-6. Add it when there's something to put in it.
- Diverges from roadmap crate names: `monsgeek-protocol` replaces `monsgeek-keyboard` because protocol knowledge is what Phase 1 actually produces, and it must be usable by all higher layers without pulling in transport dependencies.

### Device registry format
- Full device schema matching all fields from the reference project's `JsonDeviceDefinition`: id, vid, pid, name, displayName, company, type, sources, keyCount, keyLayoutName, layer, fnSysLayer, magnetism, noMagneticSwitch, hasLightLayout, hasSideLight, hotSwap, travelSetting, ledMatrix, chipFamily
- One JSON file per device (e.g., `devices/m5w.json`) — adding a new yc3121 keyboard means dropping a new JSON file, no Rust code changes
- The schema is designed for ALL yc3121 keyboards, not just M5W — M5W is simply the first device definition populated
- Device data extracted from the Windows Electron app's JS bundle (`firmware/MonsGeek_v4_setup_500.2.13_WIN2026032/dist/index.eb7071d5.js`) — this contains all device definitions, key matrices, and feature flags
- Registry API supports lookup by device ID (unique) and by VID/PID (may be ambiguous for shared-PID devices)

### Protocol constants scope
- Full FEA command set from the reference: all SET commands (0x01-0x65), all GET commands (0x80-0xE6), dongle commands, response status codes
- Both protocol families: RY5088 and YiChip `CommandTable` structs with divergent byte mappings
- `ProtocolFamily::detect()` logic based on device name prefix and PID heuristic
- All magnetism sub-commands (press travel, lift travel, RT press/lift, DKS, modtap, deadzones, key mode, snap tap, calibration)
- Checksum types: Bit7, Bit8, None — with `calculate_checksum`, `apply_checksum`, `build_command` functions
- Timing constants: query/send retries, default delay (100ms), short delay, streaming delay, animation delay
- HID report sizes (65 byte write, 64 byte read), usage pages, interface numbers
- BLE protocol constants (report ID, markers, buffer size, delay) — defined for completeness even though BLE transport is v2
- RGB/LED data constants (total size, page sizes, matrix size, chunk size)
- Key matrices do NOT live in protocol code — they belong in device JSON definitions. The reference project's hardcoded `matrix` module is a device-specific pattern we avoid.

### Reference project usage
- Reference project (`references/monsgeek-akko-linux/`) is a knowledge source, not a code source — we read it to understand the protocol, then write our own implementation
- Extract from reference: command opcodes, checksum algorithms, report framing, protocol family detection, device schema shape, timing constants
- Extract from Windows Electron app: M5W-specific device data (VID, PID, device ID, key count, key layout, LED matrix, feature flags, chip family, travel settings)
- The reference uses the broader FEA-family device space and must be relied on architecturally; the verified wired M5W identity was later corrected to VID `0x3151`, PID `0x4015`

</decisions>

<specifics>
## Specific Ideas

- The protocol crate should be fully testable with zero OS dependencies — checksum computation, command building, device JSON loading all have pure-logic unit tests
- Device JSON files live in a `devices/` directory within the `monsgeek-protocol` crate
- The 41MB JS bundle from the Windows app needs to be parsed to extract M5W device constants — this is a one-time extraction task, not a runtime dependency

</specifics>

<canonical_refs>
## Canonical References

### Protocol knowledge
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/protocol.rs` — Full FEA command constants, checksum algorithms, protocol families, timing constants, report sizes
- `references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/types.rs` — ChecksumType enum, TransportDeviceInfo, transport types

### Device registry patterns
- `references/monsgeek-akko-linux/iot_driver_linux/src/device_loader.rs` — JsonDeviceDefinition schema, JsonDeviceFile wrapper, device field definitions
- `references/monsgeek-akko-linux/iot_driver_linux/src/devices.rs` — Device lookup API (by ID, by VID/PID), feature queries

### M5W device data source
- `firmware/MonsGeek_v4_setup_500.2.13_WIN2026032/dist/index.eb7071d5.js` — 41MB minified JS containing M5W device definition, Common108_MG108B key matrix, LED matrix, all feature flags

### Project requirements
- `.planning/REQUIREMENTS.md` — REG-01 (M5W definition), REG-02 (extensible registry)
- `.planning/ROADMAP.md` §Phase 1 — Success criteria and scope

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- No existing Rust code in the project — this is the first phase, greenfield

### Established Patterns
- Reference project's workspace layout (root binary + lib crates) informs our structure
- Reference project's `JsonDeviceDefinition` serde schema informs our device JSON format
- Reference project's `protocol.rs` command constant organization (modules: `cmd`, `magnetism`, `timing`, `rgb`, `ble`, `device`, `precision`) informs our protocol crate module structure

### Integration Points
- Phase 2 will depend on `monsgeek-protocol` for command constants and checksum logic
- Phase 3 will depend on `monsgeek-protocol` for device registry lookups (reporting devices to web configurator)
- Device JSON files are the extension point — new keyboards added here feed into all downstream phases

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 01-project-scaffolding-device-registry*
*Context gathered: 2026-03-19*
