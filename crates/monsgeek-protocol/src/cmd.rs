//! HID Protocol Commands (FEA_CMD_*)
//!
//! Byte values transcribed verbatim from firmware reverse engineering.
//! These constants are shared across all protocol families -- only the
//! divergent commands live in [`crate::protocol::CommandTable`].

// SET commands (0x01 - 0x65)
pub const SET_RESET: u8 = 0x01;
pub const SET_REPORT: u8 = 0x03;
pub const SET_PROFILE: u8 = 0x04;
pub const SET_DEBOUNCE: u8 = 0x06;
pub const SET_LEDPARAM: u8 = 0x07;
pub const SET_SLEDPARAM: u8 = 0x08;
pub const SET_KBOPTION: u8 = 0x09;
pub const SET_KEYMATRIX: u8 = 0x0A;
pub const SET_MACRO: u8 = 0x0B;
pub const SET_USERPIC: u8 = 0x0C;
pub const SET_AUDIO_VIZ: u8 = 0x0D;
pub const SET_SCREEN_COLOR: u8 = 0x0E;
pub const SET_FN: u8 = 0x10;
pub const SET_SLEEPTIME: u8 = 0x11;
pub const SET_USERGIF: u8 = 0x12;
pub const SET_AUTOOS_EN: u8 = 0x17;
pub const SET_MAGNETISM_REPORT: u8 = 0x1B;
pub const SET_MAGNETISM_CAL: u8 = 0x1C;
pub const SET_KEY_MAGNETISM_MODE: u8 = 0x1D;
pub const SET_MAGNETISM_MAX_CAL: u8 = 0x1E;
pub const SET_MULTI_MAGNETISM: u8 = 0x65;

// GET commands (0x80 - 0xFE)
pub const GET_REV: u8 = 0x80;
pub const GET_REPORT: u8 = 0x83;
pub const GET_PROFILE: u8 = 0x84;
pub const GET_LEDONOFF: u8 = 0x85;
pub const GET_DEBOUNCE: u8 = 0x86;
pub const GET_LEDPARAM: u8 = 0x87;
pub const GET_SLEDPARAM: u8 = 0x88;
pub const GET_KBOPTION: u8 = 0x89;
pub const GET_KEYMATRIX: u8 = 0x8A;
pub const GET_MACRO: u8 = 0x8B;
pub const GET_USERPIC: u8 = 0x8C;
pub const GET_USB_VERSION: u8 = 0x8F;
pub const GET_FN: u8 = 0x90;
pub const GET_SLEEPTIME: u8 = 0x91;
pub const GET_AUTOOS_EN: u8 = 0x97;
pub const GET_KEY_MAGNETISM_MODE: u8 = 0x9D;
pub const GET_OLED_VERSION: u8 = 0xAD;
pub const GET_MLED_VERSION: u8 = 0xAE;
pub const GET_MULTI_MAGNETISM: u8 = 0xE5;
pub const GET_FEATURE_LIST: u8 = 0xE6;
pub const GET_CALIBRATION: u8 = 0xFE;

// Dongle-specific commands
/// Get dongle info: returns {0xF0, 1, 8, 0,0,0,0, fw_ver}.
pub const GET_DONGLE_INFO: u8 = 0xF0;
/// Set control byte: stores data[0] -> dongle_state.ctrl_byte.
pub const SET_CTRL_BYTE: u8 = 0xF6;
/// Get dongle status (9-byte response): has_response, kb_battery_info, 0,
/// kb_charging, 1, rf_ready, 1, pairing_mode, pairing_status.
/// Handled locally by dongle -- NOT forwarded to keyboard.
pub const GET_DONGLE_STATUS: u8 = 0xF7;
/// Enter pairing mode: requires 55AA55AA magic.
pub const ENTER_PAIRING: u8 = 0xF8;
/// Pairing control: sends 3-byte SPI packet {cmd=1, data[0], data[1]}.
pub const PAIRING_CMD: u8 = 0x7A;
/// Patch info -- custom firmware capabilities (battery HID, LED stream, etc.).
pub const GET_PATCH_INFO: u8 = 0xE7;
/// LED streaming -- write RGB data to WS2812 frame buffer via patch.
/// Sub-commands: page 0-6 = data, 0xFF = commit, 0xFE = release.
pub const LED_STREAM: u8 = 0xE8;
/// Get RF info: returns {rf_addr[4], fw_ver_minor, fw_ver_major, 0, 0}.
/// Handled locally by dongle -- NOT forwarded to keyboard.
pub const GET_RF_INFO: u8 = 0xFB;
/// Get cached keyboard response: copies 64B cached_kb_response into the
/// USB feature report buffer and clears has_response. Used as flush.
pub const GET_CACHED_RESPONSE: u8 = 0xFC;
/// Get dongle ID: returns {0xAA, 0x55, 0x01, 0x00}.
pub const GET_DONGLE_ID: u8 = 0xFD;
/// Set response size on dongle (dongle-local, NOT forwarded).
/// Same byte as GET_CALIBRATION (0xFE) on keyboard.
pub const SET_RESPONSE_SIZE: u8 = 0xFE;

