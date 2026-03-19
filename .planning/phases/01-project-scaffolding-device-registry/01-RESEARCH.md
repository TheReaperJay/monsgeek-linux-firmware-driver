# Phase 1: Project Scaffolding & Device Registry - Research

**Researched:** 2026-03-19
**Domain:** Rust workspace structure, serde JSON device registry, HID protocol constants
**Confidence:** HIGH

## Summary

Phase 1 is a greenfield Rust project scaffolding exercise. The project root currently contains no Cargo.toml or any Rust source -- everything must be created from scratch. The core deliverables are: (1) a Cargo workspace with three crates, (2) a JSON-driven device registry that loads one-file-per-device definitions from a `devices/` directory, (3) FEA protocol constants including command opcodes, checksum algorithms, protocol family detection, and timing constants, all verified against the reference implementation.

The reference project (`references/monsgeek-akko-linux/iot_driver_linux/`) provides complete, working implementations of all protocol constants, checksum logic, device schema, and registry patterns. This is not a research-uncertain domain -- the exact byte values, algorithms, and type shapes are fully specified in the reference code. The work is primarily careful transcription and restructuring into a cleaner architecture (one JSON file per device instead of monolithic database, protocol crate separated from transport, no hardcoded matrix module).

**Primary recommendation:** Use Rust 2024 edition, serde/serde_json for device JSON, thiserror for errors. Copy protocol constants verbatim from the reference `protocol.rs`. Define the device schema matching `JsonDeviceDefinition` from the reference `device_loader.rs`. Build the registry to scan a `devices/` directory for per-device JSON files. Keep `monsgeek-protocol` zero-dependency on OS/IO -- it must be testable with `cargo test` alone.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- Three crates: `monsgeek-protocol` (lib), `monsgeek-transport` (lib), `monsgeek-driver` (binary)
- `monsgeek-protocol` contains: FEA command constants, checksum logic, protocol types, report framing, device registry loading. Pure data and computation, no I/O, no OS dependencies.
- `monsgeek-transport` is an empty shell in Phase 1 -- HID I/O, device discovery, udev, timing guards come in Phase 2. It depends on `monsgeek-protocol`.
- `monsgeek-driver` is an empty binary crate in Phase 1 -- gRPC bridge and CLI come in later phases. Depends on both other crates.
- The roadmap's `monsgeek-keyboard` crate is deferred -- feature-specific logic doesn't exist until Phases 4-6.
- Diverges from roadmap crate names: `monsgeek-protocol` replaces `monsgeek-keyboard` because protocol knowledge is what Phase 1 actually produces.
- Full device schema matching all fields from the reference project's `JsonDeviceDefinition`: id, vid, pid, name, displayName, company, type, sources, keyCount, keyLayoutName, layer, fnSysLayer, magnetism, noMagneticSwitch, hasLightLayout, hasSideLight, hotSwap, travelSetting, ledMatrix, chipFamily
- One JSON file per device (e.g., `devices/m5w.json`) -- adding a new yc3121 keyboard means dropping a new JSON file, no Rust code changes
- Schema designed for ALL yc3121 keyboards, not just M5W
- Device data extracted from the Windows Electron app's JS bundle
- Registry API supports lookup by device ID (unique) and by VID/PID (may be ambiguous)
- Full FEA command set from the reference: all SET commands (0x01-0x65), all GET commands (0x80-0xE6), dongle commands, response status codes
- Both protocol families: RY5088 and YiChip CommandTable structs with divergent byte mappings
- ProtocolFamily::detect() logic based on device name prefix and PID heuristic
- All magnetism sub-commands
- Checksum types: Bit7, Bit8, None -- with calculate_checksum, apply_checksum, build_command functions
- Timing constants: query/send retries, default delay (100ms), short delay, streaming delay, animation delay
- HID report sizes (65 byte write, 64 byte read), usage pages, interface numbers
- BLE protocol constants (report ID, markers, buffer size, delay) -- defined for completeness
- RGB/LED data constants
- Key matrices do NOT live in protocol code -- they belong in device JSON definitions
- Reference project is a knowledge source, not a code source
- The protocol crate should be fully testable with zero OS dependencies
- Device JSON files live in a `devices/` directory within the `monsgeek-protocol` crate

