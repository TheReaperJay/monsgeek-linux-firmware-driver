---
phase: 01-project-scaffolding-device-registry
verified: 2026-03-19T10:30:00Z
status: passed
score: 9/9 must-haves verified
re_verification: false
---

# Phase 01: Project Scaffolding & Device Registry Verification Report

> Historical correction (2026-03-23): this verification report passed against an early registry entry that later proved to use the wrong M5W USB IDs. The architectural verification remains useful, but the authoritative M5W wired identity is now `0x3151:0x4015`.

**Phase Goal:** Scaffold Rust workspace (3 crates), implement JSON-driven device registry, define FEA protocol constants and checksum algorithms.
**Verified:** 2026-03-19T10:30:00Z
**Status:** PASSED
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | M5W device definition loads from JSON with the correct device metadata, later corrected to wired VID 0x3151, PID 0x4015, device ID 1308, keyLayoutName Common108_MG108B | VERIFIED | registry architecture was verified; wired USB IDs were corrected later through hardware validation |
| 2 | DeviceRegistry scans devices/ directory and returns devices by ID and by VID/PID | VERIFIED | `registry.rs` implements `load_from_directory` + dual HashMap indexes; `test_load_m5w_from_devices_dir`, `test_find_by_vid_pid_m5w` pass |
| 3 | Adding a new JSON file to devices/ makes it discoverable without modifying Rust source | VERIFIED | `test_registry_extensible` writes two JSON files to a temp dir, loads registry, asserts both found by ID |
| 4 | Workspace compiles with three crates: monsgeek-protocol, monsgeek-transport, monsgeek-driver | VERIFIED | `cargo build --workspace` succeeds; all 73 tests pass across the workspace |
| 5 | FEA command constants match reference implementation byte values exactly | VERIFIED | `cmd.rs` contains all SET/GET/dongle constants; spot-checks pass: SET_LEDPARAM=0x07, GET_USB_VERSION=0x8F, GET_DONGLE_STATUS=0xF7, STATUS_SUCCESS=0xAA |
| 6 | Bit7 checksum produces correct value for known test vectors from reference | VERIFIED | `test_checksum_bit7_single_byte`: 0x8F -> 0x70; `test_build_command_get_usb_version`: buf[8]=0x70; `test_checksum_bit7_multiple_bytes`: sum=28, result=227 |
| 7 | build_command produces 65-byte buffer with report ID 0 at byte 0, command at byte 1, checksum at correct position | VERIFIED | `build_command` allocates `vec![0u8; hid::REPORT_SIZE]`; applies checksum to `buf[1..]` (excluding report ID); `test_build_command_checksum_excludes_report_id` explicitly verifies this |
| 8 | Protocol family detection identifies M5W as YiChip by name prefix yc3121_ and by PID-family heuristic | VERIFIED | `test_detect_yichip_by_name` and `test_detect_yichip_by_pid` both pass; case-insensitive detection also tested |
| 9 | YiChip and RY5088 command tables contain divergent command bytes for the commands that differ | VERIFIED | YICHIP_COMMANDS.set_reset=0x02 vs RY5088_COMMANDS.set_reset=0x01; YICHIP_COMMANDS.set_debounce=0x11 vs 0x06; `set_report=None` on YiChip vs `Some(0x03)` on RY5088 |

