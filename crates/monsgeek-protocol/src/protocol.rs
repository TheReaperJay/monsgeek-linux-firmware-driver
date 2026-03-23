//! Protocol family detection and family-specific command tables.
//!
//! MonsGeek/Akko keyboards use two MCU families that share the same transport
//! layer but differ in ~8 command byte assignments. [`ProtocolFamily::detect`]
//! identifies the family from the device name or PID, and [`CommandTable`]
//! provides the divergent command bytes.

use std::fmt;

/// Protocol family -- determines which command byte mapping to use.
///
/// Transport layer is identical between families (same checksums, same IF2
/// vendor HID, same bootloader). Only the command byte mapping differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProtocolFamily {
    #[default]
    Ry5088,
    YiChip,
}

/// Command byte mapping table for a protocol family.
///
/// Contains only the command bytes that differ between RY5088 and YiChip.
/// Shared commands (SET_LEDPARAM, GET_USB_VERSION, GET_MACRO, etc.) use
/// the constants in [`crate::cmd`] directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandTable {
    pub set_reset: u8,
    pub set_profile: u8,
    pub set_debounce: u8,
    pub set_keymatrix: u8,
    pub set_macro: u8,
    pub get_profile: u8,
    pub get_debounce: u8,
    pub get_keymatrix: u8,
    // RY5088-only commands (None on YiChip, except get_kboption)
    pub set_report: Option<u8>,
    pub set_kboption: Option<u8>,
    pub set_sleeptime: Option<u8>,
    pub get_report: Option<u8>,
    pub get_kboption: Option<u8>,
    pub get_sleeptime: Option<u8>,
}

pub static RY5088_COMMANDS: CommandTable = CommandTable {
    set_reset: 0x01,
    set_profile: 0x04,
    set_debounce: 0x06,
    set_keymatrix: 0x0A,
    set_macro: 0x0B,
    get_profile: 0x84,
    get_debounce: 0x86,
    get_keymatrix: 0x8A,
    set_report: Some(0x03),
    set_kboption: Some(0x09),
    set_sleeptime: Some(0x11),
    get_report: Some(0x83),
    get_kboption: Some(0x89),
    get_sleeptime: Some(0x91),
};

pub static YICHIP_COMMANDS: CommandTable = CommandTable {
    set_reset: 0x02,
    set_profile: 0x05,
    set_debounce: 0x11,
    set_keymatrix: 0x09,
    set_macro: 0x08,
    get_profile: 0x85,
    get_debounce: 0x91,
    get_keymatrix: 0x89,
    set_report: None,
    set_kboption: None,
    set_sleeptime: None,
    get_report: None,
    get_kboption: Some(0x86),
    get_sleeptime: None,
};

impl ProtocolFamily {
    /// Get the command table for this protocol family.
    pub fn commands(&self) -> &'static CommandTable {
        match self {
            ProtocolFamily::Ry5088 => &RY5088_COMMANDS,
            ProtocolFamily::YiChip => &YICHIP_COMMANDS,
        }
    }

    /// Detect protocol family from device name and PID.
    ///
    /// Detection priority:
    /// 1. Name prefix from device database (`ry5088_` / `ry1086_` -> RY5088,
    ///    `yc500_` / `yc300_` / `yc3121_` / `yc3123_` -> YiChip)
    /// 2. PID heuristic (0x40xx -> YiChip)
    /// 3. Default: RY5088
    pub fn detect(device_name: Option<&str>, pid: u16) -> Self {
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
        // PID heuristic: 0x40xx PIDs are YiChip-based
        if pid & 0xFF00 == 0x4000 {
            return ProtocolFamily::YiChip;
        }
        ProtocolFamily::Ry5088
    }
}

