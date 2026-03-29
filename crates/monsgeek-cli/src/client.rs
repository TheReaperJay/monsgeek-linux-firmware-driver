use anyhow::{Context, Result, anyhow};
use monsgeek_driver::pb::driver::driver_grpc_client::DriverGrpcClient;
use monsgeek_driver::pb::driver::{
    DangleDevType, DeviceList, DeviceListChangeType, Empty, ReadMsg, SendMsg,
};
use std::time::Duration;
use tonic::Request;
use tonic::transport::Channel;

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:3814";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const WATCH_INIT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct DriverClient {
    inner: DriverGrpcClient<Channel>,
}

impl DriverClient {
    pub async fn connect(endpoint: &str) -> Result<Self> {
        let inner = tokio::time::timeout(
            CONNECT_TIMEOUT,
            DriverGrpcClient::connect(endpoint.to_string()),
        )
        .await
        .with_context(|| {
            format!(
                "timed out after {}s while connecting to DriverGrpc at {endpoint}",
                CONNECT_TIMEOUT.as_secs()
            )
        })?
        .with_context(|| format!("failed to connect to DriverGrpc at {endpoint}"))?;
        Ok(Self { inner })
    }

    pub async fn watch_dev_list_init(&mut self) -> Result<DeviceList> {
        tokio::time::timeout(WATCH_INIT_TIMEOUT, async {
            let response = self
                .inner
                .watch_dev_list(Request::new(Empty {}))
                .await
                .context("watch_dev_list RPC failed")?;
            let mut stream = response.into_inner();
            while let Some(message) = stream
                .message()
                .await
                .context("failed to read watch_dev_list stream item")?
            {
                if message.r#type == DeviceListChangeType::Init as i32 {
                    return Ok(message);
                }
            }

            Err(anyhow!(
                "watch_dev_list stream ended before init device list was received"
            ))
        })
        .await
        .with_context(|| {
            format!(
                "timed out after {}s waiting for initial device list from DriverGrpc",
                WATCH_INIT_TIMEOUT.as_secs()
            )
        })?
        .context("watch_dev_list init flow failed")
    }

    pub async fn send_msg(
        &mut self,
        device_path: impl Into<String>,
        msg_bytes: Vec<u8>,
        checksum_enum_i32: i32,
    ) -> Result<()> {
        let response = self
            .inner
            .send_msg(Request::new(SendMsg {
                device_path: device_path.into(),
                msg: msg_bytes,
                check_sum_type: checksum_enum_i32,
                dangle_dev_type: DangleDevType::None as i32,
            }))
            .await
            .context("send_msg RPC failed")?;

        let body = response.into_inner();
        if body.err.is_empty() {
            Ok(())
        } else {
            Err(anyhow!("send_msg failed: {}", body.err))
        }
    }

    pub async fn read_msg(&mut self, device_path: impl Into<String>) -> Result<Vec<u8>> {
        let response = self
            .inner
            .read_msg(Request::new(ReadMsg {
                device_path: device_path.into(),
            }))
            .await
            .context("read_msg RPC failed")?;

        let body = response.into_inner();
        if body.err.is_empty() {
            Ok(body.msg)
        } else {
            Err(anyhow!("read_msg failed: {}", body.err))
        }
    }
}
