mod db_store;
mod device_registry;
mod firmware_update;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::Stream;
use futures::stream::{self, StreamExt};
use monsgeek_protocol::{
    ChecksumType, CommandClass, CommandDispatchPolicy, CommandPolicyError, CommandPolicyErrorCode,
    CommandReadPolicy, DeviceDefinition, DeviceRegistry, evaluate_outbound_command,
    normalize_outbound_command,
};
use monsgeek_transport::discovery::{DeviceInfo, ProbeOutcome, ProbeReport, ProbeStrategy};
use monsgeek_transport::{
    TransportEvent, TransportHandle, TransportOptions, connect_at_with_options,
};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

use crate::bridge_transport;
use crate::pb::driver::driver_grpc_server::DriverGrpc;
use crate::pb::driver::{
    AllList, DeleteItem, Device, DeviceList, DeviceListChangeType, DeviceType, DjDev, EffectList,
    Empty, GetAll, GetItem, InsertDb, Item, LedFrame, MicrophoneMuteStatus, MuteMicrophone,
    OtaUpgrade, PlayEffectRequest, PlayEffectResponse, Progress, ReadMsg, ResRead, ResSend,
    SendMsg, SetLight, StopEffectRequest, SystemInfo, VenderMsg, Version, WeatherReq, WeatherRes,
    WirelessLoopStatus, dj_dev,
};
use db_store::DbStore;
use device_registry::{DevicePathRegistry, DeviceRegistration};
use firmware_update::BridgeTarget;

const DEVICE_EVENTS_CHANNEL_SIZE: usize = 32;
fn policy_error_to_status(error: &CommandPolicyError) -> Status {
    match error.code {
        CommandPolicyErrorCode::InvalidArgument => Status::invalid_argument(error.message.clone()),
        CommandPolicyErrorCode::FailedPrecondition => {
            Status::failed_precondition(error.message.clone())
        }
    }
}

/// Validate outbound command safety using protocol-owned policy.
///
/// Semantic command rules (bounds, capability gates, compatibility behavior)
/// are centralized in `monsgeek-protocol::evaluate_outbound_command`.
pub(crate) fn validate_dangerous_write(
    definition: &DeviceDefinition,
    msg: &[u8],
) -> Result<(), Status> {
    let decision = evaluate_outbound_command(definition, msg);
    if let Some(error) = decision.error.as_ref() {
        return Err(policy_error_to_status(error));
    }
    Ok(())
}

#[derive(Clone)]
struct ConnectedDevice {
    registration: DeviceRegistration,
    handle: TransportHandle,
    definition: DeviceDefinition,
}

#[derive(Clone)]
pub struct DriverService {
    registry: Arc<DeviceRegistry>,
    devices: Arc<Mutex<HashMap<String, ConnectedDevice>>>,
    path_registry: Arc<Mutex<DevicePathRegistry>>,
    opening_paths: Arc<Mutex<HashSet<String>>>,
    synthetic_reads: Arc<Mutex<HashMap<String, VecDeque<Vec<u8>>>>>,
    discovery_refreshing: Arc<AtomicBool>,
    device_tx: broadcast::Sender<DeviceList>,
    db: DbStore,
    ota_enabled: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DriverFlags {
    pub ota_enabled: bool,
}

impl DriverService {
    pub fn new() -> Self {
        Self::new_with_flags(DriverFlags::default())
    }

    pub fn new_with_flags(flags: DriverFlags) -> Self {
        let registry = match DeviceRegistry::load_from_directory(&protocol_devices_dir()) {
            Ok(r) => r,
            Err(err) => {
                tracing::error!("failed to load protocol devices: {}", err);
                DeviceRegistry::new()
            }
        };
        let (device_tx, _) = broadcast::channel(DEVICE_EVENTS_CHANNEL_SIZE);

        Self {
            registry: Arc::new(registry),
            devices: Arc::new(Mutex::new(HashMap::new())),
            path_registry: Arc::new(Mutex::new(DevicePathRegistry::new())),
            opening_paths: Arc::new(Mutex::new(HashSet::new())),
            synthetic_reads: Arc::new(Mutex::new(HashMap::new())),
            discovery_refreshing: Arc::new(AtomicBool::new(false)),
            device_tx,
            db: DbStore::new(),
            ota_enabled: flags.ota_enabled,
        }
    }

    pub fn ota_enabled(&self) -> bool {
        self.ota_enabled
    }

    /// Gracefully stop all active transport sessions and clear runtime state.
    ///
    /// This is used by process shutdown handling so transport/background threads
    /// can observe channel closure and exit naturally.
    pub fn shutdown(&self) {
        let drained: Vec<ConnectedDevice> = self
            .devices
            .lock()
            .expect("devices map poisoned")
            .drain()
            .map(|(_, connected)| connected)
            .collect();

        for connected in drained {
            connected.handle.shutdown();
        }

        self.path_registry
            .lock()
            .expect("path registry poisoned")
            .clear();
        self.opening_paths
            .lock()
            .expect("opening set poisoned")
            .clear();
        self.synthetic_reads
            .lock()
            .expect("synthetic reads map poisoned")
            .clear();
    }

    fn connect_registration(&self, registration: DeviceRegistration, emit_add: bool) {
        let Some(definition) = self.registry.find_by_id(registration.device_id).cloned() else {
            tracing::warn!(
                "discovered unknown device id {} on {:03}:{:03}",
                registration.device_id,
                registration.bus,
                registration.address
            );
            return;
        };

        let (handle, event_rx) = match connect_at_with_options(
            &definition,
            registration.bus,
            registration.address,
            TransportOptions::control_only(),
        ) {
            Ok(parts) => parts,
            Err(err) => {
                tracing::warn!(
                    "failed to open runtime transport for {} (id={}) at {:03}:{:03}: {}",
                    definition.display_name,
                    definition.id,
                    registration.bus,
                    registration.address,
                    err
                );
                return;
            }
        };

        let connected = ConnectedDevice {
            registration: registration.clone(),
            handle,
            definition,
        };

        self.devices
            .lock()
            .expect("devices map poisoned")
            .insert(registration.path.clone(), connected.clone());

        Self::spawn_transport_event_loop(
            Arc::clone(&self.registry),
            Arc::clone(&self.devices),
            Arc::clone(&self.path_registry),
            self.device_tx.clone(),
            event_rx,
        );

        if emit_add {
            let _ = self.device_tx.send(DeviceList {
                dev_list: vec![device_to_djdev(&connected, true)],
                r#type: DeviceListChangeType::Add as i32,
            });
        }
    }

    fn discover_selected_infos(&self) -> Vec<DeviceInfo> {
        match monsgeek_transport::discovery::probe_devices_with_report(&self.registry) {
            Ok(report) => {
                self.log_probe_report(&report);
                report.found
            }
            Err(err) => {
                tracing::warn!("firmware-ID probe failed: {}", err);
                Vec::new()
            }
        }
    }

