//! Checksum algorithms and command buffer construction.
//!
//! The FEA protocol uses a simple 8-bit checksum: sum the first N payload bytes,
//! subtract from 255. The checksum position depends on the command type:
//! - Bit7: checksum at byte index 7 (covers bytes 0..7)
//! - Bit8: checksum at byte index 8 (covers bytes 0..8)
//! - None: no checksum

use serde::{Deserialize, Serialize};

/// Checksum configuration for commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ChecksumType {
    /// Sum bytes 0..7, store `255 - (sum & 0xFF)` at byte 7 (most commands).
    #[default]
    Bit7,
    /// Sum bytes 0..8, store `255 - (sum & 0xFF)` at byte 8 (LED commands).
    Bit8,
    /// No checksum.
    None,
}

/// Calculate the checksum value for a data slice.
///
/// Sums the first N bytes of `data` (where N is 7 for Bit7, 8 for Bit8),
/// then returns `255 - (sum & 0xFF)`. Returns 0 for `ChecksumType::None`.
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

/// Apply checksum to a mutable data buffer in-place.
///
/// Writes the calculated checksum at the appropriate index:
/// - Bit7: `data[7]`
/// - Bit8: `data[8]`
/// - None: no-op
pub fn apply_checksum(data: &mut [u8], checksum_type: ChecksumType) {
    match checksum_type {
        ChecksumType::Bit7 => {
            if data.len() >= 8 {
                data[7] = calculate_checksum(data, checksum_type);
            }
        }
        ChecksumType::Bit8 => {
            if data.len() >= 9 {
                data[8] = calculate_checksum(data, checksum_type);
            }
        }
        ChecksumType::None => {}
    }
}

/// Build a USB command buffer with checksum.
///
/// Format: `[report_id=0] [cmd] [data...] [checksum...]`
///
/// The checksum is applied to `buf[1..]` (the payload starting at the command
/// byte), NOT `buf[0..]` -- the report ID byte (always 0) is excluded from
/// the checksum calculation.
pub fn build_command(cmd: u8, data: &[u8], checksum_type: ChecksumType) -> Vec<u8> {
    let mut buf = vec![0u8; crate::hid::REPORT_SIZE];
    buf[0] = 0; // USB report ID
    buf[1] = cmd;
    let len = std::cmp::min(data.len(), crate::hid::REPORT_SIZE - 2);
    buf[2..2 + len].copy_from_slice(&data[..len]);
    apply_checksum(&mut buf[1..], checksum_type);
    buf
}

