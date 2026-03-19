//! Key matrix bounds validation.
//!
//! Validates key indices and layer numbers against device bounds before any
//! USB write operation. This prevents firmware out-of-bounds memory corruption
//! on the yc3121.

use crate::error::TransportError;
use monsgeek_protocol::DeviceDefinition;

/// Validate a key index and layer against device bounds.
///
/// Returns `Ok(())` if `key_index < max_keys` AND `layer < max_layers`.
/// Returns `Err(BoundsViolation)` otherwise.
///
/// This MUST be called before any SET_KEYMATRIX write to prevent
/// firmware out-of-bounds memory corruption.
pub fn validate_key_index(
    key_index: u16,
    max_keys: u16,
    layer: u8,
    max_layers: u8,
) -> Result<(), TransportError> {
    if key_index >= max_keys || layer >= max_layers {
        return Err(TransportError::BoundsViolation {
            key_index,
            max_keys,
            layer,
            max_layers,
        });
    }
    Ok(())
}

/// Validate a key matrix write request against a [`DeviceDefinition`].
///
/// Extracts `key_count` and `layer` from the device definition and delegates
/// to [`validate_key_index`]. Returns `Err(BoundsViolation)` if the device
/// definition is missing `key_count` or `layer` fields -- defensive rejection
/// prevents writes to unknown layouts.
pub fn validate_write_request(
    device: &DeviceDefinition,
    key_index: u16,
    layer: u8,
) -> Result<(), TransportError> {
    let max_keys = device
        .key_count
        .ok_or(TransportError::BoundsViolation {
            key_index,
            max_keys: 0,
            layer,
            max_layers: 0,
        })? as u16;
    let max_layers = device.layer.ok_or(TransportError::BoundsViolation {
        key_index,
        max_keys,
        layer,
        max_layers: 0,
    })?;
    validate_key_index(key_index, max_keys, layer, max_layers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_key_index_zero_valid() {
        assert!(validate_key_index(0, 108, 0, 4).is_ok());
    }

    #[test]
    fn test_validate_key_index_max_valid() {
        // key_index 107 is the last valid index for max_keys=108
        // layer 3 is the last valid layer for max_layers=4
        assert!(validate_key_index(107, 108, 3, 4).is_ok());
    }

    #[test]
    fn test_validate_key_index_at_boundary_fails() {
        // key_index == max_keys should fail (0-indexed, so 108 is out of bounds for max 108)
        let result = validate_key_index(108, 108, 0, 4);
        assert!(result.is_err());
        match result.unwrap_err() {
            TransportError::BoundsViolation {
                key_index,
                max_keys,
                ..
            } => {
                assert_eq!(key_index, 108);
                assert_eq!(max_keys, 108);
            }
            other => panic!("expected BoundsViolation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_key_index_layer_at_boundary_fails() {
        // layer == max_layers should fail
        let result = validate_key_index(0, 108, 4, 4);
        assert!(result.is_err());
        match result.unwrap_err() {
            TransportError::BoundsViolation {
                layer, max_layers, ..
            } => {
                assert_eq!(layer, 4);
                assert_eq!(max_layers, 4);
            }
            other => panic!("expected BoundsViolation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_key_index_both_exceed() {
        let result = validate_key_index(200, 108, 5, 4);
        assert!(result.is_err());
        match result.unwrap_err() {
            TransportError::BoundsViolation {
                key_index,
                max_keys,
                layer,
                max_layers,
            } => {
                assert_eq!(key_index, 200);
                assert_eq!(max_keys, 108);
                assert_eq!(layer, 5);
                assert_eq!(max_layers, 4);
            }
            other => panic!("expected BoundsViolation, got: {:?}", other),
        }
    }

    fn make_device(key_count: Option<u8>, layer: Option<u8>) -> DeviceDefinition {
        DeviceDefinition {
            id: 1308,
            vid: 0x3141,
            pid: 0x4005,
            name: "test_device".to_string(),
            display_name: "Test".to_string(),
            company: None,
            device_type: "keyboard".to_string(),
            sources: vec![],
            key_count,
            key_layout_name: None,
            layer,
            fn_sys_layer: None,
            magnetism: None,
            no_magnetic_switch: None,
            has_light_layout: None,
            has_side_light: None,
            hot_swap: None,
            travel_setting: None,
            led_matrix: None,
            chip_family: None,
        }
    }

    #[test]
    fn test_validate_write_request_valid() {
        let device = make_device(Some(108), Some(4));
        assert!(validate_write_request(&device, 0, 0).is_ok());
        assert!(validate_write_request(&device, 107, 3).is_ok());
    }

    #[test]
    fn test_validate_write_request_missing_key_count() {
        let device = make_device(None, Some(4));
        let result = validate_write_request(&device, 0, 0);
        assert!(result.is_err());
        match result.unwrap_err() {
            TransportError::BoundsViolation { max_keys, .. } => {
                assert_eq!(max_keys, 0, "missing key_count should report max_keys=0");
            }
            other => panic!("expected BoundsViolation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_write_request_missing_layer() {
        let device = make_device(Some(108), None);
        let result = validate_write_request(&device, 0, 0);
        assert!(result.is_err());
        match result.unwrap_err() {
            TransportError::BoundsViolation { max_layers, .. } => {
                assert_eq!(max_layers, 0, "missing layer should report max_layers=0");
            }
            other => panic!("expected BoundsViolation, got: {:?}", other),
        }
    }

    #[test]
    fn test_udev_rules_file_exists() {
        let rules_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/deploy/99-monsgeek.rules"
        );
        let content = std::fs::read_to_string(rules_path)
            .unwrap_or_else(|e| panic!("udev rules file missing at {}: {}", rules_path, e));

        assert!(
            content.contains(r#"ATTRS{idVendor}=="3141""#),
            "udev rules must match MonsGeek VID 0x3141"
        );
        assert!(
            content.contains(r#"MODE="0660""#),
            "udev rules must set file permissions"
        );
        assert!(
            content.contains(r#"TAG+="uaccess""#),
            "udev rules must grant uaccess"
        );
        assert!(
            content.contains(r#"ATTR{bInterfaceNumber}=="02""#),
            "udev rules must target IF2"
        );
        assert!(
            content.contains("echo -n %k > /sys/bus/usb/drivers/usbhid/unbind"),
            "udev rules must unbind usbhid from IF2"
        );
    }
}
