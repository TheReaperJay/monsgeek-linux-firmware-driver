//! Device discovery for MonsGeek keyboards via USB enumeration.
//!
//! Discovery is descriptor-first and non-destructive:
//! - enumerate supported runtime VID/PID candidates
//! - resolve uniquely-mapped candidates without opening USB
//! - issue a single `GET_USB_VERSION` query only when runtime VID/PID is ambiguous
//! - never run autonomous reset/reopen recovery during discovery

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use monsgeek_protocol::{ControlTransport, DeviceDefinition, DeviceRegistry};
use rusb::UsbContext;
use serde::Serialize;

use crate::active_path::{self, ActivePathState};
use crate::controller::CommandController;
use crate::error::TransportError;
use crate::usb::UsbSession;

/// Information about a discovered MonsGeek keyboard instance.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    /// Opaque, topology-based instance identifier stable across runtime PID flips.
    pub instance_path: String,
    /// Human-readable USB topology location, e.g. `usb-b003-p1.2`.
    pub usb_location: String,
    /// USB Vendor ID (runtime descriptor).
    pub vid: u16,
    /// USB Product ID (runtime descriptor).
    pub pid: u16,
    /// Canonical PID from the registry definition.
    pub canonical_pid: u16,
    /// Device ID from the registry (matches the firmware definition).
    pub device_id: i32,
    /// Human-readable display name (e.g., `M5W`).
    pub display_name: String,
    /// Internal device name (e.g., `yc3121_m5w_soc`).
    pub name: String,
    /// Connection mode inferred for the runtime path.
    pub connection_mode: ConnectionMode,
    /// USB bus number.
    pub bus: u8,
    /// USB device address on the bus.
    pub address: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ConnectionMode {
    Usb,
    Dongle24g,
    Bluetooth,
    Unknown,
}