/// Build a BLE command buffer with checksum.
///
/// BLE uses a different framing than USB:
/// Format: `[report_id=0x06] [0x55 marker] [cmd] [data...] [checksum...]`
///
/// The checksum is applied to `buf[2..]` (starting at the cmd byte, skipping
/// the report ID and marker bytes).
pub fn build_ble_command(cmd: u8, data: &[u8], checksum_type: ChecksumType) -> Vec<u8> {
    let mut buf = vec![0u8; crate::ble::REPORT_SIZE];
    buf[0] = crate::ble::VENDOR_REPORT_ID; // Report ID 6 for BLE
    buf[1] = crate::ble::CMDRESP_MARKER; // 0x55 marker
    buf[2] = cmd;
    let len = std::cmp::min(data.len(), crate::ble::REPORT_SIZE - 3);
    buf[3..3 + len].copy_from_slice(&data[..len]);
    apply_checksum(&mut buf[2..], checksum_type);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_type_default() {
        assert_eq!(ChecksumType::default(), ChecksumType::Bit7);
    }

    #[test]
    fn test_checksum_bit7_single_byte() {
        // data = [0x8F, 0, 0, 0, 0, 0, 0, ...]
        // sum of first 7 = 0x8F = 143
        // 255 - 143 = 112 = 0x70
        let data = [0x8F, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(calculate_checksum(&data, ChecksumType::Bit7), 0x70);
    }

    #[test]
    fn test_checksum_bit7_multiple_bytes() {
        // data = [0x07, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, ...]
        // sum of first 7 = 7+1+2+3+4+5+6 = 28
        // 255 - 28 = 227
        let data = [0x07, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x00];
        assert_eq!(calculate_checksum(&data, ChecksumType::Bit7), 227);
    }

    #[test]
    fn test_checksum_bit8() {
        // data = [0x07, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x10, ...]
        // sum of first 8 = 7+1+2+3+4+5+6+16 = 44
        // 255 - 44 = 211
        let data = [0x07, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x10, 0x00];
        assert_eq!(calculate_checksum(&data, ChecksumType::Bit8), 211);
    }

    #[test]
    fn test_checksum_none() {
        let data = [0x07, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        assert_eq!(calculate_checksum(&data, ChecksumType::None), 0);
    }

    #[test]
    fn test_apply_checksum_bit7() {
        let mut data = [0x07, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x00, 0x00, 0x00];
        apply_checksum(&mut data, ChecksumType::Bit7);
        // sum of first 7 = 28, checksum = 255 - 28 = 227
        assert_eq!(data[7], 227);
    }

    #[test]
    fn test_apply_checksum_bit8() {
        let mut data = [0x07, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x10, 0x00, 0x00];
        apply_checksum(&mut data, ChecksumType::Bit8);
        // sum of first 8 = 44, checksum = 255 - 44 = 211
        assert_eq!(data[8], 211);
    }

    #[test]
    fn test_build_command_get_usb_version() {
        let buf = build_command(0x8F, &[], ChecksumType::Bit7);
        assert_eq!(buf.len(), 65);
        assert_eq!(buf[0], 0, "report ID should be 0");
        assert_eq!(buf[1], 0x8F, "command byte");
        // Checksum is on buf[1..]: [0x8F, 0, 0, 0, 0, 0, 0, checksum, ...]
        // sum of first 7 of buf[1..] = 0x8F = 143
        // checksum = 255 - 143 = 112 = 0x70
        assert_eq!(buf[8], 0x70, "checksum at buf[8] (buf[1..][7])");
    }

    #[test]
    fn test_build_command_with_data() {
        let buf = build_command(0x07, &[0x01, 0x02], ChecksumType::Bit7);
        assert_eq!(buf[0], 0, "report ID");
        assert_eq!(buf[1], 0x07, "command byte");
        assert_eq!(buf[2], 0x01, "data[0]");
        assert_eq!(buf[3], 0x02, "data[1]");
    }

    #[test]
    fn test_build_command_checksum_excludes_report_id() {
        // Build two commands: if report ID were included in checksum,
        // a non-zero report ID would change the checksum value.
        // The checksum should be based only on buf[1..], so buf[0] is irrelevant.
        let buf = build_command(0x8F, &[], ChecksumType::Bit7);
        // Manually calculate: payload starts at buf[1], checksum on [0x8F, 0,0,0,0,0,0]
        let expected_checksum = 255u8 - 0x8Fu8;
        assert_eq!(buf[8], expected_checksum);
        // Verify buf[0] is 0 (report ID) and was NOT included
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn test_build_ble_command() {
        let buf = build_ble_command(0x8F, &[], ChecksumType::Bit7);
        assert_eq!(buf.len(), 66);
        assert_eq!(buf[0], 0x06, "BLE report ID");
        assert_eq!(buf[1], 0x55, "BLE marker");
        assert_eq!(buf[2], 0x8F, "command byte");
        // Checksum is on buf[2..]: [0x8F, 0, 0, 0, 0, 0, 0, checksum, ...]
        // sum of first 7 of buf[2..] = 0x8F = 143
        // checksum = 255 - 143 = 112 = 0x70
        assert_eq!(buf[9], 0x70, "checksum at buf[9] (buf[2..][7])");
    }
}
