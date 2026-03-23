use monsgeek_driver::DriverService;
use monsgeek_driver::pb::driver::driver_grpc_server::DriverGrpc;
use monsgeek_driver::pb::driver::{Empty, GetItem, InsertDb};

#[tokio::test]
async fn grpc_db_insert_get_roundtrip() {
    let service = DriverService::new();

    DriverGrpc::insert_db(
        &service,
        tonic::Request::new(InsertDb {
            db_path: "prefs".to_string(),
            key: b"profile".to_vec(),
            value: b"2".to_vec(),
        }),
    )
    .await
    .expect("insert rpc should succeed");

    let item = DriverGrpc::get_item_from_db(
        &service,
        tonic::Request::new(GetItem {
            db_path: "prefs".to_string(),
            key: b"profile".to_vec(),
        }),
    )
    .await
    .expect("get rpc should succeed")
    .into_inner();

    assert_eq!(item.err_str, "");
    assert_eq!(item.value, b"2".to_vec());
}

#[tokio::test]
async fn grpc_get_version_shape() {
    let service = DriverService::new();
    let version = DriverGrpc::get_version(&service, tonic::Request::new(Empty {}))
        .await
        .expect("get_version should succeed")
        .into_inner();

    assert!(!version.base_version.is_empty());
    assert!(!version.time_stamp.is_empty());
}
