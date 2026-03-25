//! Per-device command payload schema definitions and resolution.
//!
//! Defines the expected wire-level payload shape for every command a device
//! supports, and provides a per-device lookup map (`CommandSchemaMap`) that the
//! transport controller uses for mandatory normalization and validation.
//!
//! Schema definitions live here (protocol knowledge), enforcement lives in the
//! transport controller (execution knowledge).

use std::collections::HashMap;

use crate::device::DeviceDefinition;
use crate::protocol::ProtocolFamily;
use crate::{cmd, hid};

/// Maximum payload bytes per command (HID report minus report ID and command byte).
pub const MAX_PAYLOAD_SIZE: usize = hid::REPORT_SIZE - 2; // 63

/// Expected wire-level payload shape for a specific command on a specific device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadSchema {
    /// No payload bytes allowed (typical for GET queries and reset commands).
    Empty,
    /// Exactly `n` bytes required.
    FixedSize(usize),
    /// Payload length must be within `[min, max]` inclusive.
    Range { min: usize, max: usize },
    /// A normalization transform is applied, then the result must be exactly
    /// `wire_size` bytes. This handles legacy callers that send incomplete
    /// payloads (e.g., debounce `[value]` → `[0x00, value]`).
    Normalized {
        wire_size: usize,
        normalizer: NormalizerFn,
    },
    /// Variable-length payload with only an upper bound.
    VariableWithMax(usize),
}

/// Named, deterministic normalization transforms.
///
/// Each variant is a pure function from input bytes to wire bytes. Using an
/// enum (not a function pointer) keeps the schema `Debug + Clone + PartialEq`
/// and makes transforms independently testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizerFn {
    /// YiChip debounce SET: prepend profile byte 0x00.
    ///
    /// - `[value]` (1 byte) → `[0x00, value]` (2 bytes)
    /// - `[0x00, value]` (already 2 bytes) → passthrough unchanged
    PrependProfileZero,
}

impl NormalizerFn {
    /// Apply the normalization transform.
    pub fn normalize(&self, data: &[u8]) -> Vec<u8> {
        match self {
            NormalizerFn::PrependProfileZero => {
                if data.len() == 1 {
                    vec![0x00, data[0]]
                } else {
                    data.to_vec()
                }
            }
        }
    }
}

/// Resolution result for a command byte against a device's command vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResolution {
    /// Command is in the device's family/override command table with a strict schema.
    Known(PayloadSchema),
    /// Command is a shared protocol constant (same byte across families) with
    /// a basic schema. Shared entries yield to device-specific entries on
    /// collision via `HashMap::entry().or_insert()`.
    Shared(PayloadSchema),
    /// Command byte is not recognized for this device.
    Unknown,
}

/// Per-device command schema map.
///
/// Built once at transport connect time from a [`DeviceDefinition`]. Maps every
/// recognized command byte to its [`CommandResolution`] (payload schema + provenance).
///
/// The transport controller holds this and calls [`resolve`](Self::resolve) for
/// every outbound command to normalize and validate payloads before framing.
#[derive(Debug, Clone)]
pub struct CommandSchemaMap {
    entries: HashMap<u8, CommandResolution>,
    protocol_family: ProtocolFamily,
    strict: bool,
}

impl CommandSchemaMap {
    /// Build a schema map for a specific device.
    ///
    /// Resolves the device's command table (with per-device overrides), registers
    /// device-specific commands as `Known`, then backfills shared protocol
    /// constants as `Shared`. Device-specific entries take precedence on collision.
    ///
    /// Starts in permissive mode (`strict: false`).
    pub fn for_device(device: &DeviceDefinition) -> Self {
        let family = device.protocol_family();
        let table = device.commands();
        let mut entries = HashMap::new();

        // ── Device-specific commands from resolved CommandTable ──────────
        //
        // The raw gRPC bridge always delivers 63-byte payloads (full HID
        // report minus the command byte) with zero-padding. VariableWithMax
        // is the only schema that accepts this without false rejections.
        // FixedSize/Empty/Range are reserved for future typed RPCs.
        //
        // SET_DEBOUNCE (YiChip) is the only command with active normalization.

        let known_var = CommandResolution::Known(PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE));

        entries.insert(table.set_reset, known_var.clone());
        entries.insert(table.set_profile, known_var.clone());

