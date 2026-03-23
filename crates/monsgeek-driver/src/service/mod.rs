use std::pin::Pin;

use futures::Stream;
use futures::stream;
use tonic::{Request, Response, Status};

use crate::pb::driver::driver_grpc_server::DriverGrpc;
use crate::pb::driver::{
    AllList, DeleteItem, DeviceList, EffectList, Empty, GetAll, GetItem, InsertDb, Item, LedFrame,
    MicrophoneMuteStatus, MuteMicrophone, OtaUpgrade, PlayEffectRequest, PlayEffectResponse,
    Progress, ReadMsg, ResRead, ResSend, SendMsg, SetLight, StopEffectRequest, SystemInfo, VenderMsg,
    Version, WeatherReq, WeatherRes, WirelessLoopStatus,
};

#[derive(Debug, Default, Clone)]
pub struct DriverService;

impl DriverService {
    pub fn new() -> Self {
        Self
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
        Ok(Response::new(Box::pin(stream::empty())))
    }

    async fn watch_system_info(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::watchSystemInfoStream>, Status> {
        Ok(Response::new(Box::pin(stream::empty())))
    }

    async fn send_raw_feature(&self, _request: Request<SendMsg>) -> Result<Response<ResSend>, Status> {
        Ok(Response::new(ResSend { err: String::new() }))
    }

    async fn read_raw_feature(&self, _request: Request<ReadMsg>) -> Result<Response<ResRead>, Status> {
        Ok(Response::new(ResRead {
            err: String::new(),
            msg: vec![],
        }))
    }

    async fn send_msg(&self, _request: Request<SendMsg>) -> Result<Response<ResSend>, Status> {
        Ok(Response::new(ResSend {
            err: "not supported yet".to_string(),
        }))
    }

    async fn read_msg(&self, _request: Request<ReadMsg>) -> Result<Response<ResRead>, Status> {
        Ok(Response::new(ResRead {
            err: "not supported yet".to_string(),
            msg: vec![],
        }))
    }

    async fn get_item_from_db(&self, _request: Request<GetItem>) -> Result<Response<Item>, Status> {
        Ok(Response::new(Item {
            value: vec![],
            err_str: String::new(),
        }))
    }

    async fn insert_db(&self, _request: Request<InsertDb>) -> Result<Response<ResSend>, Status> {
        Ok(Response::new(ResSend {
            err: "not supported yet".to_string(),
        }))
    }

    async fn delete_item_from_db(
        &self,
        _request: Request<DeleteItem>,
    ) -> Result<Response<ResSend>, Status> {
        Ok(Response::new(ResSend {
            err: "not supported yet".to_string(),
        }))
    }

    async fn get_all_keys_from_db(
        &self,
        _request: Request<GetAll>,
    ) -> Result<Response<AllList>, Status> {
        Ok(Response::new(AllList {
            data: vec![],
            err_str: String::new(),
        }))
    }

    async fn get_all_values_from_db(
        &self,
        _request: Request<GetAll>,
    ) -> Result<Response<AllList>, Status> {
        Ok(Response::new(AllList {
            data: vec![],
            err_str: String::new(),
        }))
    }

    async fn get_version(&self, _request: Request<Empty>) -> Result<Response<Version>, Status> {
        Ok(Response::new(Version {
            base_version: "0.1.0".to_string(),
            time_stamp: "2026-03-23".to_string(),
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

    async fn send_led_frame(&self, _request: Request<LedFrame>) -> Result<Response<ResSend>, Status> {
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

    async fn get_weather(&self, _request: Request<WeatherReq>) -> Result<Response<WeatherRes>, Status> {
        Ok(Response::new(WeatherRes {
            res: "not supported yet".to_string(),
        }))
    }
}
