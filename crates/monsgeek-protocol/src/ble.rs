//! Bluetooth Low Energy protocol constants.

/// Vendor report ID for BLE HID.
pub const VENDOR_REPORT_ID: u8 = 0x06;
/// Marker byte for command/response channel.
pub const CMDRESP_MARKER: u8 = 0x55;
/// Marker byte for event channel.
pub const EVENT_MARKER: u8 = 0x66;
/// Buffer size for BLE reports (65 bytes + report ID).
pub const REPORT_SIZE: usize = 66;
/// Default command delay for BLE (higher than USB due to latency).
pub const DEFAULT_DELAY_MS: u64 = 150;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vendor_report_id() {
        assert_eq!(VENDOR_REPORT_ID, 0x06);
    }

    #[test]
    fn test_cmdresp_marker() {
        assert_eq!(CMDRESP_MARKER, 0x55);
    }

    #[test]
    fn test_report_size() {
        assert_eq!(REPORT_SIZE, 66);
    }
}