### Claude's Discretion
None specified -- all decisions were locked during discussion.

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| REG-01 | Device registry contains M5W definition (VID 0x3141, PID 0x4005, key matrix Common108_MG108B, device ID 1308) | Reference `device_loader.rs` defines the full `JsonDeviceDefinition` schema. M5W data must be extracted from the Windows Electron app JS bundle (`firmware/MonsGeek_v4_setup_500.2.13_WIN2026032/dist/index.eb7071d5.js`). VID is 0x3141 (MonsGeek), not 0x3151 (Akko). |
| REG-02 | Device registry is extensible -- adding a new yc3121 keyboard requires only a JSON definition file | One-file-per-device architecture in `devices/` directory. Registry scans directory, deserializes each `.json` file into `DeviceDefinition`. No Rust source changes needed for new devices. |

</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serde | 1.0.228 | Serialization framework | Universal Rust serialization. Required for JSON device definitions. |
| serde_json | 1.0.149 | JSON parsing/writing | De facto standard JSON crate. Parses device JSON files. |
| thiserror | 2.0.18 | Error type derivation | Idiomatic error types without boilerplate. `#[derive(Error)]` for registry and protocol errors. |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| glob | 0.3.3 | File pattern matching | Scanning `devices/*.json` directory for device definition files. |

### Not Needed Yet (Phase 2+)
| Library | Purpose | Phase |
|---------|---------|-------|
| hidapi | HID device communication | Phase 2 |
| tokio | Async runtime | Phase 2+ |
| tonic / prost | gRPC | Phase 3 |
| clap | CLI argument parsing | Phase 7 |

**Installation (workspace Cargo.toml dependencies for monsgeek-protocol):**
```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
glob = "0.3"
```

**Version verification:** All versions verified against `cargo search` on 2026-03-19. serde 1.0.228, serde_json 1.0.149, thiserror 2.0.18, glob 0.3.3.

## Architecture Patterns

### Recommended Project Structure
```
monsgeek-firmware-driver/
+-- Cargo.toml                  # Workspace root
+-- .gitignore                  # Rust gitignore (target/, *.swp, etc.)
+-- crates/
|   +-- monsgeek-protocol/
|   |   +-- Cargo.toml
|   |   +-- devices/
|   |   |   +-- m5w.json        # MonsGeek M5W device definition
|   |   +-- src/
|   |       +-- lib.rs          # Crate root, re-exports
|   |       +-- cmd.rs          # FEA command opcode constants
|   |       +-- magnetism.rs    # Magnetism sub-command constants
|   |       +-- checksum.rs     # ChecksumType, calculate_checksum, apply_checksum, build_command
|   |       +-- protocol.rs     # ProtocolFamily, CommandTable, RY5088/YiChip tables
|   |       +-- timing.rs       # Timing constants (delays, retries)
|   |       +-- hid.rs          # Report sizes, usage pages, interface numbers
|   |       +-- ble.rs          # BLE protocol constants
|   |       +-- rgb.rs          # RGB/LED data constants
|   |       +-- precision.rs    # Firmware version thresholds for precision levels
|   |       +-- device.rs       # DeviceDefinition struct (serde), FnSysLayer, TravelSetting, RangeConfig
|   |       +-- registry.rs     # DeviceRegistry: load from directory, lookup by ID, lookup by VID/PID
|   |       +-- error.rs        # ProtocolError, RegistryError
|   +-- monsgeek-transport/
|   |   +-- Cargo.toml
|   |   +-- src/
|   |       +-- lib.rs          # Empty shell -- "Phase 2 will add HID transport here"
|   +-- monsgeek-driver/
|       +-- Cargo.toml
|       +-- src/
|           +-- main.rs         # Minimal main -- prints version, exits
+-- firmware/                   # Existing: Windows app bundle for data extraction
+-- references/                 # Existing: reference implementation
```