        // SET_DEBOUNCE: active normalization for legacy single-byte callers.
        // Padded bridge payloads (63 bytes) pass the >= wire_size check.
        match family {
            ProtocolFamily::YiChip => {
                entries.insert(
                    table.set_debounce,
                    CommandResolution::Known(PayloadSchema::Normalized {
                        wire_size: 2,
                        normalizer: NormalizerFn::PrependProfileZero,
                    }),
                );
            }
            ProtocolFamily::Ry5088 => {
                entries.insert(table.set_debounce, known_var.clone());
            }
        }

        entries.insert(table.set_keymatrix, known_var.clone());
        entries.insert(table.set_macro, known_var.clone());

        // GET commands: many carry query parameters (profile index, page number,
        // etc.) so we use VariableWithMax rather than Empty. Tighten per-command
        // once we've verified exact schemas against firmware behavior.
        entries.insert(
            table.get_profile,
            CommandResolution::Known(PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE)),
        );
        entries.insert(
            table.get_debounce,
            CommandResolution::Known(PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE)),
        );
        entries.insert(
            table.get_keymatrix,
            CommandResolution::Known(PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE)),
        );

        // Optional commands (only present for some families/devices)
        if let Some(c) = table.set_report {
            entries.insert(
                c,
                CommandResolution::Known(PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE)),
            );
        }
        if let Some(c) = table.set_kboption {
            entries.insert(
                c,
                CommandResolution::Known(PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE)),
            );
        }
        if let Some(c) = table.set_sleeptime {
            entries.insert(
                c,
                CommandResolution::Known(PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE)),
            );
        }
        if let Some(c) = table.get_report {
            entries.insert(
                c,
                CommandResolution::Known(PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE)),
            );
        }
        if let Some(c) = table.get_kboption {
            entries.insert(
                c,
                CommandResolution::Known(PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE)),
            );
        }
        if let Some(c) = table.get_sleeptime {
            entries.insert(
                c,
                CommandResolution::Known(PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE)),
            );
        }

        // ── Shared protocol commands (backfill, device-specific wins) ───

        register_shared_commands(&mut entries);

        Self {
            entries,
            protocol_family: family,
            strict: false,
        }
    }

    /// Schema map for pre-identity probe sessions.
    ///
    /// Only registers the commands needed for device identification
    /// (`GET_USB_VERSION`, `GET_REV`). Used by `CommandController::new()` when
    /// the device definition is not yet known.
    pub fn probe_only() -> Self {
        let mut entries = HashMap::new();
        entries.insert(
            cmd::GET_USB_VERSION,
            CommandResolution::Shared(PayloadSchema::Empty),
        );
        entries.insert(
            cmd::GET_REV,
            CommandResolution::Shared(PayloadSchema::Empty),
        );
        Self {
            entries,
            protocol_family: ProtocolFamily::default(),
            strict: false,
        }
    }

    /// Enable strict mode: unknown commands are rejected instead of warned.
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Whether unknown commands should be rejected (`true`) or warned (`false`).
    pub fn is_strict(&self) -> bool {
        self.strict
    }

    /// Resolve a command byte to its schema.
    pub fn resolve(&self, cmd: u8) -> &CommandResolution {
        self.entries
            .get(&cmd)
            .unwrap_or(&CommandResolution::Unknown)
    }

    /// The protocol family this schema map was built for.
    pub fn protocol_family(&self) -> ProtocolFamily {
        self.protocol_family
    }
}

