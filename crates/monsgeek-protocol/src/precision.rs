//! Firmware version thresholds for precision levels.
//!
//! These constants define firmware version boundaries for different
//! precision levels in travel/trigger settings.

/// Version threshold for fine precision (0.005mm steps).
/// Firmware versions >= 1280 (0x500) support fine precision.
pub const FINE_VERSION: u16 = 1280;
/// Version threshold for medium precision (0.01mm steps).
/// Firmware versions >= 768 (0x300) support medium precision.
pub const MEDIUM_VERSION: u16 = 768;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fine_version() {
        assert_eq!(FINE_VERSION, 1280);
    }

    #[test]
    fn test_medium_version() {
        assert_eq!(MEDIUM_VERSION, 768);
    }
}
