use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DeviceRegistration {
    pub path: String,
    pub device_id: i32,
    pub vid: u16,
    pub pid: u16,
}

#[derive(Debug, Default)]
pub struct DevicePathRegistry {
    by_path: HashMap<String, DeviceRegistration>,
    by_bus_address: HashMap<(u8, u8), String>,
    next_suffix: u64,
}

impl DevicePathRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn make_path(
        &mut self,
        vid: u16,
        pid: u16,
        device_id: i32,
        bus: u8,
        address: u8,
    ) -> String {
        self.next_suffix = self.next_suffix.saturating_add(1);
        // Keep the first 5 dash-separated parts compatible with webapp parse_device_path.
        // Extra identity details live in the @suffix.
        format!(
            "{:04x}-{:04x}-ffff-0002-1@id{}-b{:03}-a{:03}-n{}",
            vid, pid, device_id, bus, address, self.next_suffix
        )
    }

    pub fn register(
        &mut self,
        vid: u16,
        pid: u16,
        device_id: i32,
        bus: u8,
        address: u8,
    ) -> DeviceRegistration {
        if let Some(path) = self.by_bus_address.get(&(bus, address)).cloned() {
            if let Some(existing) = self.by_path.get(&path) {
                return existing.clone();
            }
        }

        let path = self.make_path(vid, pid, device_id, bus, address);
        let registration = DeviceRegistration {
            path: path.clone(),
            device_id,
            vid,
            pid,
        };
        self.by_bus_address.insert((bus, address), path.clone());
        self.by_path.insert(path, registration.clone());
        registration
    }

    pub fn get_by_bus_address(&self, bus: u8, address: u8) -> Option<DeviceRegistration> {
        let path = self.by_bus_address.get(&(bus, address))?;
        self.by_path.get(path).cloned()
    }

    pub fn remove_by_bus_address(&mut self, bus: u8, address: u8) -> Option<DeviceRegistration> {
        let path = self.by_bus_address.remove(&(bus, address))?;
        self.by_path.remove(&path)
    }

    pub fn list(&self) -> Vec<DeviceRegistration> {
        self.by_path.values().cloned().collect()
    }

    pub fn parse_vid_pid(path: &str) -> Option<(u16, u16)> {
        let mut parts = path.split('-');
        let vid = u16::from_str_radix(parts.next()?, 16).ok()?;
        let pid = u16::from_str_radix(parts.next()?, 16).ok()?;
        Some((vid, pid))
    }
}

#[cfg(test)]
mod tests {
    use super::DevicePathRegistry;

    #[test]
    fn synthetic_path_is_unique() {
        let mut registry = DevicePathRegistry::new();
        let p1 = registry.make_path(0x3151, 0x4015, 1308, 3, 11);
        let p2 = registry.make_path(0x3151, 0x4015, 1308, 3, 11);
        assert_ne!(p1, p2);
    }

    #[test]
    fn synthetic_path_prefix_has_vid_pid() {
        let path = DevicePathRegistry::new().make_path(0x3151, 0x4015, 1308, 3, 11);
        let parsed = DevicePathRegistry::parse_vid_pid(&path).unwrap();
        assert_eq!(parsed, (0x3151, 0x4015));
    }
}
