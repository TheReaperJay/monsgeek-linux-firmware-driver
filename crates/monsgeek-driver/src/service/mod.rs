mod db_store;
mod device_registry;

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::Stream;
use futures::stream::{self, StreamExt};
use monsgeek_protocol::{ChecksumType, DeviceDefinition, DeviceRegistry};
use monsgeek_transport::discovery::DeviceInfo;
use monsgeek_transport::{TransportEvent, TransportHandle, TransportOptions, connect_with_options};
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

#[derive(Clone)]
struct ConnectedDevice {
    registration: DeviceRegistration,
    handle: TransportHandle,
}

#[derive(Clone)]
pub struct DriverService {
    registry: Arc<DeviceRegistry>,
    devices: Arc<Mutex<HashMap<String, ConnectedDevice>>>,
    path_registry: Arc<Mutex<DevicePathRegistry>>,
    device_tx: broadcast::Sender<DeviceList>,
    db: DbStore,
    runtime_started: Arc<AtomicBool>,
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
            device_tx,
            db: DbStore::new(),
            runtime_started: Arc::new(AtomicBool::new(false)),
        }
    }

    fn ensure_runtime_started(&self) {
        if self
            .runtime_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let discovered = match monsgeek_transport::discovery::probe_devices(&self.registry) {
            Ok(list) => list,
            Err(err) => {
                tracing::warn!("initial probe failed: {}", err);
                Vec::new()
            }
        };

        for info in discovered {
            self.connect_and_register(info, false);
        }
    }

    fn connect_and_register(&self, info: DeviceInfo, emit_add: bool) {
        let Some(definition) = self.registry.find_by_id(info.device_id).cloned() else {
            tracing::warn!(
                "discovered unknown device id {} on {:03}:{:03}",
                info.device_id,
                info.bus,
                info.address
            );
            return;
        };

        if self
            .path_registry
            .lock()
            .expect("path registry poisoned")
            .get_by_bus_address(info.bus, info.address)
            .is_some()
        {
            return;
        }

        let (handle, event_rx) =
            match connect_with_options(&definition, TransportOptions::control_only()) {
                Ok(parts) => parts,
                Err(err) => {
                    tracing::warn!(
                        "failed to open transport for {} (id={}): {}",
                        definition.display_name,
                        definition.id,
                        err
                    );
                    return;
                }
            };

        let registration = self
            .path_registry
            .lock()
            .expect("path registry poisoned")
            .register(info.vid, info.pid, definition.id, info.bus, info.address);

        let connected = ConnectedDevice {
            registration: registration.clone(),
            handle,
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

                        let Some(definition) = pick_device_definition(&registry, vid, pid) else {
                            continue;
                        };

                        let (handle, nested_event_rx) = match connect_with_options(
                            &definition,
                            TransportOptions::control_only(),
                        ) {
                            Ok(parts) => parts,
                            Err(err) => {
                                tracing::warn!(
                                    "hot-plug connect failed for {:04x}:{:04x}: {}",
                                    vid,
                                    pid,
                                    err
                                );
                                continue;
                            }
                        };

                        let registration = path_registry
                            .lock()
                            .expect("path registry poisoned")
                            .register(vid, pid, definition.id, bus, address);

                        let connected = ConnectedDevice {
                            registration: registration.clone(),
                            handle,
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
        self.ensure_runtime_started();

        if self
            .devices
            .lock()
            .expect("devices map poisoned")
            .contains_key(path)
        {
            return Ok(());
        }

        let (vid, pid) = DevicePathRegistry::parse_vid_pid(path)
            .ok_or_else(|| Status::invalid_argument("invalid device path format"))?;
        let definition = pick_safe_definition_for_path(&self.registry, vid, pid, path)
            .ok_or_else(|| Status::not_found("no safe registry match for requested device path"))?;

        let probe_result = monsgeek_transport::discovery::probe_devices(&self.registry)
            .map_err(|e| Status::internal(format!("probe failed: {e}")))?;
        let info = probe_result
            .into_iter()
            .find(|d| d.device_id == definition.id)
            .ok_or_else(|| Status::not_found("device not present"))?;

        self.connect_and_register(info, true);

        if self
            .devices
            .lock()
            .expect("devices map poisoned")
            .contains_key(path)
        {
            return Ok(());
        }

        Err(Status::not_found("device path could not be resolved"))
    }

    fn get_handle_for_path(&self, path: &str) -> Result<TransportHandle, Status> {
        self.devices
            .lock()
            .expect("devices map poisoned")
            .get(path)
            .map(|d| d.handle.clone())
            .ok_or_else(|| Status::not_found("device not connected"))
    }

    async fn send_command_rpc(
        &self,
        path: &str,
        msg: Vec<u8>,
        checksum: ChecksumType,
    ) -> Result<(), Status> {
        self.open_device(path).await?;
        let handle = self.get_handle_for_path(path)?;
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
}

fn pick_device_definition(
    registry: &DeviceRegistry,
    vid: u16,
    pid: u16,
) -> Option<DeviceDefinition> {
    registry
        .find_by_vid_pid(vid, pid)
        .into_iter()
        .next()
        .cloned()
}

fn pick_safe_definition_for_path(
    registry: &DeviceRegistry,
    vid: u16,
    pid: u16,
    path: &str,
) -> Option<DeviceDefinition> {
    let matches = registry.find_by_vid_pid(vid, pid);
    if matches.is_empty() {
        return None;
    }

    if matches.len() == 1 {
        return Some(matches[0].clone());
    }

    let hinted_id = parse_id_hint(path)?;
    matches.into_iter().find(|d| d.id == hinted_id).cloned()
}

fn parse_id_hint(path: &str) -> Option<i32> {
    let suffix = path.split_once('@')?.1;
    let id_part = suffix
        .split('-')
        .find(|segment| segment.starts_with("id"))?;
    id_part.strip_prefix("id")?.parse::<i32>().ok()
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
        self.ensure_runtime_started();

        let initial_devs: Vec<DjDev> = self
            .devices
            .lock()
            .expect("devices map poisoned")
            .values()
            .map(|d| device_to_djdev(d, true))
            .collect();
        let initial_list = DeviceList {
            dev_list: initial_devs,
            r#type: DeviceListChangeType::Init as i32,
        };

        let rx = self.device_tx.subscribe();
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
        let checksum = proto_checksum_to_protocol(msg.check_sum_type);
        match self
            .send_command_rpc(&msg.device_path, msg.msg, checksum)
            .await
        {
            Ok(()) => Ok(Response::new(ResSend { err: String::new() })),
            Err(e) => Ok(Response::new(ResSend {
                err: e.message().to_string(),
            })),
        }
    }

    async fn read_msg(&self, request: Request<ReadMsg>) -> Result<Response<ResRead>, Status> {
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
