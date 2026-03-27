//! Device discovery for MonsGeek keyboards via USB enumeration.
//!
//! Enumerates connected USB devices and matches them against the `DeviceRegistry`
//! to identify supported keyboards. Each matched device produces a `DeviceInfo`
//! with VID, PID, device ID, display name, internal name, and USB bus location.

use std::collections::HashSet;

use monsgeek_protocol::{DeviceDefinition, DeviceRegistry};
use rusb::UsbContext;

use crate::controller::CommandController;
use crate::error::TransportError;
use crate::usb::UsbSession;

/// Information about a discovered MonsGeek keyboard.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// USB Vendor ID (from device registry JSON).
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UsbCandidate {
    pub vid: u16,
    pub pid: u16,
    pub bus: u8,
    pub address: u8,
}

/// Enumerate connected USB devices and resolve them by firmware device ID.
///
/// This API is retained for compatibility, but identity resolution is delegated
/// to [`probe_devices`] so callers always use `GET_USB_VERSION` instead of PID
/// heuristics.
pub fn enumerate_devices(registry: &DeviceRegistry) -> Result<Vec<DeviceInfo>, TransportError> {
    probe_devices(registry)
}

/// Find connected USB devices by matching VID/PID against the registry.
///
/// This function never opens a USB session, never claims interfaces, and never
/// sends vendor commands. It reads USB descriptors from the bus and matches
/// them against the registry's VID/PID index.
///
/// Use this for callers that open the device themselves (e.g., in
/// `SessionMode::InputOnly`) and cannot tolerate IF2 claims, vendor commands,
/// or STALL recovery resets that [`probe_devices`] performs.
pub fn find_devices_no_probe(
    registry: &DeviceRegistry,
) -> Result<Vec<DeviceInfo>, TransportError> {
    if registry.is_empty() {
        return Ok(Vec::new());
    }

    let context = rusb::Context::new()?;
    let devices = context.devices()?;
    let mut found = Vec::new();

    for device in devices.iter() {
        let descriptor = match device.device_descriptor() {
            Ok(desc) => desc,
            Err(_) => continue,
        };

        let vid = descriptor.vendor_id();
        let pid = descriptor.product_id();
        let matches = registry.find_by_runtime_vid_pid(vid, pid);

        if matches.is_empty() {
            continue;
        }

        let definition = matches[0];
        found.push(DeviceInfo {
            vid,
            pid,
            device_id: definition.id,
            display_name: definition.display_name.clone(),
            name: definition.name.clone(),
            bus: device.bus_number(),
            address: device.address(),
        });
    }

    found.sort_by_key(|info| (info.bus, info.address, info.vid, info.pid));
    Ok(found)
}

