use std::collections::HashMap;
use std::path::Path;

use crate::device::DeviceDefinition;
use crate::error::RegistryError;

/// Device registry with multi-index lookup by device ID and VID/PID.
///
/// Loads device definitions from a directory of per-device JSON files.
/// Adding a new keyboard requires only placing a new JSON file in the
/// devices directory -- no Rust source changes needed.
pub struct DeviceRegistry {
    devices_by_id: HashMap<i32, DeviceDefinition>,
    devices_by_vid_pid: HashMap<(u16, u16), Vec<i32>>,
}

impl DeviceRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            devices_by_id: HashMap::new(),
            devices_by_vid_pid: HashMap::new(),
        }
    }

    /// Load all `*.json` files from a directory into the registry.
    ///
    /// Each JSON file must contain a single `DeviceDefinition`. Files that
    /// fail to parse produce an error immediately (fail-fast).
    pub fn load_from_directory(dir: &Path) -> Result<Self, RegistryError> {
        let mut registry = Self::new();
        let pattern = dir.join("*.json");
        let pattern_str = pattern.to_str().ok_or_else(|| {
            RegistryError::GlobPattern(format!("invalid path: {}", dir.display()))
        })?;

        for entry in
            glob::glob(pattern_str).map_err(|e| RegistryError::GlobPattern(e.to_string()))?
        {
            let path = entry.map_err(|e| RegistryError::ReadFile(e.to_string()))?;
            let content = std::fs::read_to_string(&path)
                .map_err(|e| RegistryError::ReadFile(format!("{}: {}", path.display(), e)))?;
            let device: DeviceDefinition = serde_json::from_str(&content)
                .map_err(|e| RegistryError::ParseJson(format!("{}: {}", path.display(), e)))?;
            registry.add_device(device);
        }

        Ok(registry)
    }

    /// Add a device definition to the registry, indexing by ID and VID/PID.
    fn add_device(&mut self, device: DeviceDefinition) {
        let id = device.id;
        let vid_pid = (device.vid, device.pid);

        self.devices_by_vid_pid
            .entry(vid_pid)
            .or_default()
            .push(id);
        self.devices_by_id.insert(id, device);
    }

    /// Find a device by its unique device ID.
    pub fn find_by_id(&self, id: i32) -> Option<&DeviceDefinition> {
        self.devices_by_id.get(&id)
    }

    /// Find all devices matching a VID/PID combination.
    ///
    /// Multiple devices can share the same VID/PID (e.g., different firmware
    /// variants on the same hardware). Returns an empty vec if no match.
    pub fn find_by_vid_pid(&self, vid: u16, pid: u16) -> Vec<&DeviceDefinition> {
        self.devices_by_vid_pid
            .get(&(vid, pid))
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.devices_by_id.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Number of devices in the registry.
    pub fn len(&self) -> usize {
        self.devices_by_id.len()
    }

    /// Whether the registry contains no devices.
    pub fn is_empty(&self) -> bool {
        self.devices_by_id.is_empty()
    }

    /// Iterate over all device definitions in the registry.
    pub fn all_devices(&self) -> impl Iterator<Item = &DeviceDefinition> {
        self.devices_by_id.values()
    }
}

impl Default for DeviceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn devices_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("devices")
    }

    #[test]
    fn test_load_m5w_from_devices_dir() {
        let registry = DeviceRegistry::load_from_directory(&devices_dir()).unwrap();
        assert_eq!(registry.len(), 1);

        let device = registry.find_by_id(1308).expect("M5W not found by ID");
        assert_eq!(device.display_name, "M5W");
    }

    #[test]
    fn test_find_by_vid_pid_m5w() {
        let registry = DeviceRegistry::load_from_directory(&devices_dir()).unwrap();
        let matches = registry.find_by_vid_pid(0x3141, 0x4005);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, 1308);
    }

    #[test]
    fn test_find_by_vid_pid_no_match() {
        let registry = DeviceRegistry::load_from_directory(&devices_dir()).unwrap();
        let matches = registry.find_by_vid_pid(0xFFFF, 0xFFFF);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_find_by_id_no_match() {
        let registry = DeviceRegistry::load_from_directory(&devices_dir()).unwrap();
        assert!(registry.find_by_id(9999).is_none());
    }

    #[test]
    fn test_registry_extensible() {
        let tmp = std::env::temp_dir().join("monsgeek_registry_test_extensible");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Copy M5W
        fs::write(
            tmp.join("m5w.json"),
            include_str!("../devices/m5w.json"),
        )
        .unwrap();

        // Add a fictional second device
        fs::write(
            tmp.join("test_device.json"),
            r#"{
                "id": 9999,
                "vid": 12609,
                "pid": 16390,
                "name": "yc3121_test",
                "displayName": "Test Device"
            }"#,
        )
        .unwrap();

        let registry = DeviceRegistry::load_from_directory(&tmp).unwrap();
        assert_eq!(registry.len(), 2);
        assert!(registry.find_by_id(1308).is_some());
        assert!(registry.find_by_id(9999).is_some());
        assert_eq!(
            registry.find_by_id(9999).unwrap().display_name,
            "Test Device"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_empty_directory() {
        let tmp = std::env::temp_dir().join("monsgeek_registry_test_empty");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let registry = DeviceRegistry::load_from_directory(&tmp).unwrap();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_shared_vid_pid() {
        let tmp = std::env::temp_dir().join("monsgeek_registry_test_shared_vid_pid");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        // Two devices sharing the same VID/PID but different IDs
        fs::write(
            tmp.join("device_a.json"),
            r#"{"id": 100, "vid": 12609, "pid": 16389, "name": "dev_a", "displayName": "Device A"}"#,
        )
        .unwrap();
        fs::write(
            tmp.join("device_b.json"),
            r#"{"id": 200, "vid": 12609, "pid": 16389, "name": "dev_b", "displayName": "Device B"}"#,
        )
        .unwrap();

        let registry = DeviceRegistry::load_from_directory(&tmp).unwrap();
        assert_eq!(registry.len(), 2);

        let matches = registry.find_by_vid_pid(0x3141, 0x4005);
        assert_eq!(matches.len(), 2);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_invalid_json_returns_error() {
        let tmp = std::env::temp_dir().join("monsgeek_registry_test_invalid_json");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        fs::write(tmp.join("bad.json"), "{ not valid json }").unwrap();

        let result = DeviceRegistry::load_from_directory(&tmp);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&tmp);
    }
}
