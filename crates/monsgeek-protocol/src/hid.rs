//! HID report sizes, usage pages, and interface numbers.

/// USB feature report size (64 bytes + 1 byte report ID).
pub const REPORT_SIZE: usize = 65;
/// USB input report size (64 bytes, no report ID prefix).
pub const INPUT_REPORT_SIZE: usize = 64;
/// HID usage page for vendor-defined (USB).
pub const USAGE_PAGE: u16 = 0xFFFF;
/// Alternative vendor usage page seen on some models.
pub const USAGE_PAGE_ALT: u16 = 0xFF00;
/// HID usage for feature interface (USB).
pub const USAGE_FEATURE: u16 = 0x02;
/// HID usage for input interface (USB).
pub const USAGE_INPUT: u16 = 0x01;
/// Feature interface number.
pub const INTERFACE_FEATURE: i32 = 2;
/// Input interface number.
pub const INTERFACE_INPUT: i32 = 1;

/// Check if a usage page is a vendor usage page (0xFFFF or 0xFF00).
#[inline]
pub fn is_vendor_usage_page(page: u16) -> bool {
    page == USAGE_PAGE || page == USAGE_PAGE_ALT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_size() {
        assert_eq!(REPORT_SIZE, 65);
    }

    #[test]
    fn test_input_report_size() {
        assert_eq!(INPUT_REPORT_SIZE, 64);
    }

    #[test]
    fn test_usage_page() {
        assert_eq!(USAGE_PAGE, 0xFFFF);
    }

    #[test]
    fn test_interface_feature() {
        assert_eq!(INTERFACE_FEATURE, 2);
    }

    #[test]
    fn test_is_vendor_usage_page_ffff() {
        assert!(is_vendor_usage_page(0xFFFF));
    }

    #[test]
    fn test_is_vendor_usage_page_ff00() {
        assert!(is_vendor_usage_page(0xFF00));
    }

    #[test]
    fn test_is_vendor_usage_page_standard() {
        assert!(!is_vendor_usage_page(0x0001));
    }
}