### Pattern 1: One JSON File Per Device
**What:** Each keyboard device gets its own JSON file in `crates/monsgeek-protocol/devices/`. The registry scans this directory at load time.
**When to use:** Always -- this is the decided architecture.
**Example:**
```json
{
  "id": 1308,
  "vid": 12609,
  "pid": 16389,
  "name": "yc3121_m5w",
  "displayName": "MonsGeek M5W",
  "company": "MonsGeek",
  "type": "keyboard",
  "sources": ["usb"],
  "keyCount": 108,
  "keyLayoutName": "Common108_MG108B",
  "layer": 4,
  "fnSysLayer": {"win": 2, "mac": 2},
  "magnetism": false,
  "noMagneticSwitch": true,
  "hasLightLayout": true,
  "hasSideLight": false,
  "hotSwap": true,
  "chipFamily": "YC3121"
}
```
Note: VID 0x3141 = 12609 decimal, PID 0x4005 = 16389 decimal. The JSON stores numeric values; hex representations are for human reference only.

### Pattern 2: Registry with Multi-Index Lookup
**What:** `DeviceRegistry` maintains `HashMap<i32, DeviceDefinition>` (by device ID) and `HashMap<(u16, u16), Vec<i32>>` (by VID/PID -> list of device IDs). Lookups by device ID are unique; lookups by VID/PID may return multiple matches (shared-PID devices).
**When to use:** All device lookups throughout the application.
**Example:**
```rust
// Source: Reference device_loader.rs DeviceDatabase pattern
pub struct DeviceRegistry {
    devices_by_id: HashMap<i32, DeviceDefinition>,
    devices_by_vid_pid: HashMap<(u16, u16), Vec<i32>>,
}

impl DeviceRegistry {
    /// Load all device JSON files from a directory
    pub fn load_from_directory(dir: &Path) -> Result<Self, RegistryError> {
        let mut registry = Self::new();
        for entry in glob::glob(dir.join("*.json").to_str().unwrap())
            .map_err(|e| RegistryError::GlobPattern(e.to_string()))?
        {
            let path = entry.map_err(|e| RegistryError::ReadFile(e.to_string()))?;
            let content = std::fs::read_to_string(&path)
                .map_err(|e| RegistryError::ReadFile(format!("{}: {}", path.display(), e)))?;
            let device: DeviceDefinition = serde_json::from_str(&content)
                .map_err(|e| RegistryError::ParseJson(format!("{}: {}", path.display(), e)))?;
            registry.add_device(device);
        }
        Ok(registry)
    }

    pub fn find_by_id(&self, id: i32) -> Option<&DeviceDefinition> { ... }
    pub fn find_by_vid_pid(&self, vid: u16, pid: u16) -> Vec<&DeviceDefinition> { ... }
}
```

### Pattern 3: Checksum as Pure Function
**What:** Checksum computation is a pure function taking a byte slice and checksum type, returning a u8. No state, no IO, trivially testable.
**When to use:** All protocol command construction and response validation.
**Example:**
```rust
// Source: Reference protocol.rs calculate_checksum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChecksumType {
    #[default]
    Bit7,
    Bit8,
    None,
}

pub fn calculate_checksum(data: &[u8], checksum_type: ChecksumType) -> u8 {
    match checksum_type {
        ChecksumType::Bit7 => {
            let sum: u32 = data.iter().take(7).map(|&b| b as u32).sum();
            (255 - (sum & 0xFF)) as u8
        }
        ChecksumType::Bit8 => {
            let sum: u32 = data.iter().take(8).map(|&b| b as u32).sum();
            (255 - (sum & 0xFF)) as u8
        }
        ChecksumType::None => 0,
    }
}
```

### Pattern 4: Protocol Family Detection
**What:** Two protocol families (RY5088, YiChip) with different command byte mappings. Detection uses device name prefix first, then PID heuristic fallback.
**When to use:** When constructing commands for a specific device. The M5W is YiChip (PID 0x4005 matches 0x40xx pattern, and name prefix `yc3121_`).
**Example:**
```rust
// Source: Reference protocol.rs ProtocolFamily::detect
pub fn detect(device_name: Option<&str>, pid: u16) -> ProtocolFamily {
    if let Some(name) = device_name {
        let lower = name.to_ascii_lowercase();
        if lower.starts_with("ry5088_") || lower.starts_with("ry1086_") {
            return ProtocolFamily::Ry5088;
        }
        if lower.starts_with("yc500_")
            || lower.starts_with("yc300_")
            || lower.starts_with("yc3121_")
            || lower.starts_with("yc3123_")
        {
            return ProtocolFamily::YiChip;
        }
    }
    if pid & 0xFF00 == 0x4000 {
        return ProtocolFamily::YiChip;
    }
    ProtocolFamily::Ry5088
}
```

