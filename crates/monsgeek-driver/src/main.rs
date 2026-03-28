use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use monsgeek_driver::{DriverFlags, DriverService};
use monsgeek_driver::pb::driver::driver_grpc_server::DriverGrpcServer;
use tonic::transport::Server;
use tower_http::cors::{Any, CorsLayer};

type AnyError = Box<dyn Error>;

#[tokio::main]
async fn main() -> Result<(), AnyError> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("monsgeek-driver v{}", env!("CARGO_PKG_VERSION"));
        println!("Starts local gRPC-Web bridge on 127.0.0.1:3814");
        println!("Flags:");
        println!("  --enable-ota    Enable high-risk OTA RPC endpoint (disabled by default)");
        return Ok(());
    }
    let enable_ota = args.iter().any(|arg| arg == "--enable-ota");

    tracing_subscriber::fmt().with_env_filter("info").init();

    let addr: SocketAddr = "127.0.0.1:3814".parse()?;
    let service = DriverService::new_with_flags(DriverFlags {
        ota_enabled: enable_ota,
    });

    tracing::info!("Starting monsgeek-driver on {}", addr);
    if enable_ota {
        tracing::warn!("OTA bridge endpoint is ENABLED (--enable-ota)");
    } else {
        tracing::info!("OTA bridge endpoint is disabled (default)");
    }

    serve_with_takeover(service, addr).await
}

async fn serve_with_takeover(service: DriverService, addr: SocketAddr) -> Result<(), AnyError> {
    match run_server(service.clone(), addr).await {
        Ok(()) => Ok(()),
        Err(err) if error_chain_has_addr_in_use(&err) => {
            tracing::warn!(
                "Port {} already in use; attempting monsgeek-driver takeover",
                addr.port()
            );
            take_over_existing_driver(addr.port())?;
            run_server(service, addr).await?;
            Ok(())
        }
        Err(err) => Err(Box::new(err)),
    }
}

async fn run_server(
    service: DriverService,
    addr: SocketAddr,
) -> Result<(), tonic::transport::Error> {
    let grpc_service = tonic_web::enable(DriverGrpcServer::new(service.clone()));
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods(Any)
        .expose_headers(Any);

    Server::builder()
        .accept_http1(true)
        .layer(cors)
        .add_service(grpc_service)
        .serve_with_shutdown(addr, shutdown_signal(service))
        .await
}

fn error_chain_has_addr_in_use(err: &(dyn Error + 'static)) -> bool {
    if let Some(io_err) = err.downcast_ref::<io::Error>() {
        return io_err.kind() == io::ErrorKind::AddrInUse;
    }
    err.source()
        .map(error_chain_has_addr_in_use)
        .unwrap_or(false)
}

async fn shutdown_signal(service: DriverService) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        match signal(SignalKind::terminate()) {
            Ok(mut sigterm) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        tracing::info!("Received SIGINT");
                    }
                    _ = sigterm.recv() => {
                        tracing::info!("Received SIGTERM");
                    }
                }
            }
            Err(err) => {
                tracing::warn!("Failed to register SIGTERM handler: {}", err);
                let _ = tokio::signal::ctrl_c().await;
                tracing::info!("Received SIGINT");
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Received shutdown signal");
    }

    tracing::info!("Shutting down active transport sessions");
    service.shutdown();

    // Force exit after a grace period. Tonic's graceful shutdown waits for
    // active gRPC streams (e.g. watch_dev_list) to close, but browser
    // clients never close their end. Without this, the process hangs.
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_secs(2)).await;
        tracing::info!("Grace period expired, forcing exit");
        std::process::exit(0);
    });
}