/// Probe connected USB devices and identify them by firmware device ID.
///
/// Unlike [`enumerate_devices`], this function does not trust USB PID alone.
/// It opens each candidate USB device for supported vendor IDs, sends
/// `GET_USB_VERSION`, and maps the returned firmware device ID back into the
/// registry. This is slower and may reset candidate devices, but it can still
/// identify keyboards whose runtime PID differs from the registry's primary PID.
pub fn probe_devices(registry: &DeviceRegistry) -> Result<Vec<DeviceInfo>, TransportError> {
    if registry.is_empty() {
        return Ok(Vec::new());
    }

    let vendor_ids: HashSet<u16> = registry.all_devices().map(|device| device.vid).collect();
    let mut found = Vec::new();

    for vid in vendor_ids {
        for candidate in enumerate_usb_candidates(vid)? {
            // Only probe runtime PIDs that exist in the registry. Scanning every
            // same-VID interface (e.g. dongle/wired side endpoints) causes
            // avoidable STALL recovery resets and can hide valid devices.
            if !registry_supports_candidate(registry, candidate) {
                log::debug!(
                    "Probe skipped bus {} addr {} (VID:0x{:04X} PID:0x{:04X}): not in registry",
                    candidate.bus,
                    candidate.address,
                    candidate.vid,
                    candidate.pid
                );
                continue;
            }

            let session = match UsbSession::open_at(candidate.bus, candidate.address) {
                Ok(session) => session,
                Err(err) => {
                    log::warn!(
                        "Probe open failed bus {} addr {} (VID:0x{:04X} PID:0x{:04X}): {}",
                        candidate.bus,
                        candidate.address,
                        candidate.vid,
                        candidate.pid,
                        err
                    );
                    continue;
                }
            };

            let mut controller = CommandController::new(session);
            let usb_version = match controller.query_usb_version() {
                Ok(info) => info,
                Err(first_err) => {
                    // Discovery should be non-destructive. If firmware query
                    // fails, fall back to unique runtime VID/PID alias mapping.
                    log::debug!(
                        "Probe query failed bus {} addr {} (VID:0x{:04X} PID:0x{:04X}): {}",
                        candidate.bus,
                        candidate.address,
                        candidate.vid,
                        candidate.pid,
                        first_err
                    );
                    if let Some(definition) = unique_runtime_match(registry, candidate) {
                        log::warn!(
                            "Probe fallback: using runtime VID/PID alias for bus {} addr {} (VID:0x{:04X} PID:0x{:04X}) -> device {} ({})",
                            candidate.bus,
                            candidate.address,
                            candidate.vid,
                            candidate.pid,
                            definition.id,
                            definition.display_name
                        );
                        found.push(device_info_from_definition(definition, candidate));
                    }
                    continue;
                }
            };

            let Some(definition) = registry.find_by_id(usb_version.device_id_i32()) else {
                log::debug!(
                    "Probe bus {} addr {} reported unknown device ID {}",
                    candidate.bus,
                    candidate.address,
                    usb_version.device_id
                );
                if let Some(definition) = unique_runtime_match(registry, candidate) {
                    log::warn!(
                        "Probe fallback: unknown reported device ID {}, using runtime alias for bus {} addr {} (VID:0x{:04X} PID:0x{:04X}) -> device {} ({})",
                        usb_version.device_id,
                        candidate.bus,
                        candidate.address,
                        candidate.vid,
                        candidate.pid,
                        definition.id,
                        definition.display_name
                    );
                    found.push(device_info_from_definition(definition, candidate));
                }
                continue;
            };

            found.push(device_info_from_definition(definition, candidate));
        }
    }

    found.sort_by_key(|info| (info.bus, info.address, info.vid, info.pid));
    Ok(found)
}

fn registry_supports_candidate(registry: &DeviceRegistry, candidate: UsbCandidate) -> bool {
    registry.supports_runtime_vid_pid(candidate.vid, candidate.pid)
}

fn unique_runtime_match(
    registry: &DeviceRegistry,
    candidate: UsbCandidate,
) -> Option<&DeviceDefinition> {
    let matches = registry.find_by_runtime_vid_pid(candidate.vid, candidate.pid);
    if matches.len() == 1 {
        return Some(matches[0]);
    }
    None
}

/// Probe a single runtime USB location and resolve it by firmware device ID.
///
/// This is used by hot-plug handling where bus/address is already known from udev.
/// The function opens only the target device, queries `GET_USB_VERSION`, and maps
/// the returned firmware ID to the registry.
///
/// Returns `Ok(None)` when:
/// - registry is empty
/// - no device exists at the given bus/address
/// - device VID/PID is not present in the registry
/// - firmware ID is unknown to the registry
pub fn probe_device_at(
    registry: &DeviceRegistry,
    bus: u8,
    address: u8,
) -> Result<Option<DeviceInfo>, TransportError> {
    if registry.is_empty() {
        return Ok(None);
    }

    let context = rusb::Context::new()?;
    let devices = context.devices()?;
    let Some(device) = devices
        .iter()
        .find(|device| device.bus_number() == bus && device.address() == address)
    else {
        return Ok(None);
    };

    let descriptor = match device.device_descriptor() {
        Ok(descriptor) => descriptor,
        Err(_) => return Ok(None),
    };
    let candidate = UsbCandidate {
        vid: descriptor.vendor_id(),
        pid: descriptor.product_id(),
        bus,
        address,
    };

    // Avoid probing completely unknown VID/PID pairs.
    if !registry_supports_candidate(registry, candidate) {
        return Ok(None);
    }

    let session = UsbSession::open_at(candidate.bus, candidate.address)?;
    let mut controller = CommandController::new(session);
    let usb_version = match controller.query_usb_version() {
        Ok(info) => info,
        Err(first_err) => {
            log::debug!(
                "probe_device_at: query failed bus {} addr {}: {}",
                bus,
                address,
                first_err
            );
            return Ok(unique_runtime_match(registry, candidate)
                .map(|definition| device_info_from_definition(definition, candidate)));
        }
    };
    let Some(definition) = registry.find_by_id(usb_version.device_id_i32()) else {
        log::debug!(
            "Probe bus {} addr {} reported unknown device ID {}",
            candidate.bus,
            candidate.address,
            usb_version.device_id
        );
        return Ok(unique_runtime_match(registry, candidate)
            .map(|definition| device_info_from_definition(definition, candidate)));
    };

    Ok(Some(device_info_from_definition(definition, candidate)))
}