impl ConnectionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Usb => "usb",
            Self::Dongle24g => "24g",
            Self::Bluetooth => "bluetooth",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsbCandidate {
    pub instance_path: String,
    pub usb_location: String,
    pub vid: u16,
    pub pid: u16,
    pub bus: u8,
    pub address: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProbeStrategy {
    DescriptorOnly,
    FirmwareQuery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProbeOutcome {
    Identified,
    OpenFailed,
    QueryFailed,
    UnknownDeviceId,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeAttempt {
    pub vid: u16,
    pub pid: u16,
    pub bus: u8,
    pub address: u8,
    pub strategy: ProbeStrategy,
    pub outcome: ProbeOutcome,
    pub resolved_device_id: Option<i32>,
    pub error: Option<String>,
    pub recovery_attempted: bool,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeReport {
    pub active_hints: Vec<ActivePathState>,
    pub attempts: Vec<ProbeAttempt>,
    pub found: Vec<DeviceInfo>,
}

struct ProbeCandidateResult {
    attempt: ProbeAttempt,
    info: Option<DeviceInfo>,
}

const ACTIVE_PATH_TTL: Duration = Duration::from_secs(20);

pub fn enumerate_devices(registry: &DeviceRegistry) -> Result<Vec<DeviceInfo>, TransportError> {
    probe_devices(registry)
}

/// Enumerate supported devices by descriptor only.
///
/// This never opens the USB device and is safe to use for live input paths.
pub fn find_devices_no_probe(registry: &DeviceRegistry) -> Result<Vec<DeviceInfo>, TransportError> {
    if registry.is_empty() {
        return Ok(Vec::new());
    }

    let mut found = Vec::new();
    for candidate in enumerate_all_usb_candidates()? {
        let matches = registry.find_by_runtime_vid_pid(candidate.vid, candidate.pid);
        if matches.is_empty() {
            continue;
        }

        let definition = matches
            .iter()
            .copied()
            .find(|device| device.pid == candidate.pid)
            .or_else(|| matches.iter().copied().min_by_key(|device| device.id))
            .expect("matches is non-empty");
        found.push(device_info_from_definition(definition, &candidate));
    }

    dedup_found(&mut found);
    Ok(found)
}

pub fn probe_devices(registry: &DeviceRegistry) -> Result<Vec<DeviceInfo>, TransportError> {
    Ok(probe_devices_with_report(registry)?.found)
}

pub fn probe_devices_with_report(registry: &DeviceRegistry) -> Result<ProbeReport, TransportError> {
    if registry.is_empty() {
        let report = ProbeReport {
            active_hints: Vec::new(),
            attempts: Vec::new(),
            found: Vec::new(),
        };
        set_last_probe_report(&report);
        return Ok(report);
    }

    let active_hints = active_path::read_active_paths(ACTIVE_PATH_TTL);
    let active_by_instance: HashMap<&str, &ActivePathState> = active_hints
        .iter()
        .map(|state| (state.instance_path.as_str(), state))
        .collect();

    let mut attempts = Vec::new();
    let mut found = Vec::new();
    let mut candidates = enumerate_all_usb_candidates()?;
    candidates.sort_by_key(|candidate| {
        (
            !active_by_instance.contains_key(candidate.instance_path.as_str()),
            candidate.instance_path.clone(),
        )
    });

    for candidate in candidates {
        if !registry_supports_candidate(registry, &candidate) {
            continue;
        }

        let strategy = choose_probe_strategy(registry, &candidate);
        let result = probe_candidate_identity(registry, &candidate, strategy);
        attempts.push(result.attempt);
        if let Some(info) = result.info {
            found.push(info);
        }
    }

    dedup_found(&mut found);
    let report = ProbeReport {
        active_hints,
        attempts,
        found,
    };
    set_last_probe_report(&report);
    Ok(report)
}

fn choose_probe_strategy(registry: &DeviceRegistry, candidate: &UsbCandidate) -> ProbeStrategy {
    if registry
        .find_by_runtime_vid_pid(candidate.vid, candidate.pid)
        .len()
        <= 1
    {
        ProbeStrategy::DescriptorOnly
    } else {
        ProbeStrategy::FirmwareQuery
    }
}

fn probe_candidate_identity(
    registry: &DeviceRegistry,
    candidate: &UsbCandidate,
    strategy: ProbeStrategy,
) -> ProbeCandidateResult {
    let started = Instant::now();

    if strategy == ProbeStrategy::DescriptorOnly
        && let Some(definition) = unique_runtime_match(registry, candidate)
    {
        return ProbeCandidateResult {
            attempt: finalize_attempt(
                candidate,
                strategy,
                ProbeOutcome::Identified,
                Some(definition.id),
                None,
                started,
            ),
            info: Some(device_info_from_definition(definition, candidate)),
        };
    }

    let session = match UsbSession::open_at(candidate.bus, candidate.address) {
        Ok(session) => session,
        Err(err) => {
            return ProbeCandidateResult {
                attempt: finalize_attempt(
                    candidate,
                    ProbeStrategy::FirmwareQuery,
                    ProbeOutcome::OpenFailed,
                    None,
                    Some(err.to_string()),
                    started,
                ),
                info: None,
            };
        }
    };

    let mut controller = CommandController::new(session, ControlTransport::Direct);
    let usb_version = match controller.query_usb_version_discovery() {
        Ok(info) => info,
        Err(err) => {
            return ProbeCandidateResult {
                attempt: finalize_attempt(
                    candidate,
                    ProbeStrategy::FirmwareQuery,
                    ProbeOutcome::QueryFailed,
                    None,
                    Some(err.to_string()),
                    started,
                ),
                info: None,
            };
        }
    };

    let Some(definition) = registry.find_by_id(usb_version.device_id_i32()) else {
        return ProbeCandidateResult {
            attempt: finalize_attempt(
                candidate,
                ProbeStrategy::FirmwareQuery,
                ProbeOutcome::UnknownDeviceId,
                None,
                Some(format!("unknown device id {}", usb_version.device_id)),
                started,
            ),
            info: None,
        };
    };

    ProbeCandidateResult {
        attempt: finalize_attempt(
            candidate,
            ProbeStrategy::FirmwareQuery,
            ProbeOutcome::Identified,
            Some(definition.id),
            None,
            started,
        ),
        info: Some(device_info_from_definition(definition, candidate)),
    }
}

fn registry_supports_candidate(registry: &DeviceRegistry, candidate: &UsbCandidate) -> bool {
    registry.supports_runtime_vid_pid(candidate.vid, candidate.pid)
}

fn unique_runtime_match<'a>(
    registry: &'a DeviceRegistry,
    candidate: &UsbCandidate,
) -> Option<&'a DeviceDefinition> {
    let matches = registry.find_by_runtime_vid_pid(candidate.vid, candidate.pid);
    (matches.len() == 1).then_some(matches[0])
}

fn finalize_attempt(
    candidate: &UsbCandidate,
    strategy: ProbeStrategy,
    outcome: ProbeOutcome,
    resolved_device_id: Option<i32>,
    error: Option<String>,
    started: Instant,
) -> ProbeAttempt {
    ProbeAttempt {
        vid: candidate.vid,
        pid: candidate.pid,
        bus: candidate.bus,
        address: candidate.address,
        strategy,
        outcome,
        resolved_device_id,
        error,
        recovery_attempted: false,
        duration_ms: started.elapsed().as_millis(),
    }
}

fn probe_report_store() -> &'static Mutex<Option<ProbeReport>> {
    static LAST_PROBE_REPORT: OnceLock<Mutex<Option<ProbeReport>>> = OnceLock::new();
    LAST_PROBE_REPORT.get_or_init(|| Mutex::new(None))
}

fn set_last_probe_report(report: &ProbeReport) {
    *probe_report_store()
        .lock()
        .expect("probe report mutex poisoned") = Some(report.clone());
}

pub fn last_probe_report() -> Option<ProbeReport> {
    probe_report_store()
        .lock()
        .expect("probe report mutex poisoned")
        .clone()
}

/// Probe a single runtime USB location and resolve it by firmware device ID.
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
    let candidate = usb_candidate_from_device(&device, &descriptor);

    if !registry_supports_candidate(registry, &candidate) {
        return Ok(None);
    }

    Ok(probe_candidate_identity(
        registry,
        &candidate,
        choose_probe_strategy(registry, &candidate),
    )
    .info)
}