### Anti-Patterns to Avoid
- **Hardcoded key matrix in Rust source:** The reference project has a `matrix` module with hardcoded key names. Our architecture puts key matrices in device JSON files, not in Rust source. Do NOT create a hardcoded matrix module.
- **Monolithic devices.json:** The reference uses a single `devices.json` with hundreds of devices. Our architecture uses one file per device for extensibility. Do NOT create a single combined file.
- **OS dependencies in monsgeek-protocol:** This crate must be pure computation. No `hidapi`, no `tokio`, no `std::fs` except in the registry loader (which reads JSON files). The registry loader is the only part that touches the filesystem.
- **Vendor ID confusion:** The reference project uses VID 0x3151 (Akko). Our project uses VID 0x3141 (MonsGeek). These are DIFFERENT vendors sharing the same protocol. Do NOT copy the Akko VID.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON deserialization with rename rules | Custom JSON parser or manual field mapping | serde + serde_json with `#[serde(rename_all = "camelCase")]` | camelCase JSON fields map to snake_case Rust fields automatically; handles Option, defaults, nested structs |
| Error types with Display/Error impl | Manual Display + Error trait implementations | thiserror `#[derive(Error)]` | Eliminates boilerplate, ensures correct trait implementations |
| File glob pattern matching | Manual directory iteration with extension filtering | `glob` crate | Handles edge cases in path patterns, cross-platform |

**Key insight:** The protocol constants are EXACT byte values from firmware reverse engineering. There is nothing to "design" -- they must be transcribed accurately from the reference implementation. The only design work is in module organization and the registry loading pattern.

## Common Pitfalls

### Pitfall 1: VID/PID Numeric Representation Confusion
**What goes wrong:** JSON stores VID/PID as decimal integers (12609, 16389) while humans think in hex (0x3141, 0x4005). Mixing representations leads to wrong values.
**Why it happens:** The reference code has `vid_hex` and `pid_hex` string fields alongside numeric `vid` and `pid` fields.
**How to avoid:** Store VID/PID as `u16` in Rust. JSON files use decimal numbers. Provide display formatting that shows hex. Add constants: `pub const MONSGEEK_VID: u16 = 0x3141;`
**Warning signs:** Seeing the Akko VID (0x3151 = 12625) instead of MonsGeek VID (0x3141 = 12609) in device JSON.

### Pitfall 2: Serde camelCase Rename Mismatches
**What goes wrong:** The reference JSON uses camelCase field names (`keyCount`, `displayName`, `keyLayoutName`) but Rust uses snake_case. Missing `#[serde(rename_all = "camelCase")]` causes silent deserialization failures where fields become None/default.
**Why it happens:** Serde defaults to exact-match field names without rename attribute.
**How to avoid:** Always use `#[serde(rename_all = "camelCase")]` on the struct. For fields that don't follow the pattern (like `displayName` which is already camelCase), ensure the rename attribute handles it correctly. Test deserialization with actual JSON to catch mismatches.
**Warning signs:** Device loads successfully but all Optional fields are None.

### Pitfall 3: Checksum Byte Position Off-By-One
**What goes wrong:** The checksum covers bytes 1-7 (or 1-8) of the HID report, but the report has a report ID at byte 0. If `build_command` passes the wrong slice to `apply_checksum`, the checksum is wrong and the keyboard ignores the command.
**Why it happens:** `build_command` creates a 65-byte buffer where `buf[0]` is the report ID (0x00) and `buf[1]` is the command byte. The checksum function operates on `buf[1..]`, not `buf[0..]`.
**How to avoid:** Follow the reference exactly: `apply_checksum(&mut buf[1..], checksum_type)`. The checksum function takes the sub-slice starting at the command byte. Add unit tests that verify checksum values against known-good reference data.
**Warning signs:** Commands compile and send but keyboard never responds.

