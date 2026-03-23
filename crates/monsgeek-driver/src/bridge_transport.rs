use monsgeek_protocol::ChecksumType;
use monsgeek_transport::TransportHandle;

pub async fn send_command(
    handle: TransportHandle,
    data: Vec<u8>,
    checksum: ChecksumType,
) -> Result<(), String> {
    if data.is_empty() {
        return Err("empty command data".to_string());
    }

    let cmd = data[0];
    let payload = data[1..].to_vec();

    tokio::task::spawn_blocking(move || handle.send_fire_and_forget(cmd, &payload, checksum))
        .await
        .map_err(|e| format!("transport task join failed: {e}"))?
        .map_err(|e| e.to_string())
}

pub async fn read_response(handle: TransportHandle) -> Result<Vec<u8>, String> {
    tokio::task::spawn_blocking(move || handle.read_feature_report())
        .await
        .map_err(|e| format!("transport task join failed: {e}"))?
        .map(|bytes| bytes.to_vec())
        .map_err(|e| e.to_string())
}