fn take_over_existing_driver(port: u16) -> Result<(), AnyError> {
    let pids = listener_pids_for_port(port)?;
    if pids.is_empty() {
        return Err(io::Error::other(format!(
            "port {} is busy but no owning PID could be resolved",
            port
        ))
        .into());
    }

    for pid in pids {
        let cmdline = read_cmdline(pid).unwrap_or_default();
        if !cmdline.contains("monsgeek-driver") {
            return Err(io::Error::other(format!(
                "port {} is owned by non-driver process pid {} ({})",
                port,
                pid,
                if cmdline.is_empty() {
                    "<unknown>"
                } else {
                    &cmdline
                }
            ))
            .into());
        }

        tracing::warn!("Stopping stale monsgeek-driver pid {} ({})", pid, cmdline);
        send_signal(pid, libc::SIGTERM)?;
        if !wait_for_exit(pid, Duration::from_millis(1500)) {
            tracing::warn!("PID {} ignored SIGTERM; sending SIGKILL", pid);
            send_signal(pid, libc::SIGKILL)?;
            if !wait_for_exit(pid, Duration::from_millis(1500)) {
                return Err(io::Error::other(format!(
                    "failed to terminate stale monsgeek-driver pid {}",
                    pid
                ))
                .into());
            }
        }
    }

    Ok(())
}

fn listener_pids_for_port(port: u16) -> io::Result<Vec<i32>> {
    let mut inodes = BTreeSet::new();
    collect_listening_inodes("/proc/net/tcp", port, &mut inodes)?;
    collect_listening_inodes("/proc/net/tcp6", port, &mut inodes)?;
    if inodes.is_empty() {
        return Ok(Vec::new());
    }

    let mut pids = BTreeSet::new();
    for proc_entry in fs::read_dir("/proc")? {
        let proc_entry = match proc_entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let pid = match proc_entry.file_name().to_string_lossy().parse::<i32>() {
            Ok(pid) => pid,
            Err(_) => continue,
        };
        let fd_dir = proc_entry.path().join("fd");
        let fd_iter = match fs::read_dir(fd_dir) {
            Ok(iter) => iter,
            Err(_) => continue,
        };

        for fd in fd_iter.flatten() {
            let target = match fs::read_link(fd.path()) {
                Ok(target) => target,
                Err(_) => continue,
            };
            let Some(inode) = socket_inode_from_target(&target) else {
                continue;
            };
            if inodes.contains(&inode) {
                pids.insert(pid);
                break;
            }
        }
    }

    Ok(pids.into_iter().collect())
}

fn collect_listening_inodes(path: &str, port: u16, out: &mut BTreeSet<u64>) -> io::Result<()> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    let expected_port = format!("{:04X}", port);
    for line in contents.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 10 {
            continue;
        }
        // 0A = TCP_LISTEN
        if cols[3] != "0A" {
            continue;
        }
        let Some((_, port_hex)) = cols[1].split_once(':') else {
            continue;
        };
        if !port_hex.eq_ignore_ascii_case(&expected_port) {
            continue;
        }
        if let Ok(inode) = cols[9].parse::<u64>() {
            out.insert(inode);
        }
    }

    Ok(())
}

fn socket_inode_from_target(target: &Path) -> Option<u64> {
    let text = target.to_string_lossy();
    let raw = text.strip_prefix("socket:[")?.strip_suffix(']')?;
    raw.parse::<u64>().ok()
}

fn read_cmdline(pid: i32) -> io::Result<String> {
    let path = PathBuf::from(format!("/proc/{pid}/cmdline"));
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(String::new());
    }
    let parts: Vec<String> = bytes
        .split(|b| *b == 0)
        .filter(|segment| !segment.is_empty())
        .map(|segment| String::from_utf8_lossy(segment).into_owned())
        .collect();
    Ok(parts.join(" "))
}

fn send_signal(pid: i32, signal: i32) -> io::Result<()> {
    // SAFETY: libc::kill does not retain pointers; pid/signal are plain integers.
    let rc = unsafe { libc::kill(pid, signal) };
    if rc == 0 {
        return Ok(());
    }
    let err = io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(err)
}

fn wait_for_exit(pid: i32, timeout: Duration) -> bool {
    let proc_path = PathBuf::from(format!("/proc/{pid}"));
    let deadline = Instant::now() + timeout;
    loop {
        if !proc_path.exists() {
            return true;
        }
        if Instant::now() >= deadline {
            return !proc_path.exists();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_inode_parser_extracts_inode() {
        let inode = socket_inode_from_target(Path::new("socket:[1234567]"));
        assert_eq!(inode, Some(1234567));
    }

    #[test]
    fn addr_in_use_detection_on_io_error() {
        let err = io::Error::new(io::ErrorKind::AddrInUse, "already in use");
        assert!(error_chain_has_addr_in_use(&err));
    }
}