### Pitfall 4: Protocol Family Command Table Incompleteness
**What goes wrong:** Only a SUBSET of commands differ between RY5088 and YiChip. Most commands (SET_LEDPARAM, GET_USB_VERSION, etc.) use the same byte values across families. If you put ALL commands in the CommandTable struct, you'll have redundancy and confusion about which table to use.
**Why it happens:** The reference splits this clearly: shared commands in `cmd::` constants, divergent commands in `CommandTable`. Our code must preserve this split.
**How to avoid:** Copy the CommandTable struct from the reference exactly: only the 8 divergent commands (set_reset, set_profile, set_debounce, set_keymatrix, set_macro, get_profile, get_debounce, get_keymatrix) plus 6 RY5088-only optional commands.
**Warning signs:** Duplicate constant definitions or confusion about which lookup path to use.

### Pitfall 5: Device ID Type
**What goes wrong:** Device ID can be negative (the reference has `id: i32` and test data includes id=-100 for special devices). Using `u32` silently fails to parse negative IDs.
**Why it happens:** Some device IDs in the Akko ecosystem are negative for "help" or "virtual" devices.
**How to avoid:** Use `i32` for device ID, matching the reference's `JsonDeviceDefinition`.
**Warning signs:** JSON parsing error on devices with negative IDs.

### Pitfall 6: M5W Data Extraction from 41MB JS Bundle
**What goes wrong:** The M5W device data must be extracted from `firmware/MonsGeek_v4_setup_500.2.13_WIN2026032/dist/index.eb7071d5.js` -- a 41MB minified JavaScript file. Attempting to parse the entire file at once is slow and error-prone. The device definition is embedded in a JS object literal, not standalone JSON.
**Why it happens:** MonsGeek distributes device definitions only through their Windows Electron app.
**How to avoid:** This is a ONE-TIME manual extraction task, not a build step. Grep for known constants (VID 0x3141, device ID 1308, "Common108_MG108B") to find the relevant object. Extract and hand-convert to JSON. The result goes into `devices/m5w.json`.
**Warning signs:** Trying to automate JS parsing or treating this as a runtime dependency.

## Code Examples

Verified patterns from the reference implementation:

### Build Command with Checksum
```rust
// Source: Reference protocol.rs build_command
pub fn build_command(cmd: u8, data: &[u8], checksum_type: ChecksumType) -> Vec<u8> {
    let mut buf = vec![0u8; REPORT_SIZE]; // 65 bytes
    buf[0] = 0; // Report ID
    buf[1] = cmd;
    let len = std::cmp::min(data.len(), REPORT_SIZE - 2);
    buf[2..2 + len].copy_from_slice(&data[..len]);
    apply_checksum(&mut buf[1..], checksum_type); // Checksum starts at cmd byte
    buf
}
```

### Device Definition Struct
```rust
// Source: Reference device_loader.rs JsonDeviceDefinition (adapted for our schema)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDefinition {
    pub id: i32,
    pub vid: u16,
    pub pid: u16,
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(rename = "type", default = "default_device_type")]
    pub device_type: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub key_count: Option<u8>,
    #[serde(default)]
    pub key_layout_name: Option<String>,
    #[serde(default)]
    pub layer: Option<u8>,
    #[serde(default)]
    pub fn_sys_layer: Option<FnSysLayer>,
    #[serde(default)]
    pub magnetism: Option<bool>,
    #[serde(default)]
    pub no_magnetic_switch: Option<bool>,
    #[serde(default)]
    pub has_light_layout: Option<bool>,
    #[serde(default)]
    pub has_side_light: Option<bool>,
    #[serde(default)]
    pub hot_swap: Option<bool>,
    #[serde(default)]
    pub travel_setting: Option<TravelSetting>,
    #[serde(default)]
    pub led_matrix: Option<Vec<u8>>,
    #[serde(default)]
    pub chip_family: Option<String>,
}

fn default_device_type() -> String {
    "keyboard".to_string()
}
```