pub(crate) fn probe_device(device: &DeviceDefinition) -> Result<DeviceInfo, TransportError> {
    let mut last_error = None;
    let mut saw_candidate = false;
    let mut saw_mismatch = false;

    for candidate in enumerate_usb_candidates(device.vid)? {
        saw_candidate = true;

        let session = match UsbSession::open_at(candidate.bus, candidate.address) {
            Ok(session) => session,
            Err(err) => {
                log::debug!(
                    "Probe open failed bus {} addr {} (VID:0x{:04X} PID:0x{:04X}): {}",
                    candidate.bus,
                    candidate.address,
                    candidate.vid,
                    candidate.pid,
                    err
                );
                last_error = Some(err);
                continue;
            }
        };

        let mut controller = CommandController::new(session);
        let usb_version = match controller.query_usb_version() {
            Ok(info) => info,
            Err(first_err) => {
                log::debug!(
                    "probe_device: first query failed bus {} addr {} (VID:0x{:04X} PID:0x{:04X}): {} — attempting STALL recovery",
                    candidate.bus,
                    candidate.address,
                    candidate.vid,
                    candidate.pid,
                    first_err
                );
                match controller
                    .into_session()
                    .reset_and_reopen()
                    .and_then(|session| CommandController::new(session).query_usb_version())
                {
                    Ok(info) => info,
                    Err(retry_err) => {
                        log::debug!(
                            "probe_device: STALL recovery also failed bus {} addr {} (VID:0x{:04X} PID:0x{:04X}): {}",
                            candidate.bus,
                            candidate.address,
                            candidate.vid,
                            candidate.pid,
                            retry_err
                        );
                        last_error = Some(retry_err);
                        continue;
                    }
                }
            }
        };

        if usb_version.device_id_i32() == device.id {
            return Ok(device_info_from_definition(device, candidate));
        }

        saw_mismatch = true;
        log::debug!(
            "Probe mismatch bus {} addr {} (VID:0x{:04X} PID:0x{:04X}): expected device ID {}, got {}",
            candidate.bus,
            candidate.address,
            candidate.vid,
            candidate.pid,
            device.id,
            usb_version.device_id
        );
    }

    if saw_mismatch || !saw_candidate {
        return Err(TransportError::DeviceNotFound {
            vid: device.vid,
            pid: device.pid,
        });
    }

    Err(last_error.unwrap_or(TransportError::DeviceNotFound {
        vid: device.vid,
        pid: device.pid,
    }))
}

pub(crate) fn enumerate_usb_candidates(vid: u16) -> Result<Vec<UsbCandidate>, TransportError> {
    let context = rusb::Context::new()?;
    let devices = context.devices()?;
    let mut candidates = Vec::new();

    for device in devices.iter() {
        let descriptor = match device.device_descriptor() {
            Ok(descriptor) => descriptor,
            Err(_) => continue,
        };

        if descriptor.vendor_id() != vid {
            continue;
        }

        candidates.push(UsbCandidate {
            vid,
            pid: descriptor.product_id(),
            bus: device.bus_number(),
            address: device.address(),
        });
    }

    Ok(candidates)
}