    fn log_probe_report(&self, report: &ProbeReport) {
        let mut identified = 0usize;
        let mut open_failed = 0usize;
        let mut query_failed = 0usize;
        let mut recovery_failed = 0usize;
        let mut unknown_id = 0usize;

        for attempt in &report.attempts {
            match attempt.outcome {
                ProbeOutcome::Identified => identified += 1,
                ProbeOutcome::OpenFailed => open_failed += 1,
                ProbeOutcome::QueryFailed => query_failed += 1,
                ProbeOutcome::RecoveryFailed => recovery_failed += 1,
                ProbeOutcome::UnknownDeviceId => unknown_id += 1,
            }
        }

        tracing::info!(
            "probe_summary attempts={} identified={} open_failed={} query_failed={} recovery_failed={} unknown_id={} found={} active_hint={}",
            report.attempts.len(),
            identified,
            open_failed,
            query_failed,
            recovery_failed,
            unknown_id,
            report.found.len(),
            report
                .active_hint
                .as_ref()
                .map(|h| format!(
                    "{:03}:{:03}/0x{:04X}:0x{:04X}",
                    h.bus, h.address, h.vid, h.pid
                ))
                .unwrap_or_else(|| "none".to_string()),
        );

        for attempt in &report.attempts {
            let strategy = match attempt.strategy {
                ProbeStrategy::Canonical => "canonical",
                ProbeStrategy::AliasDongle => "alias_dongle",
            };
            let outcome = match attempt.outcome {
                ProbeOutcome::Identified => "identified",
                ProbeOutcome::OpenFailed => "open_failed",
                ProbeOutcome::QueryFailed => "query_failed",
                ProbeOutcome::RecoveryFailed => "recovery_failed",
                ProbeOutcome::UnknownDeviceId => "unknown_device_id",
            };
            tracing::info!(
                "probe_attempt bus={:03} addr={:03} vid=0x{:04X} pid=0x{:04X} strategy={} outcome={} recovery_attempted={} duration_ms={} resolved_device_id={} err={}",
                attempt.bus,
                attempt.address,
                attempt.vid,
                attempt.pid,
                strategy,
                outcome,
                attempt.recovery_attempted,
                attempt.duration_ms,
                attempt
                    .resolved_device_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                attempt.error.as_deref().unwrap_or("n/a"),
            );

            if attempt.strategy == ProbeStrategy::Canonical
                && attempt.outcome == ProbeOutcome::OpenFailed
                && attempt
                    .error
                    .as_deref()
                    .is_some_and(|err| err.to_ascii_lowercase().contains("resource busy"))
            {
                tracing::warn!(
                    "wired_recovery_hint bus={:03} addr={:03} vid=0x{:04X} pid=0x{:04X}: wired keyboard interface is busy. Disconnect and reconnect the wired keyboard cable, then re-run discovery. (Dongle mode does not require this step.)",
                    attempt.bus,
                    attempt.address,
                    attempt.vid,
                    attempt.pid
                );
            }
        }

        if report.found.is_empty() && !report.attempts.is_empty() {
            tracing::warn!(
                "probe_summary_empty_no_devices: attempted={} (see probe_attempt lines above)",
                report.attempts.len()
            );
        }
    }

    fn scan_registrations(&self) -> Vec<DeviceRegistration> {
        let selected = self.discover_selected_infos();
        let mut registry = self.path_registry.lock().expect("path registry poisoned");
        let mut result = Vec::new();

        for info in selected {
            let Some(definition) = self.registry.find_by_id(info.device_id) else {
                continue;
            };

            let registration =
                registry.register(info.vid, info.pid, definition.id, info.bus, info.address);
            result.push(registration);
        }

        result
    }

    async fn scan_registrations_async(&self) -> Vec<DeviceRegistration> {
        let service = self.clone();
        match tokio::task::spawn_blocking(move || service.scan_registrations()).await {
            Ok(registrations) => registrations,
            Err(err) => {
                tracing::warn!("scan_registrations task join failed: {}", err);
                Vec::new()
            }
        }
    }

    fn snapshot_known_devices(&self) -> Vec<DjDev> {
        let connected: Vec<ConnectedDevice> = {
            let devices = self.devices.lock().expect("devices map poisoned");
            devices.values().cloned().collect()
        };
        let connected_locations: std::collections::HashSet<(u8, u8)> = connected
            .iter()
            .map(|device| (device.registration.bus, device.registration.address))
            .collect();

        let mut result: Vec<DjDev> = connected
            .iter()
            .map(|connected| device_to_djdev(connected, true))
            .collect();

        let registrations = self
            .path_registry
            .lock()
            .expect("path registry poisoned")
            .all_registrations();
        for registration in registrations {
            if connected_locations.contains(&(registration.bus, registration.address)) {
                continue;
            }
            let canonical_pid = self
                .registry
                .find_by_id(registration.device_id)
                .map(|definition| definition.pid);
            result.push(registration_to_djdev(registration, canonical_pid));
        }
        result
    }

    async fn refresh_discovery_snapshot(&self) {
        let registrations = self.scan_registrations_async().await;

        let connected_locations: std::collections::HashSet<(u8, u8)> = {
            let devices = self.devices.lock().expect("devices map poisoned");
            devices
                .values()
                .map(|connected| (connected.registration.bus, connected.registration.address))
                .collect()
        };

        for registration in registrations {
            if connected_locations.contains(&(registration.bus, registration.address)) {
                continue;
            }
            self.connect_registration(registration, true);
        }
    }

    fn queue_discovery_refresh(&self) {
        if self
            .discovery_refreshing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        let service = self.clone();
        tokio::spawn(async move {
            service.refresh_discovery_snapshot().await;
            service.discovery_refreshing.store(false, Ordering::Release);
        });
    }

    fn spawn_transport_event_loop(
        registry: Arc<DeviceRegistry>,
        devices: Arc<Mutex<HashMap<String, ConnectedDevice>>>,
        path_registry: Arc<Mutex<DevicePathRegistry>>,
        device_tx: broadcast::Sender<DeviceList>,
        event_rx: crossbeam_channel::Receiver<TransportEvent>,
    ) {
        std::thread::spawn(move || {
            while let Ok(event) = event_rx.recv() {
                match event {
                    TransportEvent::DeviceLeft { bus, address } => {
                        let removed_registration = path_registry
                            .lock()
                            .expect("path registry poisoned")
                            .remove_by_bus_address(bus, address);
                        if let Some(removed_registration) = removed_registration {
                            let removed = devices
                                .lock()
                                .expect("devices map poisoned")
                                .remove(&removed_registration.path);
                            if let Some(removed) = removed {
                                let _ = device_tx.send(DeviceList {
                                    dev_list: vec![device_to_djdev(&removed, false)],
                                    r#type: DeviceListChangeType::Remove as i32,
                                });
                            }
                        }
                    }
                    TransportEvent::DeviceArrived {
                        vid,
                        pid,
                        bus,
                        address,
                    } => {
                        if path_registry
                            .lock()
                            .expect("path registry poisoned")
                            .get_by_bus_address(bus, address)
                            .is_some()
                        {
                            continue;
                        }

                        let info = match monsgeek_transport::discovery::probe_device_at(
                            &registry, bus, address,
                        ) {
                            Ok(Some(info)) => info,
                            Ok(None) => continue,
                            Err(err) => {
                                tracing::warn!(
                                    "hot-plug probe failed for {:04x}:{:04x} at {:03}:{:03}: {}",
                                    vid,
                                    pid,
                                    bus,
                                    address,
                                    err
                                );
                                continue;
                            }
                        };

                        let Some(definition) = registry.find_by_id(info.device_id).cloned() else {
                            continue;
                        };

                        let (handle, nested_event_rx) = match connect_at_with_options(
                            &definition,
                            bus,
                            address,
                            TransportOptions::control_only(),
                        ) {
                            Ok(parts) => parts,
                            Err(err) => {
                                tracing::warn!(
                                    "hot-plug runtime connect failed for {:04x}:{:04x} at {:03}:{:03}: {}",
                                    vid,
                                    pid,
                                    bus,
                                    address,
                                    err
                                );
                                continue;
                            }
                        };

                        let registration = path_registry
                            .lock()
                            .expect("path registry poisoned")
                            .register(info.vid, info.pid, definition.id, bus, address);

                        let connected = ConnectedDevice {
                            registration: registration.clone(),
                            handle,
                            definition,
                        };

                        devices
                            .lock()
                            .expect("devices map poisoned")
                            .insert(registration.path.clone(), connected.clone());

                        let _ = device_tx.send(DeviceList {
                            dev_list: vec![device_to_djdev(&connected, true)],
                            r#type: DeviceListChangeType::Add as i32,
                        });

                        Self::spawn_transport_event_loop(
                            Arc::clone(&registry),
                            Arc::clone(&devices),
                            Arc::clone(&path_registry),
                            device_tx.clone(),
                            nested_event_rx,
                        );
                    }
                    TransportEvent::InputActions { .. } => {}
                }
            }
        });
    }

