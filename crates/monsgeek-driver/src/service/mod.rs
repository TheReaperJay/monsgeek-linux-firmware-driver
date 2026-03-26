mod db_store;
mod device_registry;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::Stream;
use futures::stream::{self, StreamExt};
use monsgeek_protocol::{ChecksumType, DeviceDefinition, DeviceRegistry};
use monsgeek_transport::discovery::DeviceInfo;
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

const DEVICE_EVENTS_CHANNEL_SIZE: usize = 32;
const MAX_PROFILE: u8 = 3;

/// Validate dangerous write commands before forwarding to the transport layer.
///
/// Validates SET_KEYMATRIX and SET_KEYMATRIX_SIMPLE commands against device bounds:
/// - Profile must be <= MAX_PROFILE (3)
/// - Key index and layer must be within device definition bounds
/// - Payload must be at least 7 bytes for SET_KEYMATRIX, 3 bytes for SET_KEYMATRIX_SIMPLE
///
/// Non-dangerous commands (reads, SET_LEDPARAM, SET_PROFILE, etc.) pass through
/// without validation.
pub(crate) fn validate_dangerous_write(
    definition: &DeviceDefinition,
    msg: &[u8],
) -> Result<(), Status> {
    if msg.is_empty() {
        return Ok(());
    }

    let cmd = msg[0];
    let commands = definition.commands();

    if cmd == commands.set_keymatrix {
        if msg.len() < 7 {
            return Err(Status::invalid_argument(
                "SET_KEYMATRIX payload too short: need at least 7 bytes",
            ));
        }

        let profile = msg[1];
        let key_index = msg[2] as u16;
        let layer = msg[6];

        if profile > MAX_PROFILE {
            return Err(Status::invalid_argument(format!(
                "SET_KEYMATRIX profile {} exceeds max {}",
                profile, MAX_PROFILE
            )));
        }

        monsgeek_transport::bounds::validate_write_request(definition, key_index, layer)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
    }

    if commands.set_keymatrix_simple.is_some_and(|c| c == cmd) {
        if msg.len() < 3 {
            return Err(Status::invalid_argument(
                "SET_KEYMATRIX_SIMPLE payload too short: need at least 3 bytes",
            ));
        }

        let profile = msg[1];
        let key_index = msg[2] as u16;

        if profile > MAX_PROFILE {
            return Err(Status::invalid_argument(format!(
                "SET_KEYMATRIX_SIMPLE profile {} exceeds max {}",
                profile, MAX_PROFILE
            )));
        }

        // SIMPLE commands have no layer byte; pass layer=0 for bounds check.
        monsgeek_transport::bounds::validate_write_request(definition, key_index, 0)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
    }

    Ok(())
}

/// Normalize SIMPLE keymatrix config bytes to fix the web app's reset bug.
///
/// The web app sends config bytes `[0, 0, keycode, 0]` when resetting a key to
/// its default, but the YiChip firmware's SIMPLE handler reads config[1] as the
/// keycode. This means config[1]=0 produces a dead key. This function detects
/// the reset pattern and moves the keycode from config[2] to config[1].
///
/// Only applies to SET_KEYMATRIX_SIMPLE (0x13) and SET_FN_SIMPLE (0x15).
/// Non-SIMPLE commands and non-reset configs pass through unchanged.
pub(crate) fn normalize_simple_keymatrix(
    definition: &DeviceDefinition,
    mut msg: Vec<u8>,
) -> Vec<u8> {
    if msg.len() < 12 {
        return msg;
    }

    let cmd = msg[0];
    let commands = definition.commands();

    let is_simple = commands.set_keymatrix_simple.is_some_and(|c| c == cmd)
        || commands.set_fn_simple.is_some_and(|c| c == cmd);

    if !is_simple {
        return msg;
    }

    // Config bytes are at msg[8..12] in the 64-byte HID frame.
    let config = &msg[8..12];
    if config[0] == 0 && config[1] == 0 && config[2] != 0 && config[3] == 0 {
        msg[9] = msg[10];
        msg[10] = 0;
    }

    msg
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
    device_tx: broadcast::Sender<DeviceList>,
    db: DbStore,
}

