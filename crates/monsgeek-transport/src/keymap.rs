use crate::keycodes;

/// HID scancode to Linux keycode translation table.
/// Index by HID Usage ID (0x00-0xFF), value is Linux keycode.
/// 0 = unmapped (KEY_RESERVED, never a valid keycode).
///
/// Ported from monsgeek_hid.py HID_TO_LINUX (0x04-0x67, hardware-verified)
/// and extended with F13-F24 (0x68-0x73) from HID Usage Tables.
pub const HID_TO_LINUX: [u16; 256] = {
    let mut t = [0u16; 256];

    // Letters (HID 0x04-0x1D)
    t[0x04] = keycodes::KEY_A;
    t[0x05] = keycodes::KEY_B;
    t[0x06] = keycodes::KEY_C;
    t[0x07] = keycodes::KEY_D;
    t[0x08] = keycodes::KEY_E;
    t[0x09] = keycodes::KEY_F;
    t[0x0A] = keycodes::KEY_G;
    t[0x0B] = keycodes::KEY_H;
    t[0x0C] = keycodes::KEY_I;
    t[0x0D] = keycodes::KEY_J;
    t[0x0E] = keycodes::KEY_K;
    t[0x0F] = keycodes::KEY_L;
    t[0x10] = keycodes::KEY_M;
    t[0x11] = keycodes::KEY_N;
    t[0x12] = keycodes::KEY_O;
    t[0x13] = keycodes::KEY_P;
    t[0x14] = keycodes::KEY_Q;
    t[0x15] = keycodes::KEY_R;
    t[0x16] = keycodes::KEY_S;
    t[0x17] = keycodes::KEY_T;
    t[0x18] = keycodes::KEY_U;
    t[0x19] = keycodes::KEY_V;
    t[0x1A] = keycodes::KEY_W;
    t[0x1B] = keycodes::KEY_X;
    t[0x1C] = keycodes::KEY_Y;
    t[0x1D] = keycodes::KEY_Z;

    // Digits (HID 0x1E-0x27)
    t[0x1E] = keycodes::KEY_1;
    t[0x1F] = keycodes::KEY_2;
    t[0x20] = keycodes::KEY_3;
    t[0x21] = keycodes::KEY_4;
    t[0x22] = keycodes::KEY_5;
    t[0x23] = keycodes::KEY_6;
    t[0x24] = keycodes::KEY_7;
    t[0x25] = keycodes::KEY_8;
    t[0x26] = keycodes::KEY_9;
    t[0x27] = keycodes::KEY_0;

    // Special keys (HID 0x28-0x38)
    t[0x28] = keycodes::KEY_ENTER;
    t[0x29] = keycodes::KEY_ESC;
    t[0x2A] = keycodes::KEY_BACKSPACE;
    t[0x2B] = keycodes::KEY_TAB;
    t[0x2C] = keycodes::KEY_SPACE;
    t[0x2D] = keycodes::KEY_MINUS;
    t[0x2E] = keycodes::KEY_EQUAL;
    t[0x2F] = keycodes::KEY_LEFTBRACE;
    t[0x30] = keycodes::KEY_RIGHTBRACE;
    t[0x31] = keycodes::KEY_BACKSLASH;
    t[0x32] = keycodes::KEY_BACKSLASH; // non-US # (same as backslash per Python driver)
    t[0x33] = keycodes::KEY_SEMICOLON;
    t[0x34] = keycodes::KEY_APOSTROPHE;
    t[0x35] = keycodes::KEY_GRAVE;
    t[0x36] = keycodes::KEY_COMMA;
    t[0x37] = keycodes::KEY_DOT;
    t[0x38] = keycodes::KEY_SLASH;

    // Caps Lock
    t[0x39] = keycodes::KEY_CAPSLOCK;

    // F-keys F1-F12 (HID 0x3A-0x45)
    t[0x3A] = keycodes::KEY_F1;
    t[0x3B] = keycodes::KEY_F2;
    t[0x3C] = keycodes::KEY_F3;
    t[0x3D] = keycodes::KEY_F4;
    t[0x3E] = keycodes::KEY_F5;
    t[0x3F] = keycodes::KEY_F6;
    t[0x40] = keycodes::KEY_F7;
    t[0x41] = keycodes::KEY_F8;
    t[0x42] = keycodes::KEY_F9;
    t[0x43] = keycodes::KEY_F10;
    t[0x44] = keycodes::KEY_F11;
    t[0x45] = keycodes::KEY_F12;

    // Navigation (HID 0x46-0x52)
    t[0x46] = keycodes::KEY_SYSRQ;
    t[0x47] = keycodes::KEY_SCROLLLOCK;
    t[0x48] = keycodes::KEY_PAUSE;
    t[0x49] = keycodes::KEY_INSERT;
    t[0x4A] = keycodes::KEY_HOME;
    t[0x4B] = keycodes::KEY_PAGEUP;
    t[0x4C] = keycodes::KEY_DELETE;
    t[0x4D] = keycodes::KEY_END;
    t[0x4E] = keycodes::KEY_PAGEDOWN;
    t[0x4F] = keycodes::KEY_RIGHT;
    t[0x50] = keycodes::KEY_LEFT;
    t[0x51] = keycodes::KEY_DOWN;
    t[0x52] = keycodes::KEY_UP;

    // Keypad (HID 0x53-0x63)
    t[0x53] = keycodes::KEY_NUMLOCK;
    t[0x54] = keycodes::KEY_KPSLASH;
    t[0x55] = keycodes::KEY_KPASTERISK;
    t[0x56] = keycodes::KEY_KPMINUS;
    t[0x57] = keycodes::KEY_KPPLUS;
    t[0x58] = keycodes::KEY_KPENTER;
    t[0x59] = keycodes::KEY_KP1;
    t[0x5A] = keycodes::KEY_KP2;
    t[0x5B] = keycodes::KEY_KP3;
    t[0x5C] = keycodes::KEY_KP4;
    t[0x5D] = keycodes::KEY_KP5;
    t[0x5E] = keycodes::KEY_KP6;
    t[0x5F] = keycodes::KEY_KP7;
    t[0x60] = keycodes::KEY_KP8;
    t[0x61] = keycodes::KEY_KP9;
    t[0x62] = keycodes::KEY_KP0;
    t[0x63] = keycodes::KEY_KPDOT;

    // Misc (HID 0x64-0x67)
    t[0x64] = keycodes::KEY_102ND;
    t[0x65] = keycodes::KEY_COMPOSE;
    t[0x66] = keycodes::KEY_POWER;
    t[0x67] = keycodes::KEY_KPEQUAL;

    // Extended F-keys F13-F24 (HID 0x68-0x73)
    t[0x68] = keycodes::KEY_F13;
    t[0x69] = keycodes::KEY_F14;
    t[0x6A] = keycodes::KEY_F15;
    t[0x6B] = keycodes::KEY_F16;
    t[0x6C] = keycodes::KEY_F17;
    t[0x6D] = keycodes::KEY_F18;
    t[0x6E] = keycodes::KEY_F19;
    t[0x6F] = keycodes::KEY_F20;
    t[0x70] = keycodes::KEY_F21;
    t[0x71] = keycodes::KEY_F22;
    t[0x72] = keycodes::KEY_F23;
    t[0x73] = keycodes::KEY_F24;

    t
};