fn device_info_from_definition(
    definition: &DeviceDefinition,
    candidate: UsbCandidate,
) -> DeviceInfo {
    DeviceInfo {
        vid: candidate.vid,
        pid: candidate.pid,
        device_id: definition.id,
        display_name: definition.display_name.clone(),
        name: definition.name.clone(),
        bus: candidate.bus,
        address: candidate.address,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn protocol_devices_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../monsgeek-protocol")
            .join("devices")
    }

    #[test]
    fn test_device_info_struct_fields() {
        let info = DeviceInfo {
            vid: 0x3151,
            pid: 0x4015,
            device_id: 1308,
            display_name: "M5W".to_string(),
            name: "yc3121_m5w_soc".to_string(),
            bus: 1,
            address: 5,
        };
        assert_eq!(info.vid, 0x3151);
        assert_eq!(info.pid, 0x4015);
        assert_eq!(info.device_id, 1308);
        assert_eq!(info.display_name, "M5W");
        assert_eq!(info.name, "yc3121_m5w_soc");
        assert_eq!(info.bus, 1);
        assert_eq!(info.address, 5);
    }

    #[test]
    fn test_device_info_is_clone() {
        let info = DeviceInfo {
            vid: 0x3151,
            pid: 0x4015,
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
            vid: 0x3151,
            pid: 0x4015,
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
    fn test_probe_devices_exists_with_correct_signature() {
        let _fn_ptr: fn(
            &monsgeek_protocol::DeviceRegistry,
        ) -> Result<Vec<DeviceInfo>, crate::error::TransportError> = probe_devices;
    }

    #[test]
    fn test_probe_device_at_exists_with_correct_signature() {
        let _fn_ptr: fn(
            &monsgeek_protocol::DeviceRegistry,
            u8,
            u8,
        ) -> Result<Option<DeviceInfo>, crate::error::TransportError> = probe_device_at;
    }

    #[test]
    fn test_enumerate_devices_with_empty_registry() {
        let registry = monsgeek_protocol::DeviceRegistry::new();
        let result = enumerate_devices(&registry);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_usb_candidate_struct_fields() {
        let candidate = UsbCandidate {
            vid: 0x3151,
            pid: 0x4015,
            bus: 3,
            address: 11,
        };
        assert_eq!(candidate.vid, 0x3151);
        assert_eq!(candidate.pid, 0x4015);
        assert_eq!(candidate.bus, 3);
        assert_eq!(candidate.address, 11);
    }

    #[test]
    fn test_registry_supports_candidate_true_for_registered_pid() {
        let registry = monsgeek_protocol::DeviceRegistry::load_from_directory(&protocol_devices_dir())
            .expect("device registry should load");
        let candidate = UsbCandidate {
            vid: 0x3151,
            pid: 0x4015,
            bus: 0,
            address: 0,
        };
        assert!(super::registry_supports_candidate(&registry, candidate));
    }

    #[test]
    fn test_registry_supports_candidate_false_for_unregistered_pid() {
        let registry = monsgeek_protocol::DeviceRegistry::load_from_directory(&protocol_devices_dir())
            .expect("device registry should load");
        let candidate = UsbCandidate {
            vid: 0x3151,
            pid: 0x4FFF,
            bus: 0,
            address: 0,
        };
        assert!(!super::registry_supports_candidate(&registry, candidate));
    }

    #[test]
    fn test_unique_runtime_match_for_alias_pid() {
        let registry = monsgeek_protocol::DeviceRegistry::load_from_directory(&protocol_devices_dir())
            .expect("device registry should load");
        let candidate = UsbCandidate {
            vid: 0x3151,
            pid: 0x4011,
            bus: 0,
            address: 0,
        };
        let matched = super::unique_runtime_match(&registry, candidate).expect("should match alias");
        assert_eq!(matched.id, 1308);
    }
}
