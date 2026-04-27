use monsgeek_protocol::ChecksumType;
use monsgeek_transport::{TransportError, TransportHandle};

pub trait BridgeTransport: Send + Sync {
    fn send_fire_and_forget(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<(), TransportError>;
    fn query_command(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError>;
    fn query_raw(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError>;
    fn read_feature_report(&self) -> Result<[u8; 64], TransportError>;
}

impl BridgeTransport for TransportHandle {
    fn send_fire_and_forget(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<(), TransportError> {
        TransportHandle::send_fire_and_forget(self, cmd, data, checksum)
    }

    fn query_command(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError> {
        TransportHandle::query_command(self, cmd, data, checksum)
    }

    fn query_raw(
        &self,
        cmd: u8,
        data: &[u8],
        checksum: ChecksumType,
    ) -> Result<[u8; 64], TransportError> {
        TransportHandle::query_raw(self, cmd, data, checksum)
    }

    fn read_feature_report(&self) -> Result<[u8; 64], TransportError> {
        TransportHandle::read_feature_report(self)
    }
}

pub async fn send_command(
    handle: TransportHandle,
    data: Vec<u8>,
    checksum: ChecksumType,
) -> Result<(), String> {
    send_command_with(handle, data, checksum).await
}

pub async fn send_command_with<T>(
    handle: T,
    data: Vec<u8>,
    checksum: ChecksumType,
) -> Result<(), String>
where
    T: BridgeTransport + 'static,
{
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

pub async fn query_command(
    handle: TransportHandle,
    data: Vec<u8>,
    checksum: ChecksumType,
) -> Result<Vec<u8>, String> {
    query_command_with(handle, data, checksum).await
}

pub async fn query_raw_command(
    handle: TransportHandle,
    data: Vec<u8>,
    checksum: ChecksumType,
) -> Result<Vec<u8>, String> {
    query_raw_command_with(handle, data, checksum).await
}

pub async fn query_command_with<T>(
    handle: T,
    data: Vec<u8>,
    checksum: ChecksumType,
) -> Result<Vec<u8>, String>
where
    T: BridgeTransport + 'static,
{
    if data.is_empty() {
        return Err("empty command data".to_string());
    }

    let cmd = data[0];
    let payload = data[1..].to_vec();

    tokio::task::spawn_blocking(move || handle.query_command(cmd, &payload, checksum))
        .await
        .map_err(|e| format!("transport task join failed: {e}"))?
        .map(|bytes| bytes.to_vec())
        .map_err(|e| e.to_string())
}

pub async fn query_raw_command_with<T>(
    handle: T,
    data: Vec<u8>,
    checksum: ChecksumType,
) -> Result<Vec<u8>, String>
where
    T: BridgeTransport + 'static,
{
    if data.is_empty() {
        return Err("empty command data".to_string());
    }

    let cmd = data[0];
    let payload = data[1..].to_vec();

    tokio::task::spawn_blocking(move || handle.query_raw(cmd, &payload, checksum))
        .await
        .map_err(|e| format!("transport task join failed: {e}"))?
        .map(|bytes| bytes.to_vec())
        .map_err(|e| e.to_string())
}

pub async fn read_response(handle: TransportHandle) -> Result<Vec<u8>, String> {
    read_response_with(handle).await
}

pub async fn read_response_with<T>(handle: T) -> Result<Vec<u8>, String>
where
    T: BridgeTransport + 'static,
{
    tokio::task::spawn_blocking(move || handle.read_feature_report())
        .await
        .map_err(|e| format!("transport task join failed: {e}"))?
        .map(|bytes| bytes.to_vec())
        .map_err(|e| e.to_string())
}