    async fn open_device(&self, path: &str) -> Result<ConnectedDevice, Status> {
        if let Ok(connected) = self.find_connected_device(path) {
            return Ok(connected);
        }

        // Prefer runtime-cached registrations from watch/init/hot-plug first.
        // This avoids re-probing every vendor-matched USB device on each
        // first write/read, which can reset unrelated interfaces (e.g. 0x4011).
        let cached_registrations = self
            .path_registry
            .lock()
            .expect("path registry poisoned")
            .all_registrations();
        let registration =
            if let Some(found) = resolve_registration_for_path(path, &cached_registrations) {
                found
            } else {
                // Runtime registry can be empty during startup while async discovery
                // is still running. Perform a one-shot blocking scan only for this
                // on-demand open path.
                let scanned_registrations = self.scan_registrations_async().await;
                resolve_registration_for_path(path, &scanned_registrations)
                    .ok_or_else(|| Status::not_found("device path could not be resolved"))?
            };
        let canonical_path = registration.path.clone();

        loop {
            if let Ok(connected) = self.find_connected_device(path) {
                return Ok(connected);
            }
            if let Ok(connected) = self.find_connected_device(&canonical_path) {
                return Ok(connected);
            }

            let should_open = {
                let mut opening = self.opening_paths.lock().expect("opening set poisoned");
                if opening.contains(&canonical_path) {
                    false
                } else {
                    opening.insert(canonical_path.clone());
                    true
                }
            };

            if should_open {
                break;
            }

            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        self.connect_registration(registration, true);
        self.opening_paths
            .lock()
            .expect("opening set poisoned")
            .remove(&canonical_path);

        if let Ok(connected) = self.find_connected_device(path) {
            return Ok(connected);
        }
        self.find_connected_device(&canonical_path)
            .map_err(|_| Status::not_found("device path could not be resolved"))
    }

    fn find_connected_device(&self, path: &str) -> Result<ConnectedDevice, Status> {
        let devices = self.devices.lock().expect("devices map poisoned");
        if let Some(device) = devices.get(path) {
            return Ok(device.clone());
        }

        let hinted_id = parse_id_hint(path);
        let parsed_vid_pid = DevicePathRegistry::parse_vid_pid(path);

        if let Some((vid, pid)) = parsed_vid_pid {
            let mut by_vid_pid = devices.values().filter(|device| {
                device.registration.vid == vid
                    && device.registration.pid == pid
                    && hinted_id.is_none_or(|id| device.registration.device_id == id)
            });
            if let Some(first) = by_vid_pid.next() {
                if by_vid_pid.next().is_some() {
                    return Err(Status::failed_precondition(
                        "ambiguous device path: multiple connected devices match this VID/PID",
                    ));
                }
                return Ok(first.clone());
            }

            if let Some(id) = hinted_id {
                let mut by_vid_and_id = devices.values().filter(|device| {
                    device.registration.vid == vid && device.registration.device_id == id
                });
                if let Some(first) = by_vid_and_id.next() {
                    if by_vid_and_id.next().is_some() {
                        return Err(Status::failed_precondition(
                            "ambiguous device path: multiple connected devices share this device ID",
                        ));
                    }
                    return Ok(first.clone());
                }
            }
        }

        if let Some(id) = hinted_id {
            let mut by_id = devices
                .values()
                .filter(|device| device.registration.device_id == id);
            if let Some(first) = by_id.next() {
                if by_id.next().is_some() {
                    return Err(Status::failed_precondition(
                        "ambiguous device path: multiple connected devices share this device ID",
                    ));
                }
                return Ok(first.clone());
            }
        }

        Err(Status::not_found("device not connected"))
    }

    fn enqueue_synthetic_response(&self, path: &str, response: Vec<u8>) {
        let mut synthetic_reads = self
            .synthetic_reads
            .lock()
            .expect("synthetic reads map poisoned");
        synthetic_reads
            .entry(path.to_string())
            .or_default()
            .push_back(response);
    }

    fn enqueue_synthetic_empty_read(&self, path: &str) {
        self.enqueue_synthetic_response(path, Vec::new());
    }

    fn enqueue_synthetic_empty_read_for_aliases(&self, requested_path: &str, canonical_path: &str) {
        self.enqueue_synthetic_empty_read(requested_path);
        if canonical_path != requested_path {
            self.enqueue_synthetic_empty_read(canonical_path);
        }
    }

    fn enqueue_synthetic_response_for_aliases(
        &self,
        requested_path: &str,
        canonical_path: &str,
        response: Vec<u8>,
    ) {
        self.enqueue_synthetic_response(requested_path, response.clone());
        if canonical_path != requested_path {
            self.enqueue_synthetic_response(canonical_path, response);
        }
    }

    fn pop_synthetic_response(&self, path: &str) -> Option<Vec<u8>> {
        let mut synthetic_reads = self
            .synthetic_reads
            .lock()
            .expect("synthetic reads map poisoned");
        let queue = synthetic_reads.get_mut(path)?;
        let result = queue.pop_front();
        if queue.is_empty() {
            synthetic_reads.remove(path);
        }
        result
    }

    async fn send_command_rpc(
        &self,
        path: &str,
        msg: Vec<u8>,
        checksum: ChecksumType,
    ) -> Result<(), Status> {
        if path.trim().is_empty() {
            return Err(Status::invalid_argument("device path is empty"));
        }

        let connected = self.open_device(path).await?;
        let handle = connected.handle;
        let definition = connected.definition;
        let canonical_path = connected.registration.path;

        let decision = evaluate_outbound_command(&definition, &msg);
        if decision.read_policy == CommandReadPolicy::SyntheticEmptyRead {
            // Preserve send/read pairing for clients that always call read after send.
            self.enqueue_synthetic_empty_read_for_aliases(path, &canonical_path);
        }

        if let Some(error) = decision.error.as_ref() {
            let status = policy_error_to_status(error);
            tracing::warn!(
                "send_command_rpc: rejected command path={} canonical_path={} cmd=0x{:02X} err={}",
                path,
                canonical_path,
                msg.first().copied().unwrap_or(0),
                status
            );
            return Err(status);
        }

        if decision.dispatch == CommandDispatchPolicy::SkipTransport {
            tracing::info!(
                "send_command_rpc: policy skipped transport path={} canonical_path={} cmd=0x{:02X}",
                path,
                canonical_path,
                msg.first().copied().unwrap_or(0),
            );
            return Ok(());
        }

        let msg = normalize_outbound_command(&definition, msg);
        if decision.class == CommandClass::Query {
            let response = bridge_transport::query_command(handle, msg, checksum)
                .await
                .map_err(Status::internal)?;
            self.enqueue_synthetic_response_for_aliases(path, &canonical_path, response);
            return Ok(());
        }

        bridge_transport::send_command(handle, msg, checksum)
            .await
            .map_err(Status::internal)
    }

    async fn read_response_rpc(&self, path: &str) -> Result<Vec<u8>, Status> {
        if path.trim().is_empty() {
            return Err(Status::invalid_argument("device path is empty"));
        }

        if let Some(response) = self.pop_synthetic_response(path) {
            return Ok(response);
        }

        let connected = self.open_device(path).await?;
        let canonical_path = connected.registration.path;
        if canonical_path != path {
            if let Some(response) = self.pop_synthetic_response(&canonical_path) {
                return Ok(response);
            }
        }

        let handle = connected.handle;
        bridge_transport::read_response(handle)
            .await
            .map_err(Status::internal)
    }

    #[doc(hidden)]
    pub fn emit_device_list_for_test(&self, list: DeviceList) {
        let _ = self.device_tx.send(list);
    }
}

fn parse_id_hint(path: &str) -> Option<i32> {
    let suffix = path.split_once('@')?.1;
    let id_part = suffix
        .split('-')
        .find(|segment| segment.starts_with("id"))?;
    id_part.strip_prefix("id")?.parse::<i32>().ok()
}

fn resolve_registration_for_path(
    path: &str,
    registrations: &[DeviceRegistration],
) -> Option<DeviceRegistration> {
    if let Some(exact) = registrations
        .iter()
        .find(|registration| registration.path == path)
    {
        return Some(exact.clone());
    }

    let (vid, pid) = DevicePathRegistry::parse_vid_pid(path)?;
    let by_vid: Vec<&DeviceRegistration> = registrations
        .iter()
        .filter(|registration| registration.vid == vid)
        .collect();
    if by_vid.is_empty() {
        return None;
    }

    let hinted_id = parse_id_hint(path);
    let narrowed: Vec<&DeviceRegistration> = if let Some(id) = hinted_id {
        let by_id: Vec<&DeviceRegistration> = by_vid
            .iter()
            .copied()
            .filter(|registration| registration.device_id == id)
            .collect();
        if by_id.is_empty() { by_vid } else { by_id }
    } else {
        by_vid
    };

    let mut sorted = narrowed;
    sorted.sort_by_key(|registration| {
        (
            registration.pid != pid,
            registration.bus,
            registration.address,
        )
    });
    sorted.first().cloned().cloned()
}

fn device_to_djdev(device: &ConnectedDevice, is_online: bool) -> DjDev {
    DjDev {
        oneof_dev: Some(dj_dev::OneofDev::Dev(Device {
            dev_type: DeviceType::YzwKeyboard as i32,
            is24: device.registration.pid != device.definition.pid,
            path: device.registration.path.clone(),
            id: device.registration.device_id,
            battery: 100,
            is_online,
            vid: device.registration.vid as u32,
            pid: device.registration.pid as u32,
        })),
    }
}

fn registration_to_djdev(registration: DeviceRegistration, canonical_pid: Option<u16>) -> DjDev {
    DjDev {
        oneof_dev: Some(dj_dev::OneofDev::Dev(Device {
            dev_type: DeviceType::YzwKeyboard as i32,
            is24: canonical_pid.is_some_and(|pid| registration.pid != pid),
            path: registration.path,
            id: registration.device_id,
            battery: 100,
            is_online: true,
            vid: registration.vid as u32,
            pid: registration.pid as u32,
        })),
    }
}

fn protocol_devices_dir() -> PathBuf {
    // Deployment override for packaged installs.
    if let Ok(path) = std::env::var("MONSGEEK_DEVICE_REGISTRY_DIR") {
        return PathBuf::from(path);
    }

    // Default install location for packaged/systemd deployments.
    let installed = PathBuf::from("/usr/share/monsgeek/protocol/devices");
    if installed.is_dir() {
        return installed;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../monsgeek-protocol")
        .join("devices")
}

fn proto_checksum_to_protocol(v: i32) -> ChecksumType {
    match v {
        0 => ChecksumType::Bit7,
        1 => ChecksumType::Bit8,
        2 => ChecksumType::None,
        _ => ChecksumType::Bit7,
    }
}

#[tonic::async_trait]
impl DriverGrpc for DriverService {
    type watchDevListStream = Pin<Box<dyn Stream<Item = Result<DeviceList, Status>> + Send>>;
    type watchSystemInfoStream = Pin<Box<dyn Stream<Item = Result<SystemInfo, Status>> + Send>>;
    type upgradeOTAGATTStream = Pin<Box<dyn Stream<Item = Result<Progress, Status>> + Send>>;
    type watchVenderStream = Pin<Box<dyn Stream<Item = Result<VenderMsg, Status>> + Send>>;

    async fn watch_dev_list(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::watchDevListStream>, Status> {
        let mut initial_devs = self.snapshot_known_devices();
        let mut refreshed_synchronously = false;
        if initial_devs.is_empty() {
            tracing::info!(
                "watch_dev_list: initial snapshot empty; running synchronous discovery refresh"
            );
            self.refresh_discovery_snapshot().await;
            initial_devs = self.snapshot_known_devices();
            refreshed_synchronously = true;
        }
        if !refreshed_synchronously {
            self.queue_discovery_refresh();
        }
        let rx = self.device_tx.subscribe();
        let initial_list = DeviceList {
            dev_list: initial_devs,
            r#type: DeviceListChangeType::Init as i32,
        };

        tracing::info!(
            "watch_dev_list: sending Init with {} device(s)",
            initial_list.dev_list.len()
        );
        let initial_stream = stream::iter(std::iter::once(Ok(initial_list)));
        let updates = BroadcastStream::new(rx).filter_map(|msg| async move { msg.ok().map(Ok) });
        Ok(Response::new(Box::pin(initial_stream.chain(updates))))
    }

    async fn watch_system_info(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::watchSystemInfoStream>, Status> {
        Ok(Response::new(Box::pin(stream::empty())))
    }

    async fn send_raw_feature(
        &self,
        request: Request<SendMsg>,
    ) -> Result<Response<ResSend>, Status> {
        let msg = request.into_inner();
        match self
            .send_command_rpc(&msg.device_path, msg.msg, ChecksumType::None)
            .await
        {
            Ok(()) => Ok(Response::new(ResSend { err: String::new() })),
            Err(e) => Ok(Response::new(ResSend {
                err: e.message().to_string(),
            })),
        }
    }

    async fn read_raw_feature(
        &self,
        request: Request<ReadMsg>,
    ) -> Result<Response<ResRead>, Status> {
        let msg = request.into_inner();
        match self.read_response_rpc(&msg.device_path).await {
            Ok(data) => Ok(Response::new(ResRead {
                err: String::new(),
                msg: data,
            })),
            Err(e) => Ok(Response::new(ResRead {
                err: e.message().to_string(),
                msg: vec![],
            })),
        }
    }

    async fn send_msg(&self, request: Request<SendMsg>) -> Result<Response<ResSend>, Status> {
        let msg = request.into_inner();
        let cmd = msg.msg.first().copied().unwrap_or(0);
        tracing::info!(
            "send_msg: path={} cmd=0x{:02X} bytes={} checksum={}",
            msg.device_path,
            cmd,
            msg.msg.len(),
            msg.check_sum_type,
        );
        let checksum = proto_checksum_to_protocol(msg.check_sum_type);
        match self
            .send_command_rpc(&msg.device_path, msg.msg, checksum)
            .await
        {
            Ok(()) => Ok(Response::new(ResSend { err: String::new() })),
            Err(e) => {
                tracing::warn!("send_msg failed for path={}: {}", msg.device_path, e);
                Ok(Response::new(ResSend {
                    err: e.message().to_string(),
                }))
            }
        }
    }

    async fn read_msg(&self, request: Request<ReadMsg>) -> Result<Response<ResRead>, Status> {
        let msg = request.into_inner();
        tracing::info!("read_msg: path={}", msg.device_path);
        match self.read_response_rpc(&msg.device_path).await {
            Ok(data) => Ok(Response::new(ResRead {
                err: String::new(),
                msg: data,
            })),
            Err(e) => {
                tracing::warn!("read_msg failed for path={}: {}", msg.device_path, e);
                Ok(Response::new(ResRead {
                    err: e.message().to_string(),
                    msg: vec![],
                }))
            }
        }
    }

    async fn get_item_from_db(&self, request: Request<GetItem>) -> Result<Response<Item>, Status> {
        let req = request.into_inner();
        let value = self.db.get(&req.db_path, &req.key);
        Ok(Response::new(Item {
            value,
            err_str: String::new(),
        }))
    }

    async fn insert_db(&self, request: Request<InsertDb>) -> Result<Response<ResSend>, Status> {
        let req = request.into_inner();
        self.db.insert(req.db_path, req.key, req.value);
        Ok(Response::new(ResSend { err: String::new() }))
    }

    async fn delete_item_from_db(
        &self,
        request: Request<DeleteItem>,
    ) -> Result<Response<ResSend>, Status> {
        let req = request.into_inner();
        self.db.delete(&req.db_path, &req.key);
        Ok(Response::new(ResSend { err: String::new() }))
    }

    async fn get_all_keys_from_db(
        &self,
        request: Request<GetAll>,
    ) -> Result<Response<AllList>, Status> {
        let req = request.into_inner();
        Ok(Response::new(AllList {
            data: self.db.all_keys(&req.db_path),
            err_str: String::new(),
        }))
    }

    async fn get_all_values_from_db(
        &self,
        request: Request<GetAll>,
    ) -> Result<Response<AllList>, Status> {
        let req = request.into_inner();
        Ok(Response::new(AllList {
            data: self.db.all_values(&req.db_path),
            err_str: String::new(),
        }))
    }

    async fn get_version(&self, _request: Request<Empty>) -> Result<Response<Version>, Status> {
        Ok(Response::new(Version {
            base_version: env!("CARGO_PKG_VERSION").to_string(),
            time_stamp: "2026-03-24".to_string(),
        }))
    }

    async fn upgrade_otagatt(
        &self,
        request: Request<OtaUpgrade>,
    ) -> Result<Response<Self::upgradeOTAGATTStream>, Status> {
        if !self.ota_enabled {
            return Err(Status::failed_precondition(
                "OTA disabled; start monsgeek-driver with --enable-ota",
            ));
        }

        let req = request.into_inner();
        let target = BridgeTarget {
            device_id: parse_id_hint(&req.dev_path)
                .and_then(|id| u32::try_from(id).ok())
                .unwrap_or(1308),
            model_slug: "monsgeek-m5w".to_string(),
            device_path: req.dev_path.clone(),
        };
        let updates = firmware_update::stream_progress(req, target);
        let stream = stream::iter(updates.into_iter().map(Ok));
        Ok(Response::new(Box::pin(stream)))
    }

    async fn mute_microphone(
        &self,
        _request: Request<MuteMicrophone>,
    ) -> Result<Response<ResSend>, Status> {
        Ok(Response::new(ResSend {
            err: "not supported yet".to_string(),
        }))
    }

    async fn toggle_microphone_mute(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<MicrophoneMuteStatus>, Status> {
        Ok(Response::new(MicrophoneMuteStatus {
            is_mute: false,
            err: "not supported yet".to_string(),
        }))
    }

    async fn get_microphone_mute(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<MicrophoneMuteStatus>, Status> {
        Ok(Response::new(MicrophoneMuteStatus {
            is_mute: false,
            err: "not supported yet".to_string(),
        }))
    }

    async fn change_wireless_loop_status(
        &self,
        _request: Request<WirelessLoopStatus>,
    ) -> Result<Response<ResSend>, Status> {
        Ok(Response::new(ResSend {
            err: "not supported yet".to_string(),
        }))
    }

    async fn set_light_type(&self, _request: Request<SetLight>) -> Result<Response<Empty>, Status> {
        Ok(Response::new(Empty {}))
    }

    async fn send_led_frame(
        &self,
        _request: Request<LedFrame>,
    ) -> Result<Response<ResSend>, Status> {
        Ok(Response::new(ResSend {
            err: "not supported yet".to_string(),
        }))
    }

    async fn play_effect(
        &self,
        _request: Request<PlayEffectRequest>,
    ) -> Result<Response<PlayEffectResponse>, Status> {
        Ok(Response::new(PlayEffectResponse {
            err: "not supported yet".to_string(),
            effect_id: 0,
        }))
    }

    async fn stop_effect(
        &self,
        _request: Request<StopEffectRequest>,
    ) -> Result<Response<ResSend>, Status> {
        Ok(Response::new(ResSend {
            err: "not supported yet".to_string(),
        }))
    }

    async fn list_effects(&self, _request: Request<Empty>) -> Result<Response<EffectList>, Status> {
        Ok(Response::new(EffectList { effects: vec![] }))
    }

    async fn watch_vender(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::watchVenderStream>, Status> {
        Ok(Response::new(Box::pin(stream::empty())))
    }

    async fn get_weather(
        &self,
        _request: Request<WeatherReq>,
    ) -> Result<Response<WeatherRes>, Status> {
        Ok(Response::new(WeatherRes {
            res: "not supported yet".to_string(),
        }))
    }
}

impl Default for DriverService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::pb::driver::driver_grpc_server::DriverGrpc;
    use crate::service::DriverService;
    use crate::service::device_registry::DevicePathRegistry;

    #[test]
    fn synthetic_path_registry_roundtrip() {
        let mut registry = DevicePathRegistry::new();
        let reg = registry.register(0x3151, 0x4015, 1308, 3, 15);
        let found = registry.get_by_bus_address(3, 15).unwrap();
        assert_eq!(found.path, reg.path);
        assert_eq!(found.device_id, 1308);
    }

    #[test]
    fn resolve_registration_prefers_exact_path() {
        let registrations = vec![
            crate::service::device_registry::DeviceRegistration {
                path: "3151-4015-ffff-0002-1@id1308-b003-a003-n1".to_string(),
                device_id: 1308,
                vid: 0x3151,
                pid: 0x4015,
                bus: 3,
                address: 3,
            },
            crate::service::device_registry::DeviceRegistration {
                path: "3151-4011-ffff-0002-1@id1308-b003-a006-n2".to_string(),
                device_id: 1308,
                vid: 0x3151,
                pid: 0x4011,
                bus: 3,
                address: 6,
            },
        ];

        let resolved =
            super::resolve_registration_for_path(&registrations[0].path, &registrations).unwrap();
        assert_eq!(resolved.path, registrations[0].path);
    }

    #[test]
    fn resolve_registration_can_recover_from_stale_pid_path() {
        let registrations = vec![crate::service::device_registry::DeviceRegistration {
            path: "3151-4015-ffff-0002-1@id1308-b003-a003-n5".to_string(),
            device_id: 1308,
            vid: 0x3151,
            pid: 0x4015,
            bus: 3,
            address: 3,
        }];

        let stale_dongle_path = "3151-4011-ffff-0002-1@id1308-b003-a006-n1";
        let resolved = super::resolve_registration_for_path(stale_dongle_path, &registrations)
            .expect("stale path should resolve by id hint");
        assert_eq!(resolved.pid, 0x4015);
        assert_eq!(resolved.bus, 3);
        assert_eq!(resolved.address, 3);
    }

    #[tokio::test]
    async fn watch_dev_list_init_message_type_is_present() {
        let service = DriverService::new();
        let _ =
            DriverGrpc::watch_dev_list(&service, tonic::Request::new(crate::pb::driver::Empty {}))
                .await;
    }

    #[test]
    fn send_raw_feature_signature_exists() {
        let _ = <DriverService as DriverGrpc>::send_raw_feature;
    }

    #[test]
    fn read_raw_feature_signature_exists() {
        let _ = <DriverService as DriverGrpc>::read_raw_feature;
    }

    #[test]
    fn send_msg_signature_exists() {
        let _ = <DriverService as DriverGrpc>::send_msg;
    }

    #[test]
    fn read_msg_signature_exists() {
        let _ = <DriverService as DriverGrpc>::read_msg;
    }

    #[test]
    fn get_version_signature_exists() {
        let _ = <DriverService as DriverGrpc>::get_version;
    }

    #[test]
    fn shutdown_clears_runtime_state() {
        let service = DriverService::new();
        {
            let mut path_registry = service
                .path_registry
                .lock()
                .expect("path registry poisoned");
            path_registry.register(0x3151, 0x4015, 1308, 3, 15);
        }
        {
            let mut opening = service.opening_paths.lock().expect("opening set poisoned");
            opening.insert("stale-path".to_string());
        }

        service.shutdown();

        assert!(
            service
                .devices
                .lock()
                .expect("devices map poisoned")
                .is_empty()
        );
        assert!(
            service
                .path_registry
                .lock()
                .expect("path registry poisoned")
                .get_by_bus_address(3, 15)
                .is_none()
        );
        assert!(
            service
                .opening_paths
                .lock()
                .expect("opening set poisoned")
                .is_empty()
        );
    }

    fn make_test_definition() -> monsgeek_protocol::DeviceDefinition {
        monsgeek_protocol::DeviceDefinition {
            id: 9999,
            vid: 0x3151,
            pid: 0x4015,
            runtime_pids: vec![],
            name: "yc3121_test".to_string(),
            display_name: "Test".to_string(),
            company: None,
            device_type: "keyboard".to_string(),
            sources: vec![],
            key_count: Some(108),
            key_layout_name: None,
            layer: Some(4),
            fn_sys_layer: Some(monsgeek_protocol::FnSysLayer { win: 2, mac: 2 }),
            magnetism: None,
            no_magnetic_switch: None,
            has_light_layout: None,
            has_side_light: None,
            hot_swap: None,
            travel_setting: None,
            led_matrix: None,
            chip_family: None,
            command_overrides: None,
        }
    }

    fn make_test_definition_with_layer(layer: u8) -> monsgeek_protocol::DeviceDefinition {
        let mut def = make_test_definition();
        def.layer = Some(layer);
        def
    }

    fn make_test_definition_with_fn_sys_layers(
        win: u8,
        mac: u8,
    ) -> monsgeek_protocol::DeviceDefinition {
        let mut def = make_test_definition();
        def.fn_sys_layer = Some(monsgeek_protocol::FnSysLayer { win, mac });
        def
    }

    fn make_set_keymatrix_msg(cmd: u8, profile: u8, key_index: u8, layer: u8) -> Vec<u8> {
        let mut msg = vec![0u8; 64];
        msg[0] = cmd;
        msg[1] = profile;
        msg[2] = key_index;
        msg[6] = layer;
        msg
    }

    #[test]
    fn test_set_keymatrix_valid_passes_through() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix;
        let msg = make_set_keymatrix_msg(cmd, 0, 50, 1);
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    #[test]
    fn test_set_keymatrix_key_index_oob_rejected() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix;
        let msg = make_set_keymatrix_msg(cmd, 0, 108, 0);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("bounds violation"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_set_keymatrix_layer_oob_rejected() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix;
        let msg = make_set_keymatrix_msg(cmd, 0, 0, 4);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("bounds violation"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_set_keymatrix_profile_oob_rejected() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix;
        let msg = make_set_keymatrix_msg(cmd, 4, 0, 0);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(err.message().contains("profile"), "got: {}", err.message());
    }

    #[test]
    fn test_set_keymatrix_short_buffer_rejected() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix;
        let msg = vec![cmd, 0, 0];
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("too short"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_non_dangerous_command_passes_through() {
        let def = make_test_definition();
        let cmd = def.commands().get_keymatrix;
        let msg = make_set_keymatrix_msg(cmd, 0, 200, 200);
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    #[test]
    fn test_set_keymatrix_boundary_valid() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix;
        let msg = make_set_keymatrix_msg(cmd, 3, 107, 3);
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    /// Build a 64-byte SIMPLE command frame with config bytes at msg[8..12].
    fn make_simple_msg(cmd: u8, profile: u8, key_index: u8, config: [u8; 4]) -> Vec<u8> {
        let mut msg = vec![0u8; 64];
        msg[0] = cmd;
        msg[1] = profile;
        msg[2] = key_index;
        msg[8] = config[0];
        msg[9] = config[1];
        msg[10] = config[2];
        msg[11] = config[3];
        msg
    }

    #[test]
    fn test_normalize_simple_reset_config() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix_simple.unwrap();
        let msg = make_simple_msg(cmd, 0, 5, [0, 0, 7, 0]);
        let result = monsgeek_protocol::normalize_outbound_command(&def, msg);
        assert_eq!(result[8], 0, "config[0] stays 0");
        assert_eq!(result[9], 7, "config[1] should now hold the keycode");
        assert_eq!(result[10], 0, "config[2] should be cleared");
        assert_eq!(result[11], 0, "config[3] stays 0");
    }

    #[test]
    fn test_normalize_simple_remap_unchanged() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix_simple.unwrap();
        let msg = make_simple_msg(cmd, 0, 5, [0, 5, 0, 0]);
        let result = monsgeek_protocol::normalize_outbound_command(&def, msg);
        assert_eq!(result[8..12], [0, 5, 0, 0]);
    }

    #[test]
    fn test_normalize_simple_forbidden_unchanged() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix_simple.unwrap();
        // All zeros — not a reset pattern (config[2] == 0).
        let msg = make_simple_msg(cmd, 0, 5, [0, 0, 0, 0]);
        let result = monsgeek_protocol::normalize_outbound_command(&def, msg);
        assert_eq!(result[8..12], [0, 0, 0, 0]);
    }

    #[test]
    fn test_normalize_simple_macro_unchanged() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix_simple.unwrap();
        // config[0]=9 means macro type — not the reset pattern.
        let msg = make_simple_msg(cmd, 0, 5, [9, 0, 7, 0]);
        let result = monsgeek_protocol::normalize_outbound_command(&def, msg);
        assert_eq!(result[8..12], [9, 0, 7, 0]);
    }

    #[test]
    fn test_normalize_simple_fn_reset_config() {
        let def = make_test_definition();
        let cmd = def.commands().set_fn_simple.unwrap();
        let msg = make_simple_msg(cmd, 0, 5, [0, 0, 7, 0]);
        let result = monsgeek_protocol::normalize_outbound_command(&def, msg);
        assert_eq!(result[9], 7);
        assert_eq!(result[10], 0);
    }

    #[test]
    fn test_normalize_non_simple_unchanged() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix; // 0x09 — NOT simple
        let msg = make_simple_msg(cmd, 0, 5, [0, 0, 7, 0]);
        let result = monsgeek_protocol::normalize_outbound_command(&def, msg);
        // Non-SIMPLE command should pass through without modification.
        assert_eq!(result[8..12], [0, 0, 7, 0]);
    }

    #[test]
    fn test_set_keymatrix_simple_bounds_validated() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix_simple.unwrap();
        // key_index 108 is OOB for 108-key device (valid range: 0..107).
        let msg = make_simple_msg(cmd, 0, 108, [0, 5, 0, 0]);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("bounds violation"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_set_fn_simple_valid_passes() {
        let def = make_test_definition();
        let cmd = def.commands().set_fn_simple.unwrap();
        let msg = make_simple_msg(cmd, 0, 50, [0, 5, 0, 0]);
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    #[test]
    fn test_set_fn_simple_profile_oob_rejected() {
        let def = make_test_definition();
        let cmd = def.commands().set_fn_simple.unwrap();
        let msg = make_simple_msg(cmd, 4, 50, [0, 5, 0, 0]);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(err.message().contains("profile"), "got: {}", err.message());
    }

    #[test]
    fn test_set_fn_simple_key_index_oob_rejected() {
        let def = make_test_definition();
        let cmd = def.commands().set_fn_simple.unwrap();
        let msg = make_simple_msg(cmd, 0, 108, [0, 5, 0, 0]);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("bounds violation"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_set_fn_simple_short_buffer_rejected() {
        let def = make_test_definition();
        let cmd = def.commands().set_fn_simple.unwrap();
        let msg = vec![cmd, 0];
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("too short"),
            "got: {}",
            err.message()
        );
    }

    // ── Test helpers: SET_MACRO, SET_FN, magnetic device definitions ────

    fn make_test_definition_magnetic() -> monsgeek_protocol::DeviceDefinition {
        monsgeek_protocol::DeviceDefinition {
            magnetism: Some(true),
            no_magnetic_switch: None,
            ..make_test_definition()
        }
    }

    fn make_test_definition_non_magnetic() -> monsgeek_protocol::DeviceDefinition {
        monsgeek_protocol::DeviceDefinition {
            magnetism: None,
            no_magnetic_switch: Some(true),
            ..make_test_definition()
        }
    }

    fn make_set_macro_msg(cmd: u8, macro_index: u8, chunk_page: u8) -> Vec<u8> {
        let mut msg = vec![0u8; 64];
        msg[0] = cmd;
        msg[1] = macro_index;
        msg[2] = chunk_page;
        msg
    }

    fn make_set_fn_msg(profile: u8, key_index: u8) -> Vec<u8> {
        let mut msg = vec![0u8; 64];
        msg[0] = monsgeek_protocol::cmd::SET_FN;
        msg[1] = 0; // fn_sys
        msg[2] = profile;
        msg[3] = key_index;
        msg
    }

    fn make_set_profile_msg(
        definition: &monsgeek_protocol::DeviceDefinition,
        profile: u8,
    ) -> Vec<u8> {
        vec![definition.commands().set_profile, profile]
    }

    // ── SET_MACRO tests ─────────────────────────────────────────────────

    #[test]
    fn test_set_macro_valid_passes() {
        let def = make_test_definition();
        let cmd = def.commands().set_macro;
        let msg = make_set_macro_msg(cmd, 0, 0);
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    #[test]
    fn test_set_macro_boundary_valid() {
        let def = make_test_definition();
        let cmd = def.commands().set_macro;
        let msg = make_set_macro_msg(cmd, 49, 9);
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    #[test]
    fn test_set_macro_index_oob_rejected() {
        let def = make_test_definition();
        let cmd = def.commands().set_macro;
        let msg = make_set_macro_msg(cmd, 50, 0);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("macro_index"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_set_macro_chunk_page_oob_rejected() {
        let def = make_test_definition();
        let cmd = def.commands().set_macro;
        let msg = make_set_macro_msg(cmd, 0, 10);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("chunk_page"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_set_macro_short_buffer_rejected() {
        let def = make_test_definition();
        let cmd = def.commands().set_macro;
        let msg = vec![cmd, 0];
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("too short"),
            "got: {}",
            err.message()
        );
    }

    // ── SET_FN tests ────────────────────────────────────────────────────

    #[test]
    fn test_set_fn_valid_passes() {
        let def = make_test_definition();
        let msg = make_set_fn_msg(0, 50);
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    #[test]
    fn test_set_fn_boundary_valid() {
        let def = make_test_definition();
        let msg = make_set_fn_msg(3, 107);
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    #[test]
    fn test_set_fn_profile_oob_rejected() {
        let def = make_test_definition();
        let msg = make_set_fn_msg(4, 50);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(err.message().contains("profile"), "got: {}", err.message());
    }

    #[test]
    fn test_set_fn_key_index_oob_rejected() {
        let def = make_test_definition();
        let msg = make_set_fn_msg(0, 108);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("bounds violation"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_set_fn_short_buffer_rejected() {
        let def = make_test_definition();
        let msg = vec![monsgeek_protocol::cmd::SET_FN, 0, 0];
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("too short"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_set_fn_fn_sys_oob_rejected() {
        let def = make_test_definition();
        let mut msg = make_set_fn_msg(0, 50);
        msg[1] = 2; // fn_sys > 1
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(err.message().contains("fn_sys"), "got: {}", err.message());
    }

    #[test]
    fn test_set_fn_fn_sys_respects_single_layer_device_limit() {
        let def = make_test_definition_with_fn_sys_layers(1, 1);
        let mut msg = make_set_fn_msg(0, 50);
        msg[1] = 1; // max is 0 when fnSysLayer is 1
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(err.message().contains("fn_sys"), "got: {}", err.message());
    }

    #[test]
    fn test_set_fn_fn_sys_accepts_two_layer_device() {
        let def = make_test_definition_with_fn_sys_layers(2, 2);
        let mut msg = make_set_fn_msg(0, 50);
        msg[1] = 1;
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    #[test]
    fn test_set_profile_valid_passes() {
        let def = make_test_definition();
        let msg = make_set_profile_msg(&def, 3);
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    #[test]
    fn test_set_profile_oob_rejected() {
        let def = make_test_definition();
        let msg = make_set_profile_msg(&def, 4);
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(err.message().contains("profile"), "got: {}", err.message());
    }

    #[test]
    fn test_set_profile_short_buffer_rejected() {
        let def = make_test_definition();
        let msg = vec![def.commands().set_profile];
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("too short"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_set_profile_respects_device_layer_limit() {
        let def = make_test_definition_with_layer(2);
        let msg = make_set_profile_msg(&def, 2); // max profile should be 1
        let err = super::validate_dangerous_write(&def, &msg).unwrap_err();
        assert!(
            err.message().contains("exceeds max 1"),
            "got: {}",
            err.message()
        );
    }

    #[test]
    fn test_set_profile_allows_five_layer_device_profile_four() {
        let def = make_test_definition_with_layer(5);
        let msg = make_set_profile_msg(&def, 4);
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    // ── Magnetic command gating tests ───────────────────────────────────

    #[test]
    fn test_magnetic_cmd_non_magnetic_device_compat_blocked() {
        let def = make_test_definition_non_magnetic();
        let mut msg = vec![0u8; 64];
        msg[0] = monsgeek_protocol::cmd::SET_MAGNETISM_CAL;
        // Compatibility behavior: blocked magnetic writes are treated as
        // send-success and completed via synthetic empty read.
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
        let decision = monsgeek_protocol::evaluate_outbound_command(&def, &msg);
        assert_eq!(
            decision.dispatch,
            monsgeek_protocol::CommandDispatchPolicy::SkipTransport
        );
        assert_eq!(
            decision.read_policy,
            monsgeek_protocol::CommandReadPolicy::SyntheticEmptyRead
        );
        assert!(decision.error.is_none());
    }

    #[test]
    fn test_magnetic_cmd_magnetic_device_passes() {
        let def = make_test_definition_magnetic();
        let mut msg = vec![0u8; 64];
        msg[0] = monsgeek_protocol::cmd::SET_MAGNETISM_CAL;
        assert!(super::validate_dangerous_write(&def, &msg).is_ok());
    }

    #[test]
    fn test_all_magnetic_cmds_gated() {
        let def = make_test_definition_non_magnetic();
        let magnetic_cmds = [
            monsgeek_protocol::cmd::SET_MAGNETISM_REPORT,
            monsgeek_protocol::cmd::SET_MAGNETISM_CAL,
            monsgeek_protocol::cmd::SET_KEY_MAGNETISM_MODE,
            monsgeek_protocol::cmd::SET_MAGNETISM_MAX_CAL,
            monsgeek_protocol::cmd::SET_MULTI_MAGNETISM,
        ];
        for &cmd_byte in &magnetic_cmds {
            let mut msg = vec![0u8; 64];
            msg[0] = cmd_byte;
            let decision = monsgeek_protocol::evaluate_outbound_command(&def, &msg);
            assert_eq!(
                decision.dispatch,
                monsgeek_protocol::CommandDispatchPolicy::SkipTransport,
                "cmd 0x{:02X} should be transport-blocked on non-magnetic device",
                cmd_byte
            );
            assert_eq!(
                decision.read_policy,
                monsgeek_protocol::CommandReadPolicy::SyntheticEmptyRead
            );
            assert!(decision.error.is_none());
        }
    }

    #[test]
    fn test_magnetic_read_cmds_not_gated() {
        let def = make_test_definition_non_magnetic();
        // GET commands should pass through even on non-magnetic devices
        // (they are read-only queries, not dangerous writes).
        let read_cmds = [
            monsgeek_protocol::cmd::GET_MULTI_MAGNETISM,
            monsgeek_protocol::cmd::GET_KEY_MAGNETISM_MODE,
        ];
        for &cmd_byte in &read_cmds {
            let mut msg = vec![0u8; 64];
            msg[0] = cmd_byte;
            assert!(
                super::validate_dangerous_write(&def, &msg).is_ok(),
                "GET cmd 0x{:02X} should pass through on non-magnetic device",
                cmd_byte
            );
        }
    }

    #[test]
    fn test_policy_synthesizes_for_blocked_magnetic_write() {
        let def = make_test_definition_non_magnetic();
        let msg = vec![monsgeek_protocol::cmd::SET_MAGNETISM_CAL];
        let decision = monsgeek_protocol::evaluate_outbound_command(&def, &msg);
        assert_eq!(
            decision.read_policy,
            monsgeek_protocol::CommandReadPolicy::SyntheticEmptyRead
        );
        assert_eq!(
            decision.dispatch,
            monsgeek_protocol::CommandDispatchPolicy::SkipTransport
        );
        assert!(decision.error.is_none());
    }

    #[test]
    fn test_policy_synthesizes_and_errors_for_invalid_write() {
        let def = make_test_definition();
        let msg = vec![def.commands().set_profile];
        let decision = monsgeek_protocol::evaluate_outbound_command(&def, &msg);
        assert_eq!(
            decision.read_policy,
            monsgeek_protocol::CommandReadPolicy::SyntheticEmptyRead
        );
        assert_eq!(
            decision.dispatch,
            monsgeek_protocol::CommandDispatchPolicy::SkipTransport
        );
        assert!(decision.error.is_some());
    }

    #[tokio::test]
    async fn db_insert_get_roundtrip() {
        let service = DriverService::new();
        let _ = DriverGrpc::insert_db(
            &service,
            tonic::Request::new(crate::pb::driver::InsertDb {
                db_path: "cfg".to_string(),
                key: b"k".to_vec(),
                value: b"v".to_vec(),
            }),
        )
        .await
        .unwrap();
        let got = DriverGrpc::get_item_from_db(
            &service,
            tonic::Request::new(crate::pb::driver::GetItem {
                db_path: "cfg".to_string(),
                key: b"k".to_vec(),
            }),
        )
        .await
        .unwrap()
        .into_inner();
        assert_eq!(got.value, b"v".to_vec());
    }

    #[tokio::test]
    async fn send_command_rpc_rejects_empty_device_path() {
        let service = DriverService::new();
        let err = service
            .send_command_rpc(
                "",
                vec![monsgeek_protocol::cmd::SET_PROFILE, 0],
                monsgeek_protocol::ChecksumType::None,
            )
            .await
            .expect_err("empty device path must be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert_eq!(err.message(), "device path is empty");
    }

    #[tokio::test]
    async fn read_response_rpc_rejects_empty_device_path() {
        let service = DriverService::new();
        let err = service
            .read_response_rpc("")
            .await
            .expect_err("empty device path must be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert_eq!(err.message(), "device path is empty");
    }
}