impl fmt::Display for ProtocolFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolFamily::Ry5088 => f.write_str("RY5088"),
            ProtocolFamily::YiChip => f.write_str("YiChip"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_yichip_by_name() {
        assert_eq!(
            ProtocolFamily::detect(Some("yc3121_m5w_soc"), 0x4015),
            ProtocolFamily::YiChip
        );
    }

    #[test]
    fn test_detect_yichip_by_name_yc500() {
        assert_eq!(
            ProtocolFamily::detect(Some("yc500_test"), 0x1234),
            ProtocolFamily::YiChip
        );
    }

    #[test]
    fn test_detect_yichip_by_name_yc300() {
        assert_eq!(
            ProtocolFamily::detect(Some("yc300_test"), 0x1234),
            ProtocolFamily::YiChip
        );
    }

    #[test]
    fn test_detect_yichip_by_name_yc3123() {
        assert_eq!(
            ProtocolFamily::detect(Some("yc3123_test"), 0x1234),
            ProtocolFamily::YiChip
        );
    }

    #[test]
    fn test_detect_ry5088_by_name() {
        assert_eq!(
            ProtocolFamily::detect(Some("ry5088_test"), 0x5030),
            ProtocolFamily::Ry5088
        );
    }

    #[test]
    fn test_detect_ry5088_by_name_ry1086() {
        // Name wins over PID: ry1086_ prefix -> RY5088 even with YiChip PID
        assert_eq!(
            ProtocolFamily::detect(Some("ry1086_test"), 0x4005),
            ProtocolFamily::Ry5088
        );
    }

    #[test]
    fn test_detect_yichip_by_pid() {
        // No name, 0x40xx PID -> YiChip
        assert_eq!(
            ProtocolFamily::detect(None, 0x4005),
            ProtocolFamily::YiChip
        );
    }

    #[test]
    fn test_detect_yichip_by_pid_4099() {
        assert_eq!(
            ProtocolFamily::detect(None, 0x4099),
            ProtocolFamily::YiChip
        );
    }

    #[test]
    fn test_detect_default_ry5088() {
        // No name, non-0x40xx PID -> default RY5088
        assert_eq!(
            ProtocolFamily::detect(None, 0x5030),
            ProtocolFamily::Ry5088
        );
    }

    #[test]
    fn test_detect_case_insensitive() {
        assert_eq!(
            ProtocolFamily::detect(Some("YC3121_M5W_SOC"), 0x1234),
            ProtocolFamily::YiChip
        );
    }

    #[test]
    fn test_detect_name_wins_over_pid() {
        // yc500_ name with non-YiChip PID -> still YiChip
        assert_eq!(
            ProtocolFamily::detect(Some("yc500_test"), 0x1234),
            ProtocolFamily::YiChip
        );
        // ry1086_ name with YiChip PID -> still RY5088
        assert_eq!(
            ProtocolFamily::detect(Some("ry1086_test"), 0x4005),
            ProtocolFamily::Ry5088
        );
    }

    #[test]
    fn test_yichip_command_table() {
        assert_eq!(YICHIP_COMMANDS.set_reset, 0x02);
        assert_eq!(YICHIP_COMMANDS.set_profile, 0x05);
        assert_eq!(YICHIP_COMMANDS.set_debounce, 0x11);
        assert_eq!(YICHIP_COMMANDS.set_keymatrix, 0x09);
        assert_eq!(YICHIP_COMMANDS.set_macro, 0x08);
        assert_eq!(YICHIP_COMMANDS.get_profile, 0x85);
        assert_eq!(YICHIP_COMMANDS.get_debounce, 0x91);
        assert_eq!(YICHIP_COMMANDS.get_keymatrix, 0x89);
        assert_eq!(YICHIP_COMMANDS.get_kboption, Some(0x86));
        assert_eq!(YICHIP_COMMANDS.set_report, None);
    }

    #[test]
    fn test_ry5088_command_table() {
        assert_eq!(RY5088_COMMANDS.set_reset, 0x01);
        assert_eq!(RY5088_COMMANDS.set_profile, 0x04);
        assert_eq!(RY5088_COMMANDS.set_debounce, 0x06);
        assert_eq!(RY5088_COMMANDS.set_keymatrix, 0x0A);
        assert_eq!(RY5088_COMMANDS.set_macro, 0x0B);
        assert_eq!(RY5088_COMMANDS.get_profile, 0x84);
        assert_eq!(RY5088_COMMANDS.set_report, Some(0x03));
        assert_eq!(RY5088_COMMANDS.get_report, Some(0x83));
    }

    #[test]
    fn test_commands_method() {
        assert_eq!(ProtocolFamily::YiChip.commands().set_reset, 0x02);
        assert_eq!(ProtocolFamily::Ry5088.commands().set_reset, 0x01);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", ProtocolFamily::Ry5088), "RY5088");
        assert_eq!(format!("{}", ProtocolFamily::YiChip), "YiChip");
    }
}
