use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const ACTIVE_PATH_FILE_ENV: &str = "MONSGEEK_ACTIVE_PATH_FILE";
const DEFAULT_ACTIVE_PATH_FILE: &str = "/run/monsgeek/active-path.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivePathState {
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

pub fn active_path_file() -> PathBuf {
    if let Ok(path) = std::env::var(ACTIVE_PATH_FILE_ENV)
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }
    PathBuf::from(DEFAULT_ACTIVE_PATH_FILE)
}

pub fn publish_active_path(vid: u16, pid: u16, bus: u8, address: u8) -> io::Result<()> {
    let state = ActivePathState {
        bus,
        address,
        vid,
        pid,
        updated_at_unix_ms: unix_now_ms(),
    };
    write_active_path(&state)
}

pub fn write_active_path(state: &ActivePathState) -> io::Result<()> {
    write_active_path_to(&active_path_file(), state)
}

pub fn read_active_path(max_age: Duration) -> Option<ActivePathState> {
    let path = active_path_file();
    let state = read_active_path_from(&path).ok()?;
    state.is_fresh(max_age).then_some(state)
}

pub fn clear_active_path() -> io::Result<()> {
    let path = active_path_file();
    match fs::remove_file(path) {
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

    fn unique_test_path(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let now = unix_now_ms();
        path.push(format!("monsgeek-active-path-{label}-{now}.json"));
        path
    }

    #[test]
    fn state_freshness_honors_age() {
        let state = ActivePathState {
            bus: 1,
            address: 2,
            vid: 0x3151,
            pid: 0x4015,
            updated_at_unix_ms: unix_now_ms(),
        };
        assert!(state.is_fresh(Duration::from_secs(2)));
        assert!(state.is_fresh(Duration::from_millis(0)));
    }

    #[test]
    fn write_read_roundtrip() {
        let path = unique_test_path("roundtrip");
        let state = ActivePathState {
            bus: 1,
            address: 2,
            vid: 0x3151,
            pid: 0x4015,
            updated_at_unix_ms: unix_now_ms(),
        };
        write_active_path_to(&path, &state).expect("write should succeed");
        let loaded = read_active_path_from(&path).expect("read should succeed");
        assert_eq!(state, loaded);
        let _ = fs::remove_file(path);
    }
}
