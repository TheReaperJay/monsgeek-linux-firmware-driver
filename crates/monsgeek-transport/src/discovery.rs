//! Device discovery for MonsGeek keyboards via USB enumeration.
//!
//! Enumerates connected USB devices and matches them against the `DeviceRegistry`
//! to identify supported keyboards. Each matched device produces a `DeviceInfo`
//! with VID, PID, device ID, display name, internal name, and USB bus location.

use monsgeek_protocol::DeviceRegistry;

use crate::error::TransportError;

/// Information about a discovered MonsGeek keyboard.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// USB Vendor ID (0x3141 for MonsGeek).
    pub vid: u16,
    /// USB Product ID.
    pub pid: u16,
    /// Device ID from the registry (matches the JSON definition).
    pub device_id: i32,
    /// Human-readable display name (e.g., "M5W").
    pub display_name: String,
    /// Internal device name (e.g., "yc3121_m5w_soc").
    pub name: String,
    /// USB bus number.
    pub bus: u8,
    /// USB device address on the bus.
    pub address: u8,
}

/// Enumerate connected USB devices and match them against the device registry.
///
/// Iterates all USB devices via `rusb::devices()`, checks each device's VID/PID
/// against the registry, and returns a `DeviceInfo` for every match. A single
/// physical device may produce multiple entries if the registry maps its VID/PID
/// to multiple device IDs.
///
/// # Errors
///
/// Returns `TransportError::Usb` if `rusb::devices()` fails (e.g., no USB access).
pub fn enumerate_devices(registry: &DeviceRegistry) -> Result<Vec<DeviceInfo>, TransportError> {
    let devices = rusb::devices()?;
    let mut found = Vec::new();

    for device in devices.iter() {
        let descriptor = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => continue,
        };

        let vid = descriptor.vendor_id();
        let pid = descriptor.product_id();

        let matched = registry.find_by_vid_pid(vid, pid);
        if matched.is_empty() {
            continue;
        }

        let bus = device.bus_number();
        let address = device.address();

        for definition in matched {
            log::info!(
                "Found {} at bus {} addr {} (VID:0x{:04X} PID:0x{:04X})",
                definition.display_name,
                bus,
                address,
                vid,
                pid,
            );

            found.push(DeviceInfo {
                vid,
                pid,
                device_id: definition.id,
                display_name: definition.display_name.clone(),
                name: definition.name.clone(),
                bus,
                address,
            });
        }
    }

    Ok(found)
}

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
        let _fn_ptr: fn(
            &monsgeek_protocol::DeviceRegistry,
        ) -> Result<Vec<DeviceInfo>, crate::error::TransportError> = enumerate_devices;
    }

    #[test]
    fn test_enumerate_devices_with_empty_registry() {
        let registry = monsgeek_protocol::DeviceRegistry::new();
        let result = enumerate_devices(&registry);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
