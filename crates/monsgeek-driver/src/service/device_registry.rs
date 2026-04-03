use std::collections::HashMap;

use monsgeek_transport::discovery::{ConnectionMode, DeviceInfo};

#[derive(Debug, Clone)]
pub struct DeviceRegistration {
    pub path: String,
    pub usb_location: String,
    pub device_id: i32,
    pub vid: u16,
    pub pid: u16,
    pub canonical_pid: u16,
    pub connection_mode: ConnectionMode,
    pub bus: u8,
    pub address: u8,
    pub online: bool,
}

impl DeviceRegistration {
    pub fn from_device_info(info: &DeviceInfo) -> Self {
        Self {
            path: info.instance_path.clone(),
            usb_location: info.usb_location.clone(),
            device_id: info.device_id,
            vid: info.vid,
            pid: info.pid,
            canonical_pid: info.canonical_pid,
            connection_mode: info.connection_mode,
            bus: info.bus,
            address: info.address,
            online: true,
        }
    }
}

#[derive(Debug, Default)]
pub struct DevicePathRegistry {
    by_path: HashMap<String, DeviceRegistration>,
    by_usb_location: HashMap<String, String>,
    by_bus_address: HashMap<(u8, u8), String>,
}

impl DevicePathRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all_registrations(&self) -> Vec<DeviceRegistration> {
        self.by_path.values().cloned().collect()
    }

    pub fn get(&self, path: &str) -> Option<DeviceRegistration> {
        self.by_path.get(path).cloned()
    }

    pub fn upsert(&mut self, info: &DeviceInfo) -> DeviceRegistration {
        let path = self
            .by_usb_location
            .get(&info.usb_location)
            .cloned()
            .unwrap_or_else(|| info.instance_path.clone());

        if let Some(existing) = self.by_path.get_mut(&path) {
            self.by_bus_address
                .remove(&(existing.bus, existing.address));
            existing.usb_location = info.usb_location.clone();
            existing.device_id = info.device_id;
            existing.vid = info.vid;
            existing.pid = info.pid;
            existing.canonical_pid = info.canonical_pid;
            existing.connection_mode = info.connection_mode;
            existing.bus = info.bus;
            existing.address = info.address;
            existing.online = true;
            self.by_usb_location
                .insert(existing.usb_location.clone(), path.clone());
            self.by_bus_address
                .insert((existing.bus, existing.address), path.clone());
            return existing.clone();
        }

        let registration = DeviceRegistration {
            path: path.clone(),
            usb_location: info.usb_location.clone(),
            device_id: info.device_id,
            vid: info.vid,
            pid: info.pid,
            canonical_pid: info.canonical_pid,
            connection_mode: info.connection_mode,
            bus: info.bus,
            address: info.address,
            online: true,
        };
        self.by_usb_location
            .insert(registration.usb_location.clone(), path.clone());
        self.by_bus_address
            .insert((registration.bus, registration.address), path.clone());
        self.by_path.insert(path, registration.clone());
        registration
    }

    pub fn upsert_registration(&mut self, registration: &DeviceRegistration) -> DeviceRegistration {
        self.upsert(&DeviceInfo {
            instance_path: registration.path.clone(),
            usb_location: registration.usb_location.clone(),
            vid: registration.vid,
            pid: registration.pid,
            canonical_pid: registration.canonical_pid,
            device_id: registration.device_id,
            display_name: String::new(),
            name: String::new(),
            connection_mode: registration.connection_mode,
            bus: registration.bus,
            address: registration.address,
        })
    }

    pub fn mark_offline_missing_paths(
        &mut self,
        live_paths: &std::collections::HashSet<String>,
    ) -> Vec<DeviceRegistration> {
        let mut changed = Vec::new();
        let paths: Vec<String> = self.by_path.keys().cloned().collect();
        for path in paths {
            let Some(existing) = self.by_path.get_mut(&path) else {
                continue;
            };
            if live_paths.contains(&path) || !existing.online {
                continue;
            }
            self.by_bus_address
                .remove(&(existing.bus, existing.address));
            existing.online = false;
            changed.push(existing.clone());
        }
        changed
    }

    pub fn clear(&mut self) {
        self.by_path.clear();
        self.by_usb_location.clear();
        self.by_bus_address.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_info(path: &str, usb_location: &str, pid: u16, bus: u8, address: u8) -> DeviceInfo {
        DeviceInfo {
            instance_path: path.to_string(),
            usb_location: usb_location.to_string(),
            vid: 0x3151,
            pid,
            canonical_pid: 0x4015,
            device_id: 1308,
            display_name: "M5W".to_string(),
            name: "yc3121_m5w_soc".to_string(),
            connection_mode: ConnectionMode::Usb,
            bus,
            address,
        }
    }

    #[test]
    fn upsert_roundtrip_by_path() {
        let mut registry = DevicePathRegistry::new();
        let reg = registry.upsert(&sample_info(
            "usb-b003-p1.2",
            "usb-b003-p1.2",
            0x4015,
            3,
            15,
        ));
        let found = registry.get("usb-b003-p1.2").unwrap();
        assert_eq!(found.path, reg.path);
        assert_eq!(found.device_id, 1308);
    }

    #[test]
    fn upsert_reuses_path_for_same_usb_location() {
        let mut registry = DevicePathRegistry::new();
        registry.upsert(&sample_info(
            "usb-b003-p1.2",
            "usb-b003-p1.2",
            0x4015,
            3,
            15,
        ));
        let reg = registry.upsert(&sample_info("usb-b003-p1.2", "usb-b003-p1.2", 0x4011, 3, 6));
        assert_eq!(reg.path, "usb-b003-p1.2");
        assert_eq!(reg.pid, 0x4011);
        assert_eq!(reg.address, 6);
    }

    #[test]
    fn mark_offline_updates_online_flag() {
        let mut registry = DevicePathRegistry::new();
        registry.upsert(&sample_info(
            "usb-b003-p1.2",
            "usb-b003-p1.2",
            0x4015,
            3,
            15,
        ));
        let live = std::collections::HashSet::new();
        let changed = registry.mark_offline_missing_paths(&live);
        assert_eq!(changed.len(), 1);
        assert!(!changed[0].online);
    }
}
