use anyhow::{Context, Result, anyhow};
use monsgeek_driver::pb::driver::driver_grpc_client::DriverGrpcClient;
use monsgeek_driver::pb::driver::{
    DangleDevType, DeviceList, DeviceListChangeType, DjDev, Empty, ReadMsg, SendMsg, dj_dev,
};
use std::collections::HashMap;
use std::time::Duration;
use tonic::Request;
use tonic::transport::Channel;

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:3814";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const WATCH_INIT_TIMEOUT: Duration = Duration::from_secs(30);
const WATCH_BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(3);

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
                    let mut snapshot = message;
                    if snapshot.dev_list.is_empty() {
                        let deadline = tokio::time::Instant::now() + WATCH_BOOTSTRAP_TIMEOUT;
                        loop {
                            let now = tokio::time::Instant::now();
                            if now >= deadline {
                                break;
                            }
                            let wait = deadline - now;
                            let next = match tokio::time::timeout(wait, stream.message()).await {
                                Ok(next) => next,
                                Err(_) => break,
                            };
                            let Some(update) =
                                next.context("failed to read watch_dev_list bootstrap item")?
                            else {
                                break;
                            };
                            merge_device_list_update(&mut snapshot, update);
                            if !snapshot.dev_list.is_empty() {
                                break;
                            }
                        }
                    }
                    return Ok(snapshot);
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

fn merge_device_list_update(snapshot: &mut DeviceList, update: DeviceList) {
    match update.r#type {
        x if x == DeviceListChangeType::Init as i32 => {
            snapshot.dev_list = update.dev_list;
        }
        x if x == DeviceListChangeType::Add as i32 || x == DeviceListChangeType::Change as i32 => {
            let mut by_path: HashMap<String, DjDev> = HashMap::new();
            for entry in snapshot.dev_list.drain(..) {
                if let Some(path) = djdev_path(&entry) {
                    by_path.insert(path.to_string(), entry);
                }
            }
            for entry in update.dev_list {
                if let Some(path) = djdev_path(&entry) {
                    by_path.insert(path.to_string(), entry);
                }
            }
            let mut merged: Vec<DjDev> = by_path.into_values().collect();
            merged.sort_by_key(|entry| djdev_path(entry).map(str::to_owned).unwrap_or_default());
            snapshot.dev_list = merged;
        }
        x if x == DeviceListChangeType::Remove as i32 => {
            let removed_paths: std::collections::HashSet<String> = update
                .dev_list
                .iter()
                .filter_map(djdev_path)
                .map(ToOwned::to_owned)
                .collect();
            snapshot
                .dev_list
                .retain(|entry| djdev_path(entry).is_none_or(|path| !removed_paths.contains(path)));
        }
        _ => {}
    }
}

fn djdev_path(device: &DjDev) -> Option<&str> {
    let dj_dev::OneofDev::Dev(dev) = device.oneof_dev.as_ref()? else {
        return None;
    };
    Some(dev.path.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use monsgeek_driver::pb::driver::{Device, DeviceType};

    fn make_djdev(path: &str) -> DjDev {
        DjDev {
            oneof_dev: Some(dj_dev::OneofDev::Dev(Device {
                dev_type: DeviceType::YzwKeyboard as i32,
                is24: false,
                path: path.to_string(),
                id: 1308,
                battery: 100,
                is_online: true,
                vid: 0x3151,
                pid: 0x4015,
            })),
        }
    }

    #[test]
    fn merge_device_list_add_upserts_by_path() {
        let mut snapshot = DeviceList {
            dev_list: vec![make_djdev("a"), make_djdev("b")],
            r#type: DeviceListChangeType::Init as i32,
        };
        let update = DeviceList {
            dev_list: vec![make_djdev("b"), make_djdev("c")],
            r#type: DeviceListChangeType::Add as i32,
        };

        merge_device_list_update(&mut snapshot, update);
        let paths: Vec<String> = snapshot
            .dev_list
            .iter()
            .filter_map(djdev_path)
            .map(ToOwned::to_owned)
            .collect();
        assert_eq!(paths, vec!["a", "b", "c"]);
    }

    #[test]
    fn merge_device_list_remove_drops_paths() {
        let mut snapshot = DeviceList {
            dev_list: vec![make_djdev("a"), make_djdev("b"), make_djdev("c")],
            r#type: DeviceListChangeType::Init as i32,
        };
        let update = DeviceList {
            dev_list: vec![make_djdev("b")],
            r#type: DeviceListChangeType::Remove as i32,
        };

        merge_device_list_update(&mut snapshot, update);
        let paths: Vec<String> = snapshot
            .dev_list
            .iter()
            .filter_map(djdev_path)
            .map(ToOwned::to_owned)
            .collect();
        assert_eq!(paths, vec!["a", "c"]);
    }
}
