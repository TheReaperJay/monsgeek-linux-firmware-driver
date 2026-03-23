use monsgeek_driver::pb::driver::driver_grpc_server::DriverGrpcServer;
use monsgeek_driver::DriverService;
use tonic::transport::Server;
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        println!("monsgeek-driver v{}", env!("CARGO_PKG_VERSION"));
        println!("Starts local gRPC-Web bridge on 127.0.0.1:3814");
        return Ok(());
    }

    tracing_subscriber::fmt().with_env_filter("info").init();

    let addr = "127.0.0.1:3814".parse()?;
    let service = DriverService::new();
    let grpc_service = tonic_web::enable(DriverGrpcServer::new(service));
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods(Any)
        .expose_headers(Any);

    tracing::info!("Starting monsgeek-driver on {}", addr);

    Server::builder()
        .accept_http1(true)
        .layer(cors)
        .add_service(grpc_service)
        .serve(addr)
        .await?;

    Ok(())
}