**Score:** 9/9 truths verified

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `Cargo.toml` | Workspace root with three crate members | VERIFIED | Contains `members = ["crates/*"]`, `resolver = "2"` |
| `crates/monsgeek-protocol/Cargo.toml` | Protocol crate with serde, serde_json, thiserror, glob | VERIFIED | All four dependencies present; `edition = "2024"` |
| `crates/monsgeek-protocol/src/device.rs` | DeviceDefinition struct with serde deserialization | VERIFIED | `pub struct DeviceDefinition` with `#[serde(rename_all = "camelCase")]`; exports DeviceDefinition, FnSysLayer, TravelSetting, RangeConfig |
| `crates/monsgeek-protocol/src/registry.rs` | DeviceRegistry with directory scanning and multi-index lookup | VERIFIED | `HashMap<i32, DeviceDefinition>` + `HashMap<(u16, u16), Vec<i32>>`; `load_from_directory`, `find_by_id`, `find_by_vid_pid` all present |
| `crates/monsgeek-protocol/devices/m5w.json` | M5W device definition with corrected constants | VERIFIED | current canonical wired identity is id=1308, vid=12625 (0x3151), pid=16405 (0x4015), keyLayoutName="Common108_MG108B" |
| `crates/monsgeek-protocol/src/error.rs` | Error types for protocol and registry operations | VERIFIED | `ProtocolError` (InvalidChecksum, InvalidCommand, ResponseError) and `RegistryError` (GlobPattern, ReadFile, ParseJson, DuplicateDeviceId, NoDevicesFound) |
| `crates/monsgeek-protocol/src/cmd.rs` | All FEA SET and GET command constants, dongle commands, response status | VERIFIED | 21 SET, 21 GET, 10 dongle constants + STATUS_SUCCESS; `pub fn name(cmd: u8)` present |
| `crates/monsgeek-protocol/src/checksum.rs` | ChecksumType enum, calculate_checksum, apply_checksum, build_command | VERIFIED | All four exports present plus `build_ble_command`; `#[default] Bit7` confirmed |
| `crates/monsgeek-protocol/src/protocol.rs` | ProtocolFamily enum, CommandTable struct, RY5088_COMMANDS, YICHIP_COMMANDS | VERIFIED | `#[default] Ry5088`; CommandTable with 14 fields including Optional commands for family-exclusive ops |
| `crates/monsgeek-protocol/src/magnetism.rs` | Magnetism sub-command constants | VERIFIED | 13 constants; PRESS_TRAVEL=0x00; CALIBRATION=0xFE; `pub fn name()` present |
| `crates/monsgeek-protocol/src/timing.rs` | Timing constants for HID communication | VERIFIED | DEFAULT_DELAY_MS=100; QUERY_RETRIES=5; `pub mod dongle` sub-module present |
| `crates/monsgeek-protocol/src/hid.rs` | HID report sizes, usage pages, interface numbers | VERIFIED | REPORT_SIZE=65; INPUT_REPORT_SIZE=64; USAGE_PAGE=0xFFFF; INTERFACE_FEATURE=2; `is_vendor_usage_page()` present |
| `crates/monsgeek-protocol/src/ble.rs` | BLE protocol constants | VERIFIED | VENDOR_REPORT_ID=0x06; CMDRESP_MARKER=0x55; REPORT_SIZE=66 |
| `crates/monsgeek-protocol/src/rgb.rs` | RGB/LED data constants | VERIFIED | TOTAL_RGB_SIZE=378; NUM_PAGES=7; MATRIX_SIZE=126 |
| `crates/monsgeek-protocol/src/precision.rs` | Firmware version thresholds for precision levels | VERIFIED | FINE_VERSION=1280; MEDIUM_VERSION=768 |
| `crates/monsgeek-transport/src/lib.rs` | Empty shell for Phase 2 | VERIFIED | Doc comment present; intentionally empty pending Phase 2 |
| `crates/monsgeek-driver/src/main.rs` | Minimal binary printing version | VERIFIED | `fn main()` prints version via `env!("CARGO_PKG_VERSION")` |
| `crates/monsgeek-protocol/src/lib.rs` | All module declarations and re-exports | VERIFIED | 12 `pub mod` declarations; re-exports DeviceDefinition, DeviceRegistry, ProtocolError, RegistryError, ChecksumType, all checksum functions, ProtocolFamily, CommandTable, RY5088_COMMANDS, YICHIP_COMMANDS |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `registry.rs` | `devices/m5w.json` | glob scan + serde_json deserialization | VERIFIED | `glob::glob(pattern_str)` + `serde_json::from_str::<DeviceDefinition>` in `load_from_directory` |
| `registry.rs` | `device.rs` | DeviceDefinition type used in HashMap | VERIFIED | `HashMap<i32, DeviceDefinition>` at line 13; DeviceDefinition imported from `crate::device` |
| `checksum.rs` | `hid.rs` | REPORT_SIZE used in build_command buffer allocation | VERIFIED | `vec![0u8; crate::hid::REPORT_SIZE]` in `build_command`; `crate::ble::REPORT_SIZE` in `build_ble_command` |
| `protocol.rs` | `cmd.rs` | CommandTable contains command byte values from shared cmd space | VERIFIED | RY5088_COMMANDS and YICHIP_COMMANDS static instances present with literal byte values |
| `checksum.rs` | `protocol.rs` | ChecksumType is protocol-aware | VERIFIED | ChecksumType exported from lib.rs; used by build_command/build_ble_command which are the protocol-level builders |