/// Backfill shared protocol commands into the schema map.
///
/// Uses `entry().or_insert()` so that device-specific commands (already inserted)
/// take precedence on byte-level collisions. For example, YiChip `set_macro = 0x08`
/// wins over shared `SET_SLEDPARAM = 0x08`.
fn register_shared_commands(entries: &mut HashMap<u8, CommandResolution>) {
    let var_max = PayloadSchema::VariableWithMax(MAX_PAYLOAD_SIZE);

    // ── Shared SET commands ─────────────────────────────────────────────

    entries
        .entry(cmd::SET_LEDPARAM)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    entries
        .entry(cmd::SET_SLEDPARAM)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    entries
        .entry(cmd::SET_USERPIC)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    entries
        .entry(cmd::SET_AUDIO_VIZ)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    entries
        .entry(cmd::SET_SCREEN_COLOR)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    entries
        .entry(cmd::SET_FN)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    entries
        .entry(cmd::SET_USERGIF)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    entries
        .entry(cmd::SET_AUTOOS_EN)
        .or_insert_with(|| CommandResolution::Shared(PayloadSchema::FixedSize(1)));
    entries
        .entry(cmd::SET_MAGNETISM_REPORT)
        .or_insert_with(|| CommandResolution::Shared(PayloadSchema::FixedSize(1)));
    entries
        .entry(cmd::SET_MAGNETISM_CAL)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    entries
        .entry(cmd::SET_KEY_MAGNETISM_MODE)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    entries
        .entry(cmd::SET_MAGNETISM_MAX_CAL)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    entries
        .entry(cmd::SET_MULTI_MAGNETISM)
        .or_insert_with(|| CommandResolution::Shared(var_max.clone()));

    // ── Shared GET commands ────────────────────────────────────────────
    // Most GET commands can carry query parameters (profile index, page
    // number, etc.). Use VariableWithMax for all of them. Tighten
    // per-command once verified against firmware behavior.

    for &cmd_byte in &[
        cmd::GET_REV,
        cmd::GET_LEDONOFF,
        cmd::GET_LEDPARAM,
        cmd::GET_SLEDPARAM,
        cmd::GET_USB_VERSION,
        cmd::GET_FN,
        cmd::GET_AUTOOS_EN,
        cmd::GET_KEY_MAGNETISM_MODE,
        cmd::GET_OLED_VERSION,
        cmd::GET_MLED_VERSION,
        cmd::GET_FEATURE_LIST,
        cmd::GET_CALIBRATION,
        cmd::GET_USERPIC,
        cmd::GET_MACRO,
        cmd::GET_MULTI_MAGNETISM,
    ] {
        entries
            .entry(cmd_byte)
            .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    }

    // ── Dongle commands ─────────────────────────────────────────────────

    for &cmd_byte in &[
        cmd::GET_DONGLE_INFO,
        cmd::SET_CTRL_BYTE,
        cmd::GET_DONGLE_STATUS,
        cmd::ENTER_PAIRING,
        cmd::PAIRING_CMD,
        cmd::GET_PATCH_INFO,
        cmd::LED_STREAM,
        cmd::GET_RF_INFO,
        cmd::GET_CACHED_RESPONSE,
        cmd::GET_DONGLE_ID,
    ] {
        entries
            .entry(cmd_byte)
            .or_insert_with(|| CommandResolution::Shared(var_max.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── NormalizerFn tests ──────────────────────────────────────────────

    #[test]
    fn normalizer_prepend_profile_zero_single_byte() {
        let out = NormalizerFn::PrependProfileZero.normalize(&[5]);
        assert_eq!(out, vec![0x00, 5]);
    }

    #[test]
    fn normalizer_prepend_profile_zero_passthrough() {
        let out = NormalizerFn::PrependProfileZero.normalize(&[0x00, 5]);
        assert_eq!(out, vec![0x00, 5]);
    }

    #[test]
    fn normalizer_prepend_profile_zero_empty() {
        let out = NormalizerFn::PrependProfileZero.normalize(&[]);
        assert_eq!(out, Vec::<u8>::new());
    }

    #[test]
    fn normalizer_prepend_profile_zero_three_bytes() {
        let out = NormalizerFn::PrependProfileZero.normalize(&[0x00, 5, 0xFF]);
        assert_eq!(out, vec![0x00, 5, 0xFF]);
    }

    // ── CommandSchemaMap tests ──────────────────────────────────────────

    fn make_yichip_device() -> DeviceDefinition {
        serde_json::from_str(include_str!("../devices/m5w.json")).unwrap()
    }

    fn make_ry5088_device() -> DeviceDefinition {
        serde_json::from_str(include_str!("../devices/ry5088_fun60_nolight_1k_005.json")).unwrap()
    }

    #[test]
    fn schema_map_yichip_debounce_normalized() {
        let device = make_yichip_device();
        let map = CommandSchemaMap::for_device(&device);
        let table = device.commands();

        match map.resolve(table.set_debounce) {
            CommandResolution::Known(PayloadSchema::Normalized {
                wire_size,
                normalizer,
            }) => {
                assert_eq!(*wire_size, 2);
                assert_eq!(*normalizer, NormalizerFn::PrependProfileZero);
            }
            other => panic!("expected Known(Normalized), got {:?}", other),
        }
    }

    #[test]
    fn schema_map_ry5088_debounce_variable() {
        let device = make_ry5088_device();
        let map = CommandSchemaMap::for_device(&device);
        let table = device.commands();

        match map.resolve(table.set_debounce) {
            CommandResolution::Known(PayloadSchema::VariableWithMax(_)) => {}
            other => panic!("expected Known(VariableWithMax), got {:?}", other),
        }
    }

    #[test]
    fn schema_map_device_specific_wins_over_shared() {
        // M5W (YiChip) get_profile = 0x85, which collides with shared GET_LEDONOFF = 0x85.
        // The device-specific Known entry must win.
        let device = make_yichip_device();
        let map = CommandSchemaMap::for_device(&device);
        let table = device.commands();

        assert_eq!(table.get_profile, cmd::GET_LEDONOFF);
        match map.resolve(cmd::GET_LEDONOFF) {
            CommandResolution::Known(PayloadSchema::VariableWithMax(_)) => {}
            other => panic!(
                "expected Known(VariableWithMax) for get_profile, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn schema_map_unknown_command() {
        let device = make_yichip_device();
        let map = CommandSchemaMap::for_device(&device);
        assert_eq!(*map.resolve(0xFF), CommandResolution::Unknown);
    }

    #[test]
    fn schema_map_probe_only_allows_get_usb_version() {
        let map = CommandSchemaMap::probe_only();
        match map.resolve(cmd::GET_USB_VERSION) {
            CommandResolution::Shared(PayloadSchema::Empty) => {}
            other => panic!("expected Shared(Empty), got {:?}", other),
        }
    }

    #[test]
    fn schema_map_probe_only_rejects_other_commands() {
        let map = CommandSchemaMap::probe_only();
        assert_eq!(*map.resolve(0x11), CommandResolution::Unknown);
    }

    #[test]
    fn schema_map_strict_mode() {
        let map = CommandSchemaMap::probe_only().with_strict(true);
        assert!(map.is_strict());

        let map2 = CommandSchemaMap::probe_only();
        assert!(!map2.is_strict());
    }

    #[test]
    fn schema_map_m5w_integration() {
        let device = make_yichip_device();
        assert_eq!(device.protocol_family(), ProtocolFamily::YiChip);

        let table = device.commands();
        assert_eq!(table.set_debounce, 0x11);
        assert_eq!(table.get_debounce, 0x91);

        let map = CommandSchemaMap::for_device(&device);
        assert_eq!(map.protocol_family(), ProtocolFamily::YiChip);

        // Debounce SET: normalized
        match map.resolve(0x11) {
            CommandResolution::Known(PayloadSchema::Normalized { wire_size: 2, .. }) => {}
            other => panic!("debounce SET: expected Known(Normalized), got {:?}", other),
        }

        // Debounce GET: variable (may carry query parameters)
        match map.resolve(0x91) {
            CommandResolution::Known(PayloadSchema::VariableWithMax(_)) => {}
            other => panic!(
                "debounce GET: expected Known(VariableWithMax), got {:?}",
                other
            ),
        }

        // GET_USB_VERSION: shared variable
        match map.resolve(cmd::GET_USB_VERSION) {
            CommandResolution::Shared(PayloadSchema::VariableWithMax(_)) => {}
            CommandResolution::Known(PayloadSchema::VariableWithMax(_)) => {}
            other => panic!(
                "GET_USB_VERSION: expected Shared/Known(VariableWithMax), got {:?}",
                other
            ),
        }
    }

    #[test]
    fn schema_map_yichip_collisions_resolve_correctly() {
        let device = make_yichip_device();
        let map = CommandSchemaMap::for_device(&device);
        let table = device.commands();

        // 0x09 = YiChip set_keymatrix (not shared SET_KBOPTION)
        assert_eq!(table.set_keymatrix, 0x09);
        match map.resolve(0x09) {
            CommandResolution::Known(_) => {}
            other => panic!("0x09: expected Known (set_keymatrix), got {:?}", other),
        }

        // 0x85 = YiChip get_profile (not shared GET_LEDONOFF)
        assert_eq!(table.get_profile, 0x85);
        match map.resolve(0x85) {
            CommandResolution::Known(PayloadSchema::VariableWithMax(_)) => {}
            other => panic!(
                "0x85: expected Known(VariableWithMax) (get_profile), got {:?}",
                other
            ),
        }

        // 0x89 = YiChip get_keymatrix (not shared GET_KBOPTION)
        assert_eq!(table.get_keymatrix, 0x89);
        match map.resolve(0x89) {
            CommandResolution::Known(_) => {}
            other => panic!("0x89: expected Known (get_keymatrix), got {:?}", other),
        }
    }

    #[test]
    fn schema_map_max_payload_size_constant() {
        assert_eq!(MAX_PAYLOAD_SIZE, 63);
    }
}