### YiChip vs RY5088 Command Table
```rust
// Source: Reference protocol.rs CommandTable, RY5088_COMMANDS, YICHIP_COMMANDS
pub struct CommandTable {
    pub set_reset: u8,
    pub set_profile: u8,
    pub set_debounce: u8,
    pub set_keymatrix: u8,
    pub set_macro: u8,
    pub get_profile: u8,
    pub get_debounce: u8,
    pub get_keymatrix: u8,
    pub set_report: Option<u8>,
    pub set_kboption: Option<u8>,
    pub set_sleeptime: Option<u8>,
    pub get_report: Option<u8>,
    pub get_kboption: Option<u8>,
    pub get_sleeptime: Option<u8>,
}

pub static RY5088_COMMANDS: CommandTable = CommandTable {
    set_reset: 0x01, set_profile: 0x04, set_debounce: 0x06,
    set_keymatrix: 0x0A, set_macro: 0x0B,
    get_profile: 0x84, get_debounce: 0x86, get_keymatrix: 0x8A,
    set_report: Some(0x03), set_kboption: Some(0x09), set_sleeptime: Some(0x11),
    get_report: Some(0x83), get_kboption: Some(0x89), get_sleeptime: Some(0x91),
};

pub static YICHIP_COMMANDS: CommandTable = CommandTable {
    set_reset: 0x02, set_profile: 0x05, set_debounce: 0x11,
    set_keymatrix: 0x09, set_macro: 0x08,
    get_profile: 0x85, get_debounce: 0x91, get_keymatrix: 0x89,
    set_report: None, set_kboption: None, set_sleeptime: None,
    get_report: None, get_kboption: Some(0x86), get_sleeptime: None,
};
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Rust edition 2021 | Rust edition 2024 | Rust 1.85.0, Feb 2025 | New default. Use `edition = "2024"` in all Cargo.toml files. Supports async closures, safer unsafe, let chains. |
| thiserror 1.x | thiserror 2.0 | Late 2024 | Major version bump. Use `thiserror = "2.0"` not `"1.0"`. |
| Monolithic devices.json | One JSON file per device | Architecture decision | Reference project uses single file with 400+ devices. Our approach is one file per device for extensibility. |

**Deprecated/outdated:**
- Rust edition 2021: Still works but 2024 is available on our toolchain (rustc 1.93.1). Use 2024.
- thiserror 1.x: Superseded by 2.0 with breaking changes. Use 2.0.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test framework (cargo test) |
| Config file | None needed -- Cargo.toml `[dev-dependencies]` section |
| Quick run command | `cargo test -p monsgeek-protocol` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| REG-01 | M5W device definition loads from JSON with correct VID/PID/ID/keyLayout | unit | `cargo test -p monsgeek-protocol -- test_m5w_device_definition` | Wave 0 |
| REG-01 | M5W VID is 0x3141, PID is 0x4005, device ID is 1308 | unit | `cargo test -p monsgeek-protocol -- test_m5w_identity` | Wave 0 |
| REG-02 | Registry loads multiple devices from directory without code changes | unit | `cargo test -p monsgeek-protocol -- test_registry_extensible` | Wave 0 |
| REG-02 | Adding a JSON file to devices/ makes it discoverable by registry | unit | `cargo test -p monsgeek-protocol -- test_add_device_json` | Wave 0 |
| (SC-4) | FEA command constants match reference values | unit | `cargo test -p monsgeek-protocol -- test_command_constants` | Wave 0 |
| (SC-4) | Bit7 checksum calculation matches reference | unit | `cargo test -p monsgeek-protocol -- test_checksum_bit7` | Wave 0 |
| (SC-4) | build_command produces correct buffer with checksum | unit | `cargo test -p monsgeek-protocol -- test_build_command` | Wave 0 |
| (SC-4) | Protocol family detection for YiChip by name prefix | unit | `cargo test -p monsgeek-protocol -- test_protocol_family_detect` | Wave 0 |
| (SC-4) | Protocol family detection for YiChip by PID heuristic | unit | `cargo test -p monsgeek-protocol -- test_protocol_family_pid` | Wave 0 |
| (SC-1) | Workspace compiles: all three crates resolve dependencies | build | `cargo build --workspace` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p monsgeek-protocol`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/monsgeek-protocol/` -- entire crate (this is a greenfield project)
- [ ] `crates/monsgeek-transport/` -- empty shell crate
- [ ] `crates/monsgeek-driver/` -- minimal binary crate
- [ ] `Cargo.toml` -- workspace root
- [ ] `.gitignore` -- Rust standard gitignore
- [ ] `crates/monsgeek-protocol/devices/m5w.json` -- M5W device definition (extracted from JS bundle)

## Open Questions

1. **M5W Device Data Extraction**
   - What we know: The data is in `firmware/MonsGeek_v4_setup_500.2.13_WIN2026032/dist/index.eb7071d5.js` (41MB minified JS). We know the VID (0x3141), PID (0x4005), device ID (1308), key layout name (Common108_MG108B).
   - What's unclear: Exact values for all optional fields (magnetism, hasSideLight, travelSetting, ledMatrix). The M5W may not have magnetic switches (it's a standard mechanical keyboard), so `magnetism` may be false and `noMagneticSwitch` may be true.
   - Recommendation: Grep the JS bundle for `1308` (device ID) and `Common108_MG108B` to find the device definition object. Extract all fields manually. For fields we cannot confirm, use reasonable defaults based on the M5W being a standard mechanical full-size keyboard (108 keys, no magnetism, has LED layout, no side light).

2. **LED Matrix Data for M5W**
   - What we know: The reference project stores LED matrix data (mapping matrix position index to HID keycode) in device definitions. The M5W's matrix is named Common108_MG108B.
   - What's unclear: Whether the M5W LED matrix data is accessible in the JS bundle or whether it needs separate extraction.
   - Recommendation: Search the JS bundle for "Common108_MG108B" to find the matrix data. If found, include it in the device JSON. If not found, the `led_matrix` field can be `null` in the JSON for Phase 1 -- it's only needed for LED effects in Phase 5.

3. **Crate Directory Convention: `crates/` vs Root**
   - What we know: The reference project puts crate directories at workspace root (`monsgeek-transport/`, `monsgeek-keyboard/`). A common Rust convention for multi-crate workspaces is a `crates/` subdirectory.
   - What's unclear: Which convention to follow.
   - Recommendation: Use `crates/` subdirectory. It keeps the workspace root clean and is the more common convention in larger Rust projects. The workspace `Cargo.toml` members list becomes `["crates/*"]`.

## Sources

### Primary (HIGH confidence)
- Reference `protocol.rs` (`references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/protocol.rs`) -- All FEA command constants, checksum algorithms, protocol families, timing constants, report sizes. Read in full.
- Reference `types.rs` (`references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/types.rs`) -- ChecksumType enum, transport types. Read in full.
- Reference `device_loader.rs` (`references/monsgeek-akko-linux/iot_driver_linux/src/device_loader.rs`) -- Full JsonDeviceDefinition schema, DeviceDatabase implementation, JSON loading patterns. Read in full.
- Reference `devices.rs` (`references/monsgeek-akko-linux/iot_driver_linux/src/devices.rs`) -- Device lookup API (by ID, by VID/PID), feature queries. Read in full.
- Reference `device_registry.rs` (`references/monsgeek-akko-linux/iot_driver_linux/monsgeek-transport/src/device_registry.rs`) -- VID constant, dongle/bluetooth PID detection. Read in full.
- `cargo search` output (2026-03-19) -- Current crate versions on crates.io.
- `rustc --version` output -- Rust 1.93.1, Rust 2024 edition supported.

### Secondary (MEDIUM confidence)
- [Rust 1.85.0 announcement](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/) -- Rust 2024 edition availability confirmed.

### Tertiary (LOW confidence)
- M5W device field values (magnetism, sidelight, travel settings) -- Must be extracted from JS bundle. Exact values unverified until extraction is performed.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- serde/serde_json/thiserror are the universal Rust choices for this domain. Versions verified against crates.io.
- Architecture: HIGH -- The three-crate workspace, one-file-per-device registry, and module structure are all locked decisions from CONTEXT.md. The reference implementation provides proven patterns.
- Protocol constants: HIGH -- Exact byte values are available in the reference `protocol.rs`. These are firmware-defined values, not opinion.
- Pitfalls: HIGH -- All identified from actual code review of the reference implementation and analysis of the design decisions.
- M5W device data: MEDIUM -- VID/PID/ID/keyLayoutName are confirmed. Optional fields (magnetism, travelSetting, ledMatrix) require JS bundle extraction.

**Research date:** 2026-03-19
**Valid until:** 2026-04-19 (stable domain -- protocol constants don't change, crate versions are stable)
