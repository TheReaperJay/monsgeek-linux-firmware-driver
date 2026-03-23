use std::time::Duration;

use futures::StreamExt;
use monsgeek_driver::DriverService;
use monsgeek_driver::pb::driver::driver_grpc_server::DriverGrpc;
use monsgeek_driver::pb::driver::{
    Device, DeviceList, DeviceListChangeType, DeviceType, DjDev, Empty, dj_dev,
};

#[tokio::test]
async fn grpc_watch_dev_list_init_add_remove() {
    let service = DriverService::new();
    let response = DriverGrpc::watch_dev_list(&service, tonic::Request::new(Empty {}))
        .await
        .expect("watch_dev_list should start");
    let mut stream = response.into_inner();

    let first = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout waiting init")
        .expect("stream closed")
        .expect("init error");
    assert_eq!(first.r#type, DeviceListChangeType::Init as i32);

    let fake = DjDev {
        oneof_dev: Some(dj_dev::OneofDev::Dev(Device {
            dev_type: DeviceType::YzwKeyboard as i32,
            is24: false,
            path: "3151-4015-ffff-0002-2@id1308-b003-a015-n1".to_string(),
            id: 1308,
            battery: 100,
            is_online: true,
            vid: 0x3151,
            pid: 0x4015,
        })),
    };

    service.emit_device_list_for_test(DeviceList {
        dev_list: vec![fake.clone()],
        r#type: DeviceListChangeType::Add as i32,
    });

    let add = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout waiting add")
        .expect("stream closed")
        .expect("add error");
    assert_eq!(add.r#type, DeviceListChangeType::Add as i32);
    assert_eq!(add.dev_list.len(), 1);

    service.emit_device_list_for_test(DeviceList {
        dev_list: vec![fake],
        r#type: DeviceListChangeType::Remove as i32,
    });

    let remove = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("timeout waiting remove")
        .expect("stream closed")
        .expect("remove error");
    assert_eq!(remove.r#type, DeviceListChangeType::Remove as i32);
    assert_eq!(remove.dev_list.len(), 1);
}
