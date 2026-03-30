//! Device discovery for MonsGeek keyboards via USB enumeration.
//!
//! Enumerates connected USB devices and matches them against the `DeviceRegistry`
//! to identify supported keyboards. Each matched device produces a `DeviceInfo`
//! with VID, PID, device ID, display name, internal name, and USB bus location.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use monsgeek_protocol::{DeviceDefinition, DeviceRegistry};
use rusb::UsbContext;
use serde::Serialize;

use crate::active_path::{self, ActivePathState};
use crate::controller::CommandController;
use crate::error::TransportError;
use crate::usb::UsbSession;

/// Information about a discovered MonsGeek keyboard.
#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProbeStrategy {
    Canonical,
    AliasDongle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProbeOutcome {
    Identified,
    OpenFailed,
    QueryFailed,
    RecoveryFailed,
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
    pub active_hint: Option<ActivePathState>,
    pub attempts: Vec<ProbeAttempt>,
    pub found: Vec<DeviceInfo>,
}

#[derive(Debug, Clone, Copy)]
struct ProbeTarget {
    candidate: UsbCandidate,
    strategy: ProbeStrategy,
}

struct ProbeCandidateResult {
    attempt: ProbeAttempt,
    info: Option<DeviceInfo>,
}

const ACTIVE_PATH_TTL: Duration = Duration::from_secs(20);

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
/// or any firmware command traffic that [`probe_devices`] performs.
pub fn find_devices_no_probe(registry: &DeviceRegistry) -> Result<Vec<DeviceInfo>, TransportError> {
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

        // Descriptor-only enumeration can map to multiple profiles sharing the
        // same runtime VID/PID. Prefer an exact canonical PID match, then pick
        // the smallest stable device ID for deterministic behavior.
        let definition = matches
            .iter()
            .copied()
            .find(|device| device.pid == pid)
            .or_else(|| matches.iter().copied().min_by_key(|device| device.id))
            .expect("matches is non-empty");
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
/// registry. This is slower than descriptor-only discovery, but it can still
/// identify keyboards whose runtime PID differs from the registry's primary PID.
pub fn probe_devices(registry: &DeviceRegistry) -> Result<Vec<DeviceInfo>, TransportError> {
    Ok(probe_devices_with_report(registry)?.found)
}

pub fn probe_devices_with_report(registry: &DeviceRegistry) -> Result<ProbeReport, TransportError> {
    if registry.is_empty() {
        let report = ProbeReport {
            active_hint: None,
            attempts: Vec::new(),
            found: Vec::new(),
        };
        set_last_probe_report(&report);
        return Ok(report);
    }

    let mut vendor_ids: Vec<u16> = registry
        .all_devices()
        .map(|device| device.vid)
        .collect::<HashSet<u16>>()
        .into_iter()
        .collect();
    vendor_ids.sort_unstable();

    let active_hint = active_path::read_active_path(ACTIVE_PATH_TTL);
    if let Some(active) = active_hint.as_ref() {
        log::debug!(
            "probe_devices: active hint {:03}:{:03} VID 0x{:04X} PID 0x{:04X}",
            active.bus,
            active.address,
            active.vid,
            active.pid
        );
    } else {
        log::debug!("probe_devices: no fresh active path hint");
    }

    let mut attempts = Vec::new();
    let mut found = Vec::new();

    for vid in vendor_ids {
        let active_for_vid = active_hint
            .as_ref()
            .filter(|state| state.vid == vid)
            .cloned();
        let mut canonical_candidates = Vec::new();
        let mut alias_candidates = Vec::new();

        for candidate in enumerate_usb_candidates(vid)? {
            log::info!(
                "probe_candidate_seen bus={:03} addr={:03} vid=0x{:04X} pid=0x{:04X}",
                candidate.bus,
                candidate.address,
                candidate.vid,
                candidate.pid
            );
            // Only probe runtime PIDs that exist in the registry.
            if !registry_supports_candidate(registry, candidate) {
                log::info!(
                    "probe_candidate_skipped_not_in_registry bus={:03} addr={:03} vid=0x{:04X} pid=0x{:04X}",
                    candidate.bus,
                    candidate.address,
                    candidate.vid,
                    candidate.pid
                );
                continue;
            }

            if registry_has_canonical_pid(registry, candidate) {
                log::info!(
                    "probe_candidate_classified bus={:03} addr={:03} vid=0x{:04X} pid=0x{:04X} class=canonical",
                    candidate.bus,
                    candidate.address,
                    candidate.vid,
                    candidate.pid
                );
                canonical_candidates.push(candidate);
            } else {
                log::info!(
                    "probe_candidate_classified bus={:03} addr={:03} vid=0x{:04X} pid=0x{:04X} class=alias_dongle",
                    candidate.bus,
                    candidate.address,
                    candidate.vid,
                    candidate.pid
                );
                alias_candidates.push(candidate);
            }
        }

        sort_candidates_by_active_hint(&mut canonical_candidates, active_for_vid.as_ref());
        sort_candidates_by_active_hint(&mut alias_candidates, active_for_vid.as_ref());

        let targets = build_probe_targets(
            &canonical_candidates,
            &alias_candidates,
            active_for_vid.as_ref(),
        );

        log::info!(
            "probe_devices: VID 0x{:04X} canonical={} alias={} targets={}",
            vid,
            canonical_candidates.len(),
            alias_candidates.len(),
            targets.len()
        );

        let mut canonical_identified_for_vid = false;
        for target in targets {
            if canonical_identified_for_vid && target.strategy == ProbeStrategy::AliasDongle {
                log::info!(
                    "probe_target_skipped bus={:03} addr={:03} vid=0x{:04X} pid=0x{:04X} strategy={:?} reason=canonical_already_identified_for_vid",
                    target.candidate.bus,
                    target.candidate.address,
                    target.candidate.vid,
                    target.candidate.pid,
                    target.strategy
                );
                continue;
            }
            log::info!(
                "probe_target_selected bus={:03} addr={:03} vid=0x{:04X} pid=0x{:04X} strategy={:?}",
                target.candidate.bus,
                target.candidate.address,
                target.candidate.vid,
                target.candidate.pid,
                target.strategy
            );
            let result = probe_candidate_identity(registry, target.candidate, target.strategy);
            if result.info.is_some() && target.strategy == ProbeStrategy::Canonical {
                canonical_identified_for_vid = true;
            }
            attempts.push(result.attempt);
            if let Some(info) = result.info {
                found.push(info);
            }
        }
    }

    found.sort_by_key(|info| (info.bus, info.address, info.vid, info.pid));
    found.dedup_by_key(|info| (info.bus, info.address, info.vid, info.pid, info.device_id));
    let report = ProbeReport {
        active_hint,
        attempts,
        found,
    };
    set_last_probe_report(&report);
    Ok(report)
}

fn probe_candidate_identity(
    registry: &DeviceRegistry,
    candidate: UsbCandidate,
    strategy: ProbeStrategy,
) -> ProbeCandidateResult {
    let started = Instant::now();

    let session = match UsbSession::open_at(candidate.bus, candidate.address) {
        Ok(session) => session,
        Err(err) => {
            log::warn!(
                "Probe open failed bus {} addr {} (VID:0x{:04X} PID:0x{:04X}) strategy={:?}: {}",
                candidate.bus,
                candidate.address,
                candidate.vid,
                candidate.pid,
                strategy,
                err
            );
            return ProbeCandidateResult {
                attempt: finalize_attempt(
                    candidate,
                    strategy,
                    ProbeOutcome::OpenFailed,
                    None,
                    Some(err.to_string()),
                    false,
                    started,
                ),
                info: None,
            };
        }
    };

    let mut controller = CommandController::new(session);
    let mut recovery_attempted = false;
    let usb_version = match query_usb_version_for_candidate(&mut controller, strategy) {
        Ok(info) => info,
        Err(first_err) => {
            log::debug!(
                "Probe query failed bus {} addr {} (VID:0x{:04X} PID:0x{:04X}) strategy={:?}: {}",
                candidate.bus,
                candidate.address,
                candidate.vid,
                candidate.pid,
                strategy,
                first_err
            );
            if !should_attempt_stall_recovery(&first_err, strategy) {
                return ProbeCandidateResult {
                    attempt: finalize_attempt(
                        candidate,
                        strategy,
                        ProbeOutcome::QueryFailed,
                        None,
                        Some(first_err.to_string()),
                        false,
                        started,
                    ),
                    info: None,
                };
            }
            recovery_attempted = true;

            log::debug!(
                "Probe query hit stall-like USB error, attempting one reset/reopen recovery bus {} addr {} (VID:0x{:04X} PID:0x{:04X}) strategy={:?}",
                candidate.bus,
                candidate.address,
                candidate.vid,
                candidate.pid,
                strategy,
            );

            let recovered = match controller.into_session().reset_and_reopen() {
                Ok(session) => session,
                Err(reset_err) => {
                    log::debug!(
                        "Probe recovery reset/reopen failed bus {} addr {} (VID:0x{:04X} PID:0x{:04X}): {}",
                        candidate.bus,
                        candidate.address,
                        candidate.vid,
                        candidate.pid,
                        reset_err
                    );
                    return ProbeCandidateResult {
                        attempt: finalize_attempt(
                            candidate,
                            strategy,
                            ProbeOutcome::RecoveryFailed,
                            None,
                            Some(reset_err.to_string()),
                            true,
                            started,
                        ),
                        info: None,
                    };
                }
            };

            let mut recovered_controller = CommandController::new(recovered);
            match query_usb_version_for_candidate(&mut recovered_controller, strategy) {
                Ok(info) => info,
                Err(retry_err) => {
                    log::debug!(
                        "Probe recovery query failed bus {} addr {} (VID:0x{:04X} PID:0x{:04X}): {}",
                        candidate.bus,
                        candidate.address,
                        candidate.vid,
                        candidate.pid,
                        retry_err
                    );
                    return ProbeCandidateResult {
                        attempt: finalize_attempt(
                            candidate,
                            strategy,
                            ProbeOutcome::RecoveryFailed,
                            None,
                            Some(retry_err.to_string()),
                            true,
                            started,
                        ),
                        info: None,
                    };
                }
            }
        }
    };

    let Some(definition) = registry.find_by_id(usb_version.device_id_i32()) else {
        log::debug!(
            "Probe bus {} addr {} (VID:0x{:04X} PID:0x{:04X}) strategy={:?} reported unknown device ID {}",
            candidate.bus,
            candidate.address,
            candidate.vid,
            candidate.pid,
            strategy,
            usb_version.device_id
        );
        return ProbeCandidateResult {
            attempt: finalize_attempt(
                candidate,
                strategy,
                ProbeOutcome::UnknownDeviceId,
                None,
                Some(format!("unknown device id {}", usb_version.device_id)),
                recovery_attempted,
                started,
            ),
            info: None,
        };
    };

    let info = device_info_from_definition(definition, candidate);
    ProbeCandidateResult {
        attempt: finalize_attempt(
            candidate,
            strategy,
            ProbeOutcome::Identified,
            Some(info.device_id),
            None,
            recovery_attempted,
            started,
        ),
        info: Some(info),
    }
}

fn registry_supports_candidate(registry: &DeviceRegistry, candidate: UsbCandidate) -> bool {
    registry.supports_runtime_vid_pid(candidate.vid, candidate.pid)
}

fn registry_has_canonical_pid(registry: &DeviceRegistry, candidate: UsbCandidate) -> bool {
    !registry
        .find_by_vid_pid(candidate.vid, candidate.pid)
        .is_empty()
}

fn sort_candidates_by_active_hint(
    candidates: &mut [UsbCandidate],
    active: Option<&ActivePathState>,
) {
    candidates.sort_by_key(|candidate| {
        (
            active.is_none_or(|state| !candidate_matches_active_path(*candidate, state)),
            candidate.bus,
            candidate.address,
            candidate.pid,
        )
    });
}

fn build_probe_targets(
    canonical_candidates: &[UsbCandidate],
    alias_candidates: &[UsbCandidate],
    active: Option<&ActivePathState>,
) -> Vec<ProbeTarget> {
    let alias_first = active.is_some_and(|state| candidate_list_contains(alias_candidates, state));
    let mut targets = Vec::with_capacity(canonical_candidates.len() + alias_candidates.len());

    if alias_first {
        targets.extend(
            alias_candidates
                .iter()
                .copied()
                .map(|candidate| ProbeTarget {
                    candidate,
                    strategy: ProbeStrategy::AliasDongle,
                }),
        );
        targets.extend(
            canonical_candidates
                .iter()
                .copied()
                .map(|candidate| ProbeTarget {
                    candidate,
                    strategy: ProbeStrategy::Canonical,
                }),
        );
    } else {
        targets.extend(
            canonical_candidates
                .iter()
                .copied()
                .map(|candidate| ProbeTarget {
                    candidate,
                    strategy: ProbeStrategy::Canonical,
                }),
        );
        targets.extend(
            alias_candidates
                .iter()
                .copied()
                .map(|candidate| ProbeTarget {
                    candidate,
                    strategy: ProbeStrategy::AliasDongle,
                }),
        );
    }

    targets
}

fn candidate_matches_active_path(candidate: UsbCandidate, state: &ActivePathState) -> bool {
    candidate.bus == state.bus
        && candidate.address == state.address
        && candidate.vid == state.vid
        && candidate.pid == state.pid
}

fn candidate_list_contains(candidates: &[UsbCandidate], state: &ActivePathState) -> bool {
    candidates
        .iter()
        .copied()
        .any(|candidate| candidate_matches_active_path(candidate, state))
}

fn should_attempt_stall_recovery(err: &TransportError, strategy: ProbeStrategy) -> bool {
    match err {
        TransportError::Usb(message) => {
            let lower = message.to_ascii_lowercase();
            lower.contains("pipe") || lower.contains("timeout") || lower.contains("timed out")
        }
        // Dongle alias paths can return repeated zero-echo placeholders while IF2 is
        // in a bad state. Treat this as recoverable once via reset/reopen.
        TransportError::EchoMismatch { actual, .. } => {
            strategy == ProbeStrategy::AliasDongle && *actual == 0x00
        }
        _ => false,
    }
}

fn query_usb_version_for_candidate(
    controller: &mut CommandController,
    strategy: ProbeStrategy,
) -> Result<crate::usb::UsbVersionInfo, TransportError> {
    match strategy {
        ProbeStrategy::Canonical => controller.query_usb_version_discovery(),
        ProbeStrategy::AliasDongle => controller.query_usb_version_discovery_dongle(),
    }
}

fn finalize_attempt(
    candidate: UsbCandidate,
    strategy: ProbeStrategy,
    outcome: ProbeOutcome,
    resolved_device_id: Option<i32>,
    error: Option<String>,
    recovery_attempted: bool,
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
        recovery_attempted,
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

#[cfg(test)]
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

    let strategy = if registry_has_canonical_pid(registry, candidate) {
        ProbeStrategy::Canonical
    } else {
        ProbeStrategy::AliasDongle
    };

    let result = probe_candidate_identity(registry, candidate, strategy);
    if result.info.is_none() {
        log::debug!(
            "probe_device_at: bus {} addr {} strategy={:?} outcome={:?} err={}",
            bus,
            address,
            result.attempt.strategy,
            result.attempt.outcome,
            result.attempt.error.as_deref().unwrap_or("n/a")
        );
    }

    Ok(result.info)
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
        let registry =
            monsgeek_protocol::DeviceRegistry::load_from_directory(&protocol_devices_dir())
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
        let registry =
            monsgeek_protocol::DeviceRegistry::load_from_directory(&protocol_devices_dir())
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
        let registry =
            monsgeek_protocol::DeviceRegistry::load_from_directory(&protocol_devices_dir())
                .expect("device registry should load");
        let candidate = UsbCandidate {
            vid: 0x3151,
            pid: 0x4011,
            bus: 0,
            address: 0,
        };
        let matched =
            super::unique_runtime_match(&registry, candidate).expect("should match alias");
        assert_eq!(matched.id, 1308);
    }

    #[test]
    fn build_probe_targets_keeps_alias_when_canonical_exists() {
        let canonical = [UsbCandidate {
            vid: 0x3151,
            pid: 0x4015,
            bus: 3,
            address: 10,
        }];
        let alias = [UsbCandidate {
            vid: 0x3151,
            pid: 0x4011,
            bus: 3,
            address: 11,
        }];

        let targets = build_probe_targets(&canonical, &alias, None);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].strategy, ProbeStrategy::Canonical);
        assert_eq!(targets[1].strategy, ProbeStrategy::AliasDongle);
    }

    #[test]
    fn build_probe_targets_prefers_alias_when_active_hint_matches_alias() {
        let canonical = [UsbCandidate {
            vid: 0x3151,
            pid: 0x4015,
            bus: 3,
            address: 10,
        }];
        let alias = [UsbCandidate {
            vid: 0x3151,
            pid: 0x4011,
            bus: 3,
            address: 11,
        }];
        let active = ActivePathState {
            bus: 3,
            address: 11,
            vid: 0x3151,
            pid: 0x4011,
            updated_at_unix_ms: 0,
        };

        let targets = build_probe_targets(&canonical, &alias, Some(&active));
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].strategy, ProbeStrategy::AliasDongle);
        assert_eq!(targets[1].strategy, ProbeStrategy::Canonical);
    }
}
