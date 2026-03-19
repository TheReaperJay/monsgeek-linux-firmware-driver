//! Device discovery for MonsGeek keyboards via USB enumeration.
//!
//! Enumerates connected USB devices and matches them against the `DeviceRegistry`
//! to identify supported keyboards.

// Placeholder — tests drive implementation.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_info_struct_fields() {
        let info = DeviceInfo {
            vid: 0x3141,
            pid: 0x4005,
            device_id: 1308,
            display_name: "M5W".to_string(),
            name: "yc3121_m5w_soc".to_string(),
            bus: 1,
            address: 5,
        };
        assert_eq!(info.vid, 0x3141);
        assert_eq!(info.pid, 0x4005);
        assert_eq!(info.device_id, 1308);
        assert_eq!(info.display_name, "M5W");
        assert_eq!(info.name, "yc3121_m5w_soc");
        assert_eq!(info.bus, 1);
        assert_eq!(info.address, 5);
    }

    #[test]
    fn test_device_info_is_clone() {
        let info = DeviceInfo {
            vid: 0x3141,
            pid: 0x4005,
            device_id: 1308,
            display_name: "M5W".to_string(),
            name: "yc3121_m5w_soc".to_string(),
            bus: 1,
            address: 5,
        };
        let cloned = info.clone();
        assert_eq!(cloned.vid, info.vid);
        assert_eq!(cloned.device_id, info.device_id);
    }

    #[test]
    fn test_device_info_is_debug() {
        let info = DeviceInfo {
            vid: 0x3141,
            pid: 0x4005,
            device_id: 1308,
            display_name: "M5W".to_string(),
            name: "yc3121_m5w_soc".to_string(),
            bus: 1,
            address: 5,
        };
        let debug_str = format!("{:?}", info);
        assert!(debug_str.contains("DeviceInfo"));
        assert!(debug_str.contains("M5W"));
    }

    #[test]
    fn test_enumerate_devices_exists_with_correct_signature() {
        // Verify the function signature compiles
        let _fn_ptr: fn(
            &monsgeek_protocol::DeviceRegistry,
        ) -> Result<Vec<DeviceInfo>, crate::error::TransportError> = enumerate_devices;
    }

    #[test]
    fn test_enumerate_devices_with_empty_registry() {
        // Empty registry should match no devices (even if USB devices exist)
        let registry = monsgeek_protocol::DeviceRegistry::new();
        let result = enumerate_devices(&registry);
        // Should not error — just return empty or whatever devices match (none)
        assert!(result.is_ok());
        // With an empty registry, no devices should match
        assert!(result.unwrap().is_empty());
    }
}
