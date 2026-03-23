mod mock_transport;

use std::time::Duration;

use mock_transport::MockTransport;
use monsgeek_driver::DriverService;
use monsgeek_driver::bridge_transport;
use monsgeek_driver::pb::driver::driver_grpc_server::{DriverGrpc, DriverGrpcServer};
use monsgeek_protocol::ChecksumType;
use tokio::sync::oneshot;
use tonic::transport::Server;
use tower_http::cors::{Any, CorsLayer};

fn free_local_addr() -> std::net::SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let addr = listener.local_addr().expect("read addr");
    drop(listener);
    addr
}

async fn start_test_server(addr: std::net::SocketAddr) -> oneshot::Sender<()> {
    let service = DriverService::new();
    let grpc = tonic_web::enable(DriverGrpcServer::new(service));
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods(Any)
        .expose_headers(Any);

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        Server::builder()
            .accept_http1(true)
            .layer(cors)
            .add_service(grpc)
            .serve_with_shutdown(addr, async move {
                let _ = rx.await;
            })
            .await
            .expect("server failed");
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    tx
}

#[test]
fn grpc_full_service_contract_present() {
    fn assert_driver_grpc_impl<T: DriverGrpc>() {}
    assert_driver_grpc_impl::<DriverService>();
}

#[tokio::test]
async fn grpc_server_starts_http1() {
    let addr = free_local_addr();
    let shutdown = start_test_server(addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .request(
            reqwest::Method::OPTIONS,
            format!("http://{addr}/driver.DriverGrpc/watchDevList"),
        )
        .header("Origin", "https://app.monsgeek.com")
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "content-type,x-grpc-web")
        .send()
        .await
        .expect("preflight request should succeed");

    assert!(resp.status().is_success());
    assert_eq!(resp.version(), reqwest::Version::HTTP_11);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn grpc_cors_headers_present() {
    let addr = free_local_addr();
    let shutdown = start_test_server(addr).await;

    let client = reqwest::Client::new();
    let resp = client
        .request(
            reqwest::Method::OPTIONS,
            format!("http://{addr}/driver.DriverGrpc/watchDevList"),
        )
        .header("Origin", "https://app.monsgeek.com")
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "content-type,x-grpc-web")
        .send()
        .await
        .expect("preflight request should succeed");

    assert!(resp.headers().contains_key("access-control-allow-origin"));
    assert!(resp.headers().contains_key("access-control-allow-methods"));
    assert!(resp.headers().contains_key("access-control-allow-headers"));

    let _ = shutdown.send(());
}

#[tokio::test]
async fn grpc_send_raw_feature_forwards() {
    let mut reply = [0u8; 64];
    reply[0] = 0x8F;
    let mock = MockTransport::with_responses(vec![reply]);

    bridge_transport::send_command_with(mock.clone(), vec![0x8F, 0xAA, 0xBB], ChecksumType::None)
        .await
        .expect("send should succeed");

    let sent = mock.sent_commands();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].cmd, 0x8F);
    assert_eq!(sent[0].payload, vec![0xAA, 0xBB]);
    assert_eq!(sent[0].checksum, ChecksumType::None);
}

#[tokio::test]
async fn grpc_read_raw_feature_returns_data() {
    let mut reply = [0u8; 64];
    reply[0] = 0x83;
    reply[1] = 0x11;
    let mock = MockTransport::with_responses(vec![reply]);

    let out = bridge_transport::read_response_with(mock)
        .await
        .expect("read should succeed");
    assert_eq!(out.len(), 64);
    assert_eq!(out[0], 0x83);
    assert_eq!(out[1], 0x11);
}

#[tokio::test]
async fn grpc_send_msg_forwards_with_checksum() {
    let mock = MockTransport::new();
    bridge_transport::send_command_with(mock.clone(), vec![0x06, 0x05], ChecksumType::Bit7)
        .await
        .expect("send should succeed");
    let sent = mock.sent_commands();
    assert_eq!(sent[0].checksum, ChecksumType::Bit7);
}