---

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| REG-01 | 01-01-PLAN.md, 01-02-PLAN.md | Device registry contains M5W definition (wired VID 0x3151, PID 0x4015, key matrix Common108_MG108B, device ID 1308) | SATISFIED | current planning truth uses the corrected wired M5W identity |
| REG-02 | 01-01-PLAN.md, 01-02-PLAN.md | Device registry is extensible — adding a new yc3121 keyboard requires only a JSON definition file | SATISFIED | `test_registry_extensible` writes a second JSON file at runtime and verifies it is discovered without any Rust source changes |

No orphaned requirements — REQUIREMENTS.md Traceability table maps only REG-01 and REG-02 to Phase 1.

---

### Anti-Patterns Found

None. Full scan across all crate source files (`crates/**/*.rs`) found:
- No TODO/FIXME/XXX/HACK/PLACEHOLDER comments
- No stub return values (return null, return {}, return [])
- No empty catch blocks or swallowed errors
- The empty `monsgeek-transport/src/lib.rs` is intentional (documented in plan as Phase 2 deliverable) — not a stub, it is the correct state for this phase

---

### Human Verification Required

None. All phase deliverables are programmatically verifiable:
- Build success confirmed by `cargo test --workspace` (73/73 tests pass)
- Byte-level constants verified via unit tests
- File existence and content verified by direct read

---

### Test Suite Summary

Full workspace test run: **73 passed, 0 failed**

Breakdown by module:
- `device` — 7 tests (M5W deserialization, has_magnetism logic, optional fields, FnSysLayer)
- `registry` — 8 tests (directory load, find_by_id, find_by_vid_pid, extensibility, empty dir, shared VID/PID, invalid JSON)
- `cmd` — 8 tests (command byte spot-checks, name() lookup)
- `magnetism` — 6 tests (sub-command byte spot-checks, name() lookup)
- `checksum` — 11 tests (Bit7/Bit8/None algorithms, apply_checksum, build_command, BLE command, checksum-excludes-report-ID)
- `protocol` — 14 tests (family detection by name/PID/case, command table values, Display impl)
- `hid` — 7 tests (report sizes, usage pages, interface numbers, is_vendor_usage_page)
- `ble` — 3 tests (vendor report ID, marker, report size)
- `rgb` — 3 tests (total size, num pages, matrix size)
- `timing` — 3 tests (default delay, query retries, send retries)
- `precision` — 2 tests (fine and medium version thresholds)
- `monsgeek-driver` — 0 tests (minimal binary, no logic to test)
- `monsgeek-transport` — 0 tests (empty shell, Phase 2 deliverable)

---

### Commit Verification

All four commits from summaries confirmed present in git history:
- `968ae2c` — feat(01-01): scaffold workspace with device types, M5W JSON, and tests
- `7e81840` — test(01-01): add registry tests for directory scanning and multi-index lookup
- `1dc5818` — feat(01-02): add FEA command constants, magnetism sub-commands, and constant modules
- `c78afec` — feat(01-02): add checksum algorithms, protocol family detection, and command builders

---

_Verified: 2026-03-19T10:30:00Z_
_Verifier: Claude (gsd-verifier)_
