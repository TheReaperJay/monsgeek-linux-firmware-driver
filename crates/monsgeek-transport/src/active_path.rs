use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const ACTIVE_DEVICES_DIR_ENV: &str = "MONSGEEK_ACTIVE_DEVICES_DIR";
const DEFAULT_ACTIVE_DEVICES_DIR: &str = "/run/monsgeek/active-devices";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivePathState {
    pub instance_path: String,
    pub usb_location: String,
    pub bus: u8,
    pub address: u8,
    pub vid: u16,
    pub pid: u16,
    pub updated_at_unix_ms: u64,
}

impl ActivePathState {
    pub fn is_fresh(&self, max_age: Duration) -> bool {
        let now_ms = unix_now_ms();
        let age_ms = now_ms.saturating_sub(self.updated_at_unix_ms);
        age_ms <= duration_to_millis(max_age)
    }
}

pub fn active_devices_dir() -> PathBuf {
    if let Ok(path) = std::env::var(ACTIVE_DEVICES_DIR_ENV)
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }
    PathBuf::from(DEFAULT_ACTIVE_DEVICES_DIR)
}

pub fn publish_active_path(
    instance_path: &str,
    usb_location: &str,
    vid: u16,
    pid: u16,
    bus: u8,
    address: u8,
) -> io::Result<()> {
    let state = ActivePathState {
        instance_path: instance_path.to_string(),
        usb_location: usb_location.to_string(),
        bus,
        address,
        vid,
        pid,
        updated_at_unix_ms: unix_now_ms(),
    };
    write_active_path(&state)
}

pub fn write_active_path(state: &ActivePathState) -> io::Result<()> {
    let dir = active_devices_dir();
    let path = state_file_path(&dir, &state.instance_path);
    write_active_path_to(&path, state)
}

pub fn remove_active_path(instance_path: &str) -> io::Result<()> {
    let path = state_file_path(&active_devices_dir(), instance_path);
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub fn read_active_paths(max_age: Duration) -> Vec<ActivePathState> {
    let dir = active_devices_dir();
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut states = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(state) = read_active_path_from(&path) else {
            continue;
        };
        if state.is_fresh(max_age) {
            states.push(state);
        }
    }

    states.sort_by(|a, b| b.updated_at_unix_ms.cmp(&a.updated_at_unix_ms));
    states
}

pub fn read_active_path(max_age: Duration) -> Option<ActivePathState> {
    read_active_paths(max_age).into_iter().next()
}

pub fn clear_active_path() -> io::Result<()> {
    let dir = active_devices_dir();
    match fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn write_active_path_to(path: &Path, state: &ActivePathState) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = path.with_extension("tmp");
    let payload =
        serde_json::to_vec(state).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    fs::write(&tmp, payload)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn read_active_path_from(path: &Path) -> io::Result<ActivePathState> {
    let raw = fs::read(path)?;
    serde_json::from_slice::<ActivePathState>(&raw)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn state_file_path(dir: &Path, instance_path: &str) -> PathBuf {
    let mut file = String::with_capacity(instance_path.len() * 2 + 5);
    for byte in instance_path.as_bytes() {
        file.push(hex_char(byte >> 4));
        file.push(hex_char(byte & 0x0F));
    }
    file.push_str(".json");
    dir.join(file)
}

fn hex_char(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => '0',
    }
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn duration_to_millis(duration: Duration) -> u64 {
    duration.as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_dir(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let now = unix_now_ms();
        path.push(format!("monsgeek-active-devices-{label}-{now}"));
        path
    }

    fn sample_state(path: &str) -> ActivePathState {
        ActivePathState {
            instance_path: path.to_string(),
            usb_location: "usb-b003-p1.2".to_string(),
            bus: 3,
            address: 7,
            vid: 0x3151,
            pid: 0x4015,
            updated_at_unix_ms: unix_now_ms(),
        }
    }

    #[test]
    fn state_freshness_honors_age() {
        let state = sample_state("usb-b003-p1.2");
        assert!(state.is_fresh(Duration::from_secs(2)));
        assert!(state.is_fresh(Duration::from_millis(0)));
    }

    #[test]
    fn write_read_roundtrip() {
        let dir = unique_test_dir("roundtrip");
        let state = sample_state("usb-b003-p1.2");
        let path = state_file_path(&dir, &state.instance_path);
        write_active_path_to(&path, &state).expect("write should succeed");
        let loaded = read_active_path_from(&path).expect("read should succeed");
        assert_eq!(state, loaded);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn state_file_path_is_hex_encoded() {
        let dir = unique_test_dir("path");
        let path = state_file_path(&dir, "usb-b003-p1.2");
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        assert!(name.ends_with(".json"));
        assert!(name.starts_with("7573622d623030332d70312e32"));
    }
}