// Response status
pub const STATUS_SUCCESS: u8 = 0xAA;

/// Get human-readable name for a command byte.
pub fn name(cmd: u8) -> &'static str {
    match cmd {
        SET_RESET => "SET_RESET",
        SET_REPORT => "SET_REPORT",
        SET_PROFILE => "SET_PROFILE",
        SET_DEBOUNCE => "SET_DEBOUNCE",
        SET_LEDPARAM => "SET_LEDPARAM",
        SET_SLEDPARAM => "SET_SLEDPARAM",
        SET_KBOPTION => "SET_KBOPTION",
        SET_KEYMATRIX => "SET_KEYMATRIX",
        SET_MACRO => "SET_MACRO",
        SET_USERPIC => "SET_USERPIC",
        SET_AUDIO_VIZ => "SET_AUDIO_VIZ",
        SET_SCREEN_COLOR => "SET_SCREEN_COLOR",
        SET_USERGIF => "SET_USERGIF",
        SET_FN => "SET_FN",
        SET_SLEEPTIME => "SET_SLEEPTIME",
        SET_AUTOOS_EN => "SET_AUTOOS_EN",
        SET_MAGNETISM_REPORT => "SET_MAGNETISM_REPORT",
        SET_MAGNETISM_CAL => "SET_MAGNETISM_CAL",
        SET_MAGNETISM_MAX_CAL => "SET_MAGNETISM_MAX_CAL",
        SET_KEY_MAGNETISM_MODE => "SET_KEY_MAGNETISM_MODE",
        SET_MULTI_MAGNETISM => "SET_MULTI_MAGNETISM",
        GET_REV => "GET_REV",
        GET_REPORT => "GET_REPORT",
        GET_PROFILE => "GET_PROFILE",
        GET_LEDONOFF => "GET_LEDONOFF",
        GET_DEBOUNCE => "GET_DEBOUNCE",
        GET_LEDPARAM => "GET_LEDPARAM",
        GET_SLEDPARAM => "GET_SLEDPARAM",
        GET_KBOPTION => "GET_KBOPTION",
        GET_USERPIC => "GET_USERPIC",
        GET_KEYMATRIX => "GET_KEYMATRIX",
        GET_MACRO => "GET_MACRO",
        GET_USB_VERSION => "GET_USB_VERSION",
        GET_FN => "GET_FN",
        GET_SLEEPTIME => "GET_SLEEPTIME",
        GET_AUTOOS_EN => "GET_AUTOOS_EN",
        GET_KEY_MAGNETISM_MODE => "GET_KEY_MAGNETISM_MODE",
        GET_OLED_VERSION => "GET_OLED_VERSION",
        GET_MLED_VERSION => "GET_MLED_VERSION",
        GET_MULTI_MAGNETISM => "GET_MULTI_MAGNETISM",
        GET_FEATURE_LIST => "GET_FEATURE_LIST",
        GET_CALIBRATION => "GET_CALIBRATION",
        GET_DONGLE_INFO => "GET_DONGLE_INFO",
        SET_CTRL_BYTE => "SET_CTRL_BYTE",
        GET_DONGLE_STATUS => "GET_DONGLE_STATUS",
        ENTER_PAIRING => "ENTER_PAIRING",
        PAIRING_CMD => "PAIRING_CMD",
        GET_PATCH_INFO => "GET_PATCH_INFO",
        LED_STREAM => "LED_STREAM",
        GET_RF_INFO => "GET_RF_INFO",
        GET_CACHED_RESPONSE => "GET_CACHED_RESPONSE",
        GET_DONGLE_ID => "GET_DONGLE_ID",
        STATUS_SUCCESS => "STATUS_SUCCESS",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_ledparam() {
        assert_eq!(SET_LEDPARAM, 0x07);
    }

    #[test]
    fn test_get_usb_version() {
        assert_eq!(GET_USB_VERSION, 0x8F);
    }

    #[test]
    fn test_get_dongle_status() {
        assert_eq!(GET_DONGLE_STATUS, 0xF7);
    }

    #[test]
    fn test_status_success() {
        assert_eq!(STATUS_SUCCESS, 0xAA);
    }

    #[test]
    fn test_set_multi_magnetism() {
        assert_eq!(SET_MULTI_MAGNETISM, 0x65);
    }

    #[test]
    fn test_get_multi_magnetism() {
        assert_eq!(GET_MULTI_MAGNETISM, 0xE5);
    }

    #[test]
    fn test_name_known_command() {
        assert_eq!(name(0x07), "SET_LEDPARAM");
    }

    #[test]
    fn test_name_unknown_command() {
        assert_eq!(name(0xFF), "UNKNOWN");
    }
}