pub(crate) fn probe_device(device: &DeviceDefinition) -> Result<DeviceInfo, TransportError> {
    let mut last_error = None;

    for candidate in enumerate_usb_candidates(device.vid)? {
        if !device.supports_runtime_pid(candidate.pid) {
            continue;
        }

        let info = if candidate.pid == device.pid {
            device_info_from_definition(device, &candidate)
        } else {
            let session = match UsbSession::open_at(candidate.bus, candidate.address) {
                Ok(session) => session,
                Err(err) => {
                    last_error = Some(err);
                    continue;
                }
            };

            let mut controller = CommandController::new(session, ControlTransport::Direct);
            let usb_version = match controller.query_usb_version() {
                Ok(info) => info,
                Err(err) => {
                    last_error = Some(err);
                    continue;
                }
            };

            if usb_version.device_id_i32() != device.id {
                continue;
            }

            device_info_from_definition(device, &candidate)
        };

        return Ok(info);
    }

    Err(last_error.unwrap_or(TransportError::DeviceNotFound {
        vid: device.vid,
        pid: device.pid,
    }))
}

pub(crate) fn enumerate_usb_candidates(vid: u16) -> Result<Vec<UsbCandidate>, TransportError> {
    Ok(enumerate_all_usb_candidates()?
        .into_iter()
        .filter(|candidate| candidate.vid == vid)
        .collect())
}

fn enumerate_all_usb_candidates() -> Result<Vec<UsbCandidate>, TransportError> {
    let context = rusb::Context::new()?;
    let devices = context.devices()?;
    let mut candidates = Vec::new();

    for device in devices.iter() {
        let descriptor = match device.device_descriptor() {
            Ok(descriptor) => descriptor,
            Err(_) => continue,
        };
        candidates.push(usb_candidate_from_device(&device, &descriptor));
    }

    Ok(candidates)
}

fn usb_candidate_from_device(
    device: &rusb::Device<rusb::Context>,
    descriptor: &rusb::DeviceDescriptor,
) -> UsbCandidate {
    let usb_location = format_usb_location(device);
    let instance_path = usb_location.clone();
    UsbCandidate {
        instance_path,
        usb_location,
        vid: descriptor.vendor_id(),
        pid: descriptor.product_id(),
        bus: device.bus_number(),
        address: device.address(),
    }
}

fn format_usb_location(device: &rusb::Device<rusb::Context>) -> String {
    let bus = device.bus_number();
    match device.port_numbers() {
        Ok(ports) if !ports.is_empty() => format!(
            "usb-b{bus:03}-p{}",
            ports
                .iter()
                .map(|port| port.to_string())
                .collect::<Vec<String>>()
                .join(".")
        ),
        _ => format!("usb-b{bus:03}-a{:03}", device.address()),
    }
}

fn device_info_from_definition(
    definition: &DeviceDefinition,
    candidate: &UsbCandidate,
) -> DeviceInfo {
    DeviceInfo {
        instance_path: candidate.instance_path.clone(),
        usb_location: candidate.usb_location.clone(),
        vid: candidate.vid,
        pid: candidate.pid,
        canonical_pid: definition.pid,
        device_id: definition.id,
        display_name: definition.display_name.clone(),
        name: definition.name.clone(),
        connection_mode: infer_connection_mode(definition, candidate),
        bus: candidate.bus,
        address: candidate.address,
    }
}

fn infer_connection_mode(
    definition: &DeviceDefinition,
    candidate: &UsbCandidate,
) -> ConnectionMode {
    if candidate.pid == definition.pid {
        ConnectionMode::Usb
    } else {
        ConnectionMode::Unknown
    }
}

fn dedup_found(found: &mut Vec<DeviceInfo>) {
    found.sort_by_key(|info| info.instance_path.clone());
    found.dedup_by_key(|info| info.instance_path.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_info_struct_fields() {
        let info = DeviceInfo {
            instance_path: "usb-b001-p1.2".to_string(),
            usb_location: "usb-b001-p1.2".to_string(),
            vid: 0x3151,
            pid: 0x4015,
            canonical_pid: 0x4015,
            device_id: 1308,
            display_name: "M5W".to_string(),
            name: "yc3121_m5w_soc".to_string(),
            connection_mode: ConnectionMode::Usb,
            bus: 1,
            address: 5,
        };
        assert_eq!(info.instance_path, "usb-b001-p1.2");
        assert_eq!(info.usb_location, "usb-b001-p1.2");
        assert_eq!(info.canonical_pid, 0x4015);
        assert_eq!(info.device_id, 1308);
        assert_eq!(info.connection_mode.as_str(), "usb");
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
}