impl DriverService {
    pub fn new() -> Self {
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
            device_tx,
            db: DbStore::new(),
        }
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
        match monsgeek_transport::discovery::probe_devices(&self.registry) {
            Ok(found) => found,
            Err(err) => {
                tracing::warn!("firmware-ID probe failed: {}", err);
                Vec::new()
            }
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

    /// Discovery path used by `watchDevList`.
    ///
    /// Returns already-connected devices first (from live transport sessions),
    /// then probes USB for any new devices not yet connected. This avoids
    /// re-probing devices whose IF2 is already claimed by a running transport,
    /// which would fail and hide the device from subsequent subscribers.
    async fn scan_devices(&self) -> Vec<DjDev> {
        // Start with devices that already have a running transport.
        let mut result: Vec<DjDev> = {
            let devices = self.devices.lock().expect("devices map poisoned");
            devices
                .values()
                .map(|connected| device_to_djdev(connected, true))
                .collect()
        };

        let connected_ids: std::collections::HashSet<i32> = {
            let devices = self.devices.lock().expect("devices map poisoned");
            devices
                .values()
                .map(|connected| connected.registration.device_id)
                .collect()
        };

        // Probe USB for devices not yet connected.
        for registration in self.scan_registrations() {
            if !connected_ids.contains(&registration.device_id) {
                result.push(registration_to_djdev(registration));
            }
        }

        result
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

    async fn open_device(&self, path: &str) -> Result<(), Status> {
        if self.get_handle_for_path(path).is_ok() {
            return Ok(());
        }

        let registrations = self.scan_registrations();
        let registration = resolve_registration_for_path(path, &registrations)
            .ok_or_else(|| Status::not_found("device path could not be resolved"))?;
        let canonical_path = registration.path.clone();

        loop {
            if self.get_handle_for_path(path).is_ok()
                || self.get_handle_for_path(&canonical_path).is_ok()
            {
                return Ok(());
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

        if self.get_handle_for_path(path).is_ok()
            || self.get_handle_for_path(&canonical_path).is_ok()
        {
            return Ok(());
        }

        Err(Status::not_found("device path could not be resolved"))
    }

    fn find_connected_device(&self, path: &str) -> Result<ConnectedDevice, Status> {
        let devices = self.devices.lock().expect("devices map poisoned");
        if let Some(device) = devices.get(path) {
            return Ok(device.clone());
        }

        let hinted_id = parse_id_hint(path);
        let parsed_vid_pid = DevicePathRegistry::parse_vid_pid(path);

        if let Some((vid, pid)) = parsed_vid_pid {
            if let Some(device) = devices.values().find(|device| {
                device.registration.vid == vid
                    && device.registration.pid == pid
                    && hinted_id.is_none_or(|id| device.registration.device_id == id)
            }) {
                return Ok(device.clone());
            }

            if let Some(id) = hinted_id {
                if let Some(device) = devices.values().find(|device| {
                    device.registration.vid == vid && device.registration.device_id == id
                }) {
                    return Ok(device.clone());
                }
            }
        }

        if let Some(id) = hinted_id {
            if let Some(device) = devices
                .values()
                .find(|device| device.registration.device_id == id)
            {
                return Ok(device.clone());
            }
        }

        Err(Status::not_found("device not connected"))
    }

    fn get_handle_for_path(&self, path: &str) -> Result<TransportHandle, Status> {
        self.find_connected_device(path).map(|d| d.handle)
    }

    fn get_device_for_path(
        &self,
        path: &str,
    ) -> Result<(TransportHandle, DeviceDefinition), Status> {
        self.find_connected_device(path)
            .map(|d| (d.handle, d.definition))
    }

    async fn send_command_rpc(
        &self,
        path: &str,
        msg: Vec<u8>,
        checksum: ChecksumType,
    ) -> Result<(), Status> {
        self.open_device(path).await?;
        let (handle, definition) = self.get_device_for_path(path)?;
        validate_dangerous_write(&definition, &msg)?;
        let msg = normalize_simple_keymatrix(&definition, msg);
        bridge_transport::send_command(handle, msg, checksum)
            .await
            .map_err(Status::internal)
    }

    async fn read_response_rpc(&self, path: &str) -> Result<Vec<u8>, Status> {
        self.open_device(path).await?;
        let handle = self.get_handle_for_path(path)?;
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
            is24: device.registration.pid == 0x4011,
            path: device.registration.path.clone(),
            id: device.registration.device_id,
            battery: 100,
            is_online,
            vid: device.registration.vid as u32,
            pid: device.registration.pid as u32,
        })),
    }
}

fn registration_to_djdev(registration: DeviceRegistration) -> DjDev {
    DjDev {
        oneof_dev: Some(dj_dev::OneofDev::Dev(Device {
            dev_type: DeviceType::YzwKeyboard as i32,
            is24: registration.pid == 0x4011,
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
        let initial_devs = self.scan_devices().await;
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
        tracing::debug!(
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
        tracing::debug!("read_msg: path={}", msg.device_path);
        match self.read_response_rpc(&msg.device_path).await {
            Ok(data) => {
                Ok(Response::new(ResRead {
                    err: String::new(),
                    msg: data,
                }))
            }
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
        _request: Request<OtaUpgrade>,
    ) -> Result<Response<Self::upgradeOTAGATTStream>, Status> {
        Ok(Response::new(Box::pin(stream::empty())))
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
            name: "yc3121_test".to_string(),
            display_name: "Test".to_string(),
            company: None,
            device_type: "keyboard".to_string(),
            sources: vec![],
            key_count: Some(108),
            key_layout_name: None,
            layer: Some(4),
            fn_sys_layer: None,
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
        assert!(
            err.message().contains("profile"),
            "got: {}",
            err.message()
        );
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
        let result = super::normalize_simple_keymatrix(&def, msg);
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
        let result = super::normalize_simple_keymatrix(&def, msg);
        assert_eq!(result[8..12], [0, 5, 0, 0]);
    }

    #[test]
    fn test_normalize_simple_forbidden_unchanged() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix_simple.unwrap();
        // All zeros — not a reset pattern (config[2] == 0).
        let msg = make_simple_msg(cmd, 0, 5, [0, 0, 0, 0]);
        let result = super::normalize_simple_keymatrix(&def, msg);
        assert_eq!(result[8..12], [0, 0, 0, 0]);
    }

    #[test]
    fn test_normalize_simple_macro_unchanged() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix_simple.unwrap();
        // config[0]=9 means macro type — not the reset pattern.
        let msg = make_simple_msg(cmd, 0, 5, [9, 0, 7, 0]);
        let result = super::normalize_simple_keymatrix(&def, msg);
        assert_eq!(result[8..12], [9, 0, 7, 0]);
    }

    #[test]
    fn test_normalize_simple_fn_reset_config() {
        let def = make_test_definition();
        let cmd = def.commands().set_fn_simple.unwrap();
        let msg = make_simple_msg(cmd, 0, 5, [0, 0, 7, 0]);
        let result = super::normalize_simple_keymatrix(&def, msg);
        assert_eq!(result[9], 7);
        assert_eq!(result[10], 0);
    }

    #[test]
    fn test_normalize_non_simple_unchanged() {
        let def = make_test_definition();
        let cmd = def.commands().set_keymatrix; // 0x09 — NOT simple
        let msg = make_simple_msg(cmd, 0, 5, [0, 0, 7, 0]);
        let result = super::normalize_simple_keymatrix(&def, msg);
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
}
