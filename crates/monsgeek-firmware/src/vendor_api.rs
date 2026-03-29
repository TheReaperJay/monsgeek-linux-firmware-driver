use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use futures_util::StreamExt;
use reqwest::Client;

pub const API_BASE: &str = "https://api2.rongyuan.tech:3816/api/v2";
pub const DOWNLOAD_BASE: &str = "https://api2.rongyuan.tech:3816/download";

#[derive(Debug, thiserror::Error)]
pub enum VendorApiError {
    #[error("request error: {0}")]
    Request(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("server error {code}: {message}")]
    Server { code: i32, message: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FirmwareVersions {
    pub usb: Option<u16>,
    pub rf: Option<u16>,
    pub mled: Option<u16>,
    pub nord: Option<u16>,
    pub oled: Option<u16>,
    pub flash: Option<u16>,
    pub download_path: Option<String>,
    pub raw_version: String,
}

impl FirmwareVersions {
    pub fn parse(version_str: &str) -> Self {
        let mut result = Self {
            raw_version: version_str.to_string(),
            ..Default::default()
        };

        let parts: Vec<&str> = version_str.split('_').collect();
        let mut index = 0usize;
        while index + 1 < parts.len() {
            let key = parts[index].to_ascii_lowercase();
            let value = u16::from_str_radix(parts[index + 1], 16);
            if let Ok(parsed) = value {
                match key.as_str() {
                    "usb" => result.usb = Some(parsed),
                    "rfv" | "rf" => result.rf = Some(parsed),
                    "mledv" | "mled" => result.mled = Some(parsed),
                    "nordv" | "nord" => result.nord = Some(parsed),
                    "oledv" | "oled" => result.oled = Some(parsed),
                    "flashv" | "flash" => result.flash = Some(parsed),
                    _ => {}
                }
            }
            index += 2;
        }

        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareCheckResponse {
    pub versions: FirmwareVersions,
    pub lowest_app_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    pub downloaded_bytes: usize,
    pub total_bytes: Option<u64>,
}

pub async fn check_vendor_firmware(
    device_id: u32,
) -> Result<FirmwareCheckResponse, VendorApiError> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|err| VendorApiError::Request(err.to_string()))?;

    let url = format!("{API_BASE}/get_fw_version");
    let mut body = HashMap::new();
    body.insert("dev_id", device_id);

    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|err| VendorApiError::Request(err.to_string()))?;

    if !response.status().is_success() {
        return Err(VendorApiError::Server {
            code: response.status().as_u16() as i32,
            message: response.status().to_string(),
        });
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|err| VendorApiError::Parse(err.to_string()))?;

    if let Some(err_code) = json.get("errCode").and_then(|value| value.as_i64())
        && err_code != 0
    {
        return Err(VendorApiError::Server {
            code: err_code as i32,
            message: "API error".to_string(),
        });
    }

    let data = json
        .get("data")
        .ok_or_else(|| VendorApiError::Parse("No data in response".to_string()))?;

    let version_str = data
        .get("version_str")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let mut versions = FirmwareVersions::parse(version_str);
    versions.download_path = data
        .get("path")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);

    let lowest_app_version = data
        .get("lowest_app_version_str")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);

    Ok(FirmwareCheckResponse {
        versions,
        lowest_app_version,
    })
}

pub async fn download_vendor_firmware(
    download_path: &str,
    output: impl AsRef<Path>,
) -> Result<usize, VendorApiError> {
    download_vendor_firmware_with_progress(download_path, output, |_| {}).await
}

pub async fn download_vendor_firmware_with_progress<F>(
    download_path: &str,
    output: impl AsRef<Path>,
    mut on_progress: F,
) -> Result<usize, VendorApiError>
where
    F: FnMut(DownloadProgress),
{
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|err| VendorApiError::Request(err.to_string()))?;

    let url = format!("{DOWNLOAD_BASE}{download_path}");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| VendorApiError::Request(err.to_string()))?;

    if !response.status().is_success() {
        return Err(VendorApiError::Server {
            code: response.status().as_u16() as i32,
            message: response.status().to_string(),
        });
    }

    let total_bytes = response.content_length();
    let mut downloaded_bytes = 0usize;
    let mut file = std::fs::File::create(output)?;
    let mut stream = response.bytes_stream();

    while let Some(next) = stream.next().await {
        let chunk = next.map_err(|err| VendorApiError::Request(err.to_string()))?;
        file.write_all(&chunk)?;
        downloaded_bytes += chunk.len();
        on_progress(DownloadProgress {
            downloaded_bytes,
            total_bytes,
        });
    }

    file.flush()?;
    Ok(downloaded_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_string_maps_usb_and_rf() {
        let versions = FirmwareVersions::parse("usb_665_rfv_42");
        assert_eq!(versions.usb, Some(0x665));
        assert_eq!(versions.rf, Some(0x42));
    }

    #[test]
    fn parse_version_string_ignores_unknown_pairs() {
        let versions = FirmwareVersions::parse("foo_10_usb_110_bar_22");
        assert_eq!(versions.usb, Some(0x110));
        assert_eq!(versions.oled, None);
    }
}
