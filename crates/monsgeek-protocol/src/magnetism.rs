//! Magnetism (Hall Effect trigger) sub-commands for GET/SET_MULTI_MAGNETISM.
//!
//! These identify which parameter is being read or written in a multi-magnetism
//! command payload.

/// Press travel (actuation point).
pub const PRESS_TRAVEL: u8 = 0x00;
/// Lift travel (release point).
pub const LIFT_TRAVEL: u8 = 0x01;
/// Rapid Trigger press sensitivity.
pub const RT_PRESS: u8 = 0x02;
/// Rapid Trigger lift sensitivity.
pub const RT_LIFT: u8 = 0x03;
/// DKS (Dynamic Keystroke) travel.
pub const DKS_TRAVEL: u8 = 0x04;
/// Mod-Tap activation time.
pub const MODTAP_TIME: u8 = 0x05;
/// Bottom deadzone.
pub const BOTTOM_DEADZONE: u8 = 0x06;
/// Key mode (Normal, RT, DKS, etc.).
pub const KEY_MODE: u8 = 0x07;
/// Snap Tap anti-SOCD enable.
pub const SNAPTAP_ENABLE: u8 = 0x09;
/// DKS trigger modes/actions.
pub const DKS_MODES: u8 = 0x0A;
/// Top deadzone (firmware >= 1024).
pub const TOP_DEADZONE: u8 = 0xFB;
/// Switch type (if replaceable).
pub const SWITCH_TYPE: u8 = 0xFC;
/// Raw sensor calibration values.
pub const CALIBRATION: u8 = 0xFE;

/// Get human-readable name for a magnetism sub-command byte.
pub fn name(subcmd: u8) -> &'static str {
    match subcmd {
        PRESS_TRAVEL => "PRESS_TRAVEL",
        LIFT_TRAVEL => "LIFT_TRAVEL",
        RT_PRESS => "RT_PRESS",
        RT_LIFT => "RT_LIFT",
        DKS_TRAVEL => "DKS_TRAVEL",
        MODTAP_TIME => "MODTAP_TIME",
        BOTTOM_DEADZONE => "BOTTOM_DEADZONE",
        KEY_MODE => "KEY_MODE",
        SNAPTAP_ENABLE => "SNAPTAP_ENABLE",
        DKS_MODES => "DKS_MODES",
        TOP_DEADZONE => "TOP_DEADZONE",
        SWITCH_TYPE => "SWITCH_TYPE",
        CALIBRATION => "CALIBRATION",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_press_travel() {
        assert_eq!(PRESS_TRAVEL, 0x00);
    }

    #[test]
    fn test_rt_press() {
        assert_eq!(RT_PRESS, 0x02);
    }

    #[test]
    fn test_key_mode() {
        assert_eq!(KEY_MODE, 0x07);
    }

    #[test]
    fn test_calibration() {
        assert_eq!(CALIBRATION, 0xFE);
    }

    #[test]
    fn test_name_known() {
        assert_eq!(name(0x00), "PRESS_TRAVEL");
    }

    #[test]
    fn test_name_unknown() {
        assert_eq!(name(0xFF), "UNKNOWN");
    }
}