/// Modifier bit position to Linux keycode.
/// Index 0 = Left Ctrl (bit 0 of HID modifier byte), etc.
/// Matches monsgeek_hid.py MODIFIER_MAP (hardware-verified).
pub const MODIFIER_KEYCODES: [u16; 8] = [
    keycodes::KEY_LEFTCTRL,   // Bit 0
    keycodes::KEY_LEFTSHIFT,  // Bit 1
    keycodes::KEY_LEFTALT,    // Bit 2
    keycodes::KEY_LEFTMETA,   // Bit 3
    keycodes::KEY_RIGHTCTRL,  // Bit 4
    keycodes::KEY_RIGHTSHIFT, // Bit 5
    keycodes::KEY_RIGHTALT,   // Bit 6
    keycodes::KEY_RIGHTMETA,  // Bit 7
];

/// Yields all unique non-zero keycodes from HID_TO_LINUX and MODIFIER_KEYCODES.
/// Phase 2 uses this to register keys with uinput.
pub fn all_keycodes() -> impl Iterator<Item = u16> {
    let mut seen = [false; 256];
    let mut result = Vec::new();

    for &kc in &HID_TO_LINUX {
        if kc != 0 && !seen[kc as usize] {
            seen[kc as usize] = true;
            result.push(kc);
        }
    }

    for &kc in &MODIFIER_KEYCODES {
        if !seen[kc as usize] {
            seen[kc as usize] = true;
            result.push(kc);
        }
    }

    result.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_all_python_keys() {
        // All 100 entries from monsgeek_hid.py HID_TO_LINUX dict (0x04-0x67)
        let expected: &[(usize, u16)] = &[
            (0x04, 30),  // A
            (0x05, 48),  // B
            (0x06, 46),  // C
            (0x07, 32),  // D
            (0x08, 18),  // E
            (0x09, 33),  // F
            (0x0A, 34),  // G
            (0x0B, 35),  // H
            (0x0C, 23),  // I
            (0x0D, 36),  // J
            (0x0E, 37),  // K
            (0x0F, 38),  // L
            (0x10, 50),  // M
            (0x11, 49),  // N
            (0x12, 24),  // O
            (0x13, 25),  // P
            (0x14, 16),  // Q
            (0x15, 19),  // R
            (0x16, 31),  // S
            (0x17, 20),  // T
            (0x18, 22),  // U
            (0x19, 47),  // V
            (0x1A, 17),  // W
            (0x1B, 45),  // X
            (0x1C, 21),  // Y
            (0x1D, 44),  // Z
            (0x1E, 2),   // 1
            (0x1F, 3),   // 2
            (0x20, 4),   // 3
            (0x21, 5),   // 4
            (0x22, 6),   // 5
            (0x23, 7),   // 6
            (0x24, 8),   // 7
            (0x25, 9),   // 8
            (0x26, 10),  // 9
            (0x27, 11),  // 0
            (0x28, 28),  // Enter
            (0x29, 1),   // Escape
            (0x2A, 14),  // Backspace
            (0x2B, 15),  // Tab
            (0x2C, 57),  // Space
            (0x2D, 12),  // -
            (0x2E, 13),  // =
            (0x2F, 26),  // [
            (0x30, 27),  // ]
            (0x31, 43),  // backslash
            (0x32, 43),  // non-US # (same as backslash)
            (0x33, 39),  // ;
            (0x34, 40),  // '
            (0x35, 41),  // `
            (0x36, 51),  // ,
            (0x37, 52),  // .
            (0x38, 53),  // /
            (0x39, 58),  // Caps Lock
            (0x3A, 59),  // F1
            (0x3B, 60),  // F2
            (0x3C, 61),  // F3
            (0x3D, 62),  // F4
            (0x3E, 63),  // F5
            (0x3F, 64),  // F6
            (0x40, 65),  // F7
            (0x41, 66),  // F8
            (0x42, 67),  // F9
            (0x43, 68),  // F10
            (0x44, 87),  // F11
            (0x45, 88),  // F12
            (0x46, 99),  // Print Screen
            (0x47, 70),  // Scroll Lock
            (0x48, 119), // Pause
            (0x49, 110), // Insert
            (0x4A, 102), // Home
            (0x4B, 104), // Page Up
            (0x4C, 111), // Delete
            (0x4D, 107), // End
            (0x4E, 109), // Page Down
            (0x4F, 106), // Right
            (0x50, 105), // Left
            (0x51, 108), // Down
            (0x52, 103), // Up
            (0x53, 69),  // Num Lock
            (0x54, 98),  // Keypad /
            (0x55, 55),  // Keypad *
            (0x56, 74),  // Keypad -
            (0x57, 78),  // Keypad +
            (0x58, 96),  // Keypad Enter
            (0x59, 79),  // Keypad 1
            (0x5A, 80),  // Keypad 2
            (0x5B, 81),  // Keypad 3
            (0x5C, 75),  // Keypad 4
            (0x5D, 76),  // Keypad 5
            (0x5E, 77),  // Keypad 6
            (0x5F, 71),  // Keypad 7
            (0x60, 72),  // Keypad 8
            (0x61, 73),  // Keypad 9
            (0x62, 82),  // Keypad 0
            (0x63, 83),  // Keypad .
            (0x64, 86),  // non-US backslash
            (0x65, 127), // Compose
            (0x66, 116), // Power
            (0x67, 117), // Keypad =
        ];

        for &(hid, linux) in expected {
            assert_eq!(
                HID_TO_LINUX[hid], linux,
                "HID 0x{hid:02X}: expected Linux keycode {linux}, got {}",
                HID_TO_LINUX[hid]
            );
        }
    }

    #[test]
    fn test_extended_f_keys() {
        // HID 0x68-0x73 map to F13-F24 (Linux keycodes 183-194)
        for (i, hid) in (0x68..=0x73_usize).enumerate() {
            let expected = 183 + i as u16;
            assert_eq!(
                HID_TO_LINUX[hid],
                expected,
                "HID 0x{hid:02X}: expected {expected} (F{}), got {}",
                13 + i,
                HID_TO_LINUX[hid]
            );
        }
    }

    #[test]
    fn test_modifiers() {
        let expected: [(usize, u16); 8] = [
            (0, 29),  // Left Ctrl
            (1, 42),  // Left Shift
            (2, 56),  // Left Alt
            (3, 125), // Left Meta
            (4, 97),  // Right Ctrl
            (5, 54),  // Right Shift
            (6, 100), // Right Alt
            (7, 126), // Right Meta
        ];

        for (bit, keycode) in expected {
            assert_eq!(
                MODIFIER_KEYCODES[bit], keycode,
                "Modifier bit {bit}: expected {keycode}, got {}",
                MODIFIER_KEYCODES[bit]
            );
        }
    }

    #[test]
    fn test_unmapped() {
        assert_eq!(HID_TO_LINUX[0x00], 0, "HID 0x00 should be unmapped");
        assert_eq!(HID_TO_LINUX[0x01], 0, "HID 0x01 should be unmapped");
        assert_eq!(HID_TO_LINUX[0x02], 0, "HID 0x02 should be unmapped");
        assert_eq!(HID_TO_LINUX[0x03], 0, "HID 0x03 should be unmapped");
        assert_eq!(HID_TO_LINUX[0x74], 0, "HID 0x74 should be unmapped");
        assert_eq!(HID_TO_LINUX[0xFF], 0, "HID 0xFF should be unmapped");
    }

    #[test]
    fn test_all_keycodes_count() {
        let keycodes: HashSet<u16> = all_keycodes().collect();
        assert!(
            keycodes.len() >= 108,
            "Expected at least 108 unique keycodes, got {}",
            keycodes.len()
        );
    }

    #[test]
    fn test_all_keycodes_contains_modifiers() {
        let keycodes: HashSet<u16> = all_keycodes().collect();
        let modifier_values: [u16; 8] = [29, 42, 56, 125, 97, 54, 100, 126];
        for &kc in &modifier_values {
            assert!(
                keycodes.contains(&kc),
                "all_keycodes() missing modifier keycode {kc}"
            );
        }
    }

    #[test]
    fn test_no_duplicates_in_all_keycodes() {
        let vec: Vec<u16> = all_keycodes().collect();
        let set: HashSet<u16> = vec.iter().copied().collect();
        assert_eq!(
            vec.len(),
            set.len(),
            "all_keycodes() contains duplicates: {} items but {} unique",
            vec.len(),
            set.len()
        );
    }
}
