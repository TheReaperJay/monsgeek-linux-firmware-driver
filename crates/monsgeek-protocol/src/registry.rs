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
